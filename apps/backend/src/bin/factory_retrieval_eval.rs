//! Retrieval eval for the factory F2 exit gate: Recall@K and MRR of code
//! retrieval on questions derived from golden tasks.
//!
//! Usage:
//!   factory-retrieval-eval --db <eval.db> --repo <snapshot> --history <git repo> \
//!     --project <name> [--variants dense,bm25,rrf] [--no-embed] < golden.jsonl
//!
//! The snapshot is indexed into the eval database (unchanged files are not
//! re-processed on later runs, so an interrupted embedding pass resumes). Each
//! golden task becomes a question: its title without the conventional-commit
//! prefix, with the files its change modified as the gold set
//! (`git diff --name-status base_sha merge_sha` in the history repo). Gold files
//! missing from the index are excluded and reported. `--no-embed` indexes without
//! the embedding model (lexical variants only). Nothing leaves the machine.

use nexusmind::{
    db::{connection::connect, migrations, queries},
    embed::EmbedService,
    retrieval::{self, eval, lexical},
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::BufRead;
use std::sync::{Arc, Mutex};

const KS: [usize; 3] = [5, 10, 20];
/// RRF constant from the original paper; robust without tuning.
const RRF_K: f64 = 60.0;

#[derive(Deserialize)]
struct GoldenTask {
    id: String,
    title: String,
    base_sha: String,
    merge_sha: String,
}

#[derive(Default)]
struct Totals {
    questions: usize,
    recall: [f64; 3],
    hits: [usize; 3],
    rr: f64,
}

fn arg(args: &[String], name: &str) -> anyhow::Result<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|at| args.get(at + 1).cloned())
        .ok_or_else(|| anyhow::anyhow!("missing {name}"))
}

fn is_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (db_path, repo, project) = (
        arg(&args, "--db")?,
        arg(&args, "--repo")?,
        arg(&args, "--project")?,
    );
    let repo = std::fs::canonicalize(&repo)?.to_string_lossy().into_owned();
    let history = arg(&args, "--history").unwrap_or_else(|_| repo.clone());
    let no_embed = args.iter().any(|a| a == "--no-embed");
    let variants: Vec<String> = arg(&args, "--variants")
        .unwrap_or_else(|_| if no_embed { "bm25" } else { "dense,bm25,rrf" }.to_string())
        .split(',')
        .map(str::to_string)
        .collect();
    for variant in &variants {
        let needs_vectors = variant != "bm25";
        if !["dense", "bm25", "rrf"].contains(&variant.as_str()) || (no_embed && needs_vectors) {
            anyhow::bail!("unsupported variant {variant}");
        }
    }

    let conn = connect(&db_path)?;
    migrations::run_all(&conn)?;
    let org_id: String =
        match conn.query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0)) {
            Ok(id) => id,
            Err(_) => {
                queries::bootstrap(&conn, "Eval", "eval", "eval@localhost", "Eval")?
                    .0
                    .id
            }
        };
    let db = Arc::new(Mutex::new(conn));
    let embed = if no_embed {
        None
    } else {
        Some(Arc::new(EmbedService::init()?))
    };

    eprintln!("indexing {repo} as {project}…");
    let started = std::time::Instant::now();
    let indexed =
        nexusmind::indexer::index_project(&org_id, &project, &repo, &db, embed.as_ref(), false)?;
    eprintln!("indexed in {:?}: {indexed:?}", started.elapsed());

    let conn = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
    let project_id: i64 = conn.query_row(
        "SELECT id FROM code_projects WHERE org_id = ?1 AND name = ?2",
        rusqlite::params![org_id, project],
        |r| r.get(0),
    )?;
    let (chunks, unembedded): (i64, i64) = conn.query_row(
        "SELECT count(*), count(*) - count(embedding) FROM code_chunks WHERE code_project_id = ?1",
        [project_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if embed.is_some() && unembedded > 0 {
        anyhow::bail!("{unembedded} of {chunks} chunks have no vector; rerun to resume embedding");
    }
    let indexed_files: BTreeSet<String> = {
        let mut stmt =
            conn.prepare("SELECT DISTINCT file_path FROM code_chunks WHERE code_project_id = ?1")?;
        let rows = stmt.query_map([project_id], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<_, _>>()?
    };

    let mut totals: BTreeMap<String, Totals> = BTreeMap::new();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let task: GoldenTask = serde_json::from_str(&line)?;
        if !is_sha(&task.base_sha) || !is_sha(&task.merge_sha) {
            anyhow::bail!("task {}: invalid sha", task.id);
        }
        let diff = std::process::Command::new("git")
            .args(["-C", &history, "diff", "--name-status", "-M"])
            .args([&task.base_sha, &task.merge_sha])
            .output()?;
        if !diff.status.success() {
            println!(
                "{}",
                serde_json::json!({"id": task.id, "error": "commits_missing"})
            );
            continue;
        }
        let modified = eval::modified_files(&String::from_utf8_lossy(&diff.stdout));
        let (gold, excluded): (BTreeSet<String>, BTreeSet<String>) = modified
            .into_iter()
            .partition(|f| indexed_files.contains(f));
        let query = eval::query_from_title(&task.title);
        if gold.is_empty() {
            println!(
                "{}",
                serde_json::json!({"id": task.id, "query": query, "skipped": "no_indexed_gold", "excluded": excluded})
            );
            continue;
        }
        let paths = |hits: Vec<retrieval::FileHit>| -> Vec<String> {
            hits.into_iter().map(|h| h.file_path).collect()
        };
        let lexical_ranking = paths(lexical::bm25_file_ranking(&conn, project_id, &query)?);
        let dense_ranking = match &embed {
            Some(svc) => paths(retrieval::dense_file_ranking(
                &conn,
                project_id,
                &query,
                &svc.embed_query(&query)?,
            )?),
            None => Vec::new(),
        };
        for variant in &variants {
            let ranked = match variant.as_str() {
                "dense" => dense_ranking.clone(),
                "bm25" => lexical_ranking.clone(),
                _ => retrieval::reciprocal_rank_fusion(
                    &[dense_ranking.clone(), lexical_ranking.clone()],
                    RRF_K,
                ),
            };
            let recalls = KS.map(|k| eval::recall_at(&ranked, &gold, k));
            let hits = KS.map(|k| eval::hit_at(&ranked, &gold, k));
            let rr = eval::reciprocal_rank(&ranked, &gold);
            let total = totals.entry(variant.clone()).or_default();
            total.questions += 1;
            for (sum, value) in total.recall.iter_mut().zip(recalls) {
                *sum += value;
            }
            for (sum, hit) in total.hits.iter_mut().zip(hits) {
                *sum += usize::from(hit);
            }
            total.rr += rr;
            println!(
                "{}",
                serde_json::json!({
                    "id": task.id, "query": query, "variant": variant,
                    "gold": gold, "excluded": excluded,
                    "recall@5": recalls[0], "recall@10": recalls[1], "recall@20": recalls[2],
                    "hit@5": hits[0], "hit@10": hits[1], "hit@20": hits[2],
                    "rr": rr, "top5": &ranked[..ranked.len().min(5)],
                })
            );
        }
    }
    for (variant, total) in &totals {
        let n = total.questions.max(1) as f64;
        println!(
            "{}",
            serde_json::json!({"summary": {
                "project": project, "variant": variant, "questions": total.questions,
                "recall@5": total.recall[0] / n, "recall@10": total.recall[1] / n,
                "recall@20": total.recall[2] / n,
                "hit@5": total.hits[0] as f64 / n, "hit@10": total.hits[1] as f64 / n,
                "hit@20": total.hits[2] as f64 / n, "mrr": total.rr / n,
            }})
        );
    }
    Ok(())
}
