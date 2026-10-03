//! Code retrieval ("Bibliotecario", factory F2): rankers over an indexed code
//! project, shared by the code search API, the ContextPack builder and the
//! retrieval eval.

pub mod context_pack;
pub mod eval;
pub mod graph;
pub mod lexical;
pub mod rerank;

use rusqlite::Connection;

/// One ranked file: its best chunk's score and symbol.
#[derive(Clone, Debug, PartialEq)]
pub struct FileHit {
    pub file_path: String,
    pub top_symbol: Option<String>,
    pub score: f32,
}

/// Dense ranking of a project's files for an embedded query: cosine over every
/// chunk vector, the deterministic path/symbol re-rank, then each file scored by
/// its best chunk. Best first; ties keep a stable order by path.
pub fn dense_file_ranking(
    conn: &Connection,
    code_project_id: i64,
    query: &str,
    query_vector: &[f32],
) -> anyhow::Result<Vec<FileHit>> {
    let pairs = crate::db::queries::get_code_embeddings(conn, code_project_id)?;
    let locations = crate::db::queries::get_code_chunk_locations(conn, code_project_id)?;
    let locations: std::collections::HashMap<i64, (String, Option<String>)> = locations
        .into_iter()
        .map(|(id, file_path, symbol)| (id, (file_path, symbol)))
        .collect();
    let query_tokens = crate::api::code::tokenize_query(query);
    let mut best: std::collections::HashMap<&str, (f32, Option<&str>)> =
        std::collections::HashMap::new();
    for (id, blob) in &pairs {
        let Some((file_path, symbol)) = locations.get(id) else {
            continue;
        };
        let cosine = crate::embed::cosine(query_vector, &crate::embed::deserialize(blob));
        let score =
            crate::api::code::rerank_score(cosine, file_path, symbol.as_deref(), &query_tokens);
        let entry = best.entry(file_path).or_insert((f32::MIN, None));
        if score > entry.0 {
            *entry = (score, symbol.as_deref());
        }
    }
    let mut hits: Vec<FileHit> = best
        .into_iter()
        .map(|(file_path, (score, symbol))| FileHit {
            file_path: file_path.to_string(),
            top_symbol: symbol.map(str::to_string),
            score,
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.file_path.cmp(&b.file_path))
    });
    Ok(hits)
}

/// Reciprocal rank fusion of file rankings: each file scores `Σ 1 / (k + rank)`
/// over the lists it appears in. Best first; ties keep the first list's order.
pub fn reciprocal_rank_fusion(rankings: &[Vec<String>], k: f64) -> Vec<String> {
    let weighted: Vec<(&[String], f64)> = rankings.iter().map(|r| (r.as_slice(), 1.0)).collect();
    weighted_reciprocal_rank_fusion(&weighted, k)
}

/// [`reciprocal_rank_fusion`] with a weight per ranking: `Σ w / (k + rank)`.
pub fn weighted_reciprocal_rank_fusion(rankings: &[(&[String], f64)], k: f64) -> Vec<String> {
    let mut scores: Vec<(String, f64, usize)> = Vec::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for &(ranking, weight) in rankings {
        for (rank, file) in ranking.iter().enumerate() {
            let at = *index.entry(file.clone()).or_insert_with(|| {
                scores.push((file.clone(), 0.0, scores.len()));
                scores.len() - 1
            });
            scores[at].1 += weight / (k + rank as f64 + 1.0);
        }
    }
    scores.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.2.cmp(&b.2))
    });
    scores.into_iter().map(|(file, _, _)| file).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{migrations, queries};

    #[test]
    fn files_are_ranked_by_their_best_chunk() {
        let conn = crate::db::connection::connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, _, _) =
            queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let project = queries::upsert_code_project(&conn, &org.id, "app", "/src").unwrap();
        let chunk = |path: &str, symbol: &str, vector: [f32; 2]| {
            let blob = crate::embed::serialize(&vector);
            queries::insert_code_chunk(
                &conn,
                project,
                path,
                "h",
                Some("rust"),
                Some(symbol),
                1,
                2,
                "fn x() {}",
                Some(&blob),
            )
            .unwrap();
        };
        chunk("src/far.rs", "far", [0.0, 1.0]);
        chunk("src/near.rs", "weak", [0.6, 0.8]);
        chunk("src/near.rs", "strong", [1.0, 0.0]);
        chunk("src/mid.rs", "mid", [0.8, 0.6]);
        // A chunk without a vector is not ranked.
        queries::insert_code_chunk(
            &conn,
            project,
            "src/unembedded.rs",
            "h",
            None,
            None,
            1,
            2,
            "x",
            None,
        )
        .unwrap();

        let hits = dense_file_ranking(&conn, project, "zzz", &[1.0, 0.0]).unwrap();
        let order: Vec<&str> = hits.iter().map(|h| h.file_path.as_str()).collect();
        assert_eq!(order, ["src/near.rs", "src/mid.rs", "src/far.rs"]);
        assert_eq!(hits[0].top_symbol.as_deref(), Some("strong"));
    }

    #[test]
    fn fusion_rewards_files_both_rankers_agree_on() {
        let list = |files: &[&str]| files.iter().map(|f| f.to_string()).collect::<Vec<_>>();
        let fused = reciprocal_rank_fusion(&[list(&["a", "b", "c"]), list(&["c", "d", "b"])], 60.0);
        assert_eq!(fused, ["c", "b", "a", "d"]);
        assert!(reciprocal_rank_fusion(&[], 60.0).is_empty());
        // A down-weighted ranking's files come after the full-weight ones.
        let (strong, weak) = (list(&["a", "b"]), list(&["c", "d"]));
        let fused = weighted_reciprocal_rank_fusion(&[(&strong, 1.0), (&weak, 0.25)], 60.0);
        assert_eq!(fused, ["a", "b", "c", "d"]);
    }

    #[test]
    fn bm25_finds_split_identifiers_and_follows_deletes() {
        let conn = crate::db::connection::connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, _, _) =
            queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let project = queries::upsert_code_project(&conn, &org.id, "app", "/src").unwrap();
        let other = queries::upsert_code_project(&conn, &org.id, "other", "/o").unwrap();
        let add = |project: i64, path: &str, symbol: &str, body: &str| {
            queries::insert_code_chunk(
                &conn,
                project,
                path,
                "h",
                Some("ts"),
                Some(symbol),
                1,
                2,
                body,
                None,
            )
            .unwrap()
        };
        add(
            project,
            "src/pages/SaleDetailPage.tsx",
            "SaleDetailPage",
            "export function SaleDetailPage() { return <RefundButton/> }",
        );
        add(
            project,
            "src/api/refunds.ts",
            "createRefund",
            "export async function createRefund(saleId) {}",
        );
        add(
            project,
            "src/utils/date.ts",
            "formatDate",
            "export const formatDate = (d) => d",
        );
        add(other, "src/api/refunds.ts", "createRefund", "other project");

        let hits =
            lexical::bm25_file_ranking(&conn, project, "raise a refund from the sale detail page")
                .unwrap();
        let files: Vec<&str> = hits.iter().map(|h| h.file_path.as_str()).collect();
        assert_eq!(
            files,
            ["src/pages/SaleDetailPage.tsx", "src/api/refunds.ts"]
        );

        queries::delete_chunks_for_file(&conn, project, "src/api/refunds.ts").unwrap();
        let hits = lexical::bm25_file_ranking(&conn, project, "create refund").unwrap();
        let files: Vec<&str> = hits.iter().map(|h| h.file_path.as_str()).collect();
        assert_eq!(files, ["src/pages/SaleDetailPage.tsx"]);
    }

    #[test]
    fn config_files_are_found_lexically_and_never_wait_for_a_vector() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".github/workflows")).unwrap();
        std::fs::write(
            root.join(".github/workflows/deploy.yml"),
            "name: Deploy\njobs:\n  backend:\n    runs-on: ubuntu-latest\n",
        )
        .unwrap();
        std::fs::write(
            root.join("Dockerfile"),
            "FROM rust:1.98-slim-bookworm\nARG VITE_API_URL\n",
        )
        .unwrap();
        std::fs::write(root.join("main.rs"), "fn main() { serve(); }\n").unwrap();

        let conn = crate::db::connection::connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, _, _) =
            queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        let db = std::sync::Arc::new(std::sync::Mutex::new(conn));
        crate::indexer::index_project(&org.id, "app", root.to_str().unwrap(), &db, None, false)
            .unwrap();

        let conn = db.lock().unwrap();
        let project: i64 = conn
            .query_row("SELECT id FROM code_projects WHERE name = 'app'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let hits =
            lexical::bm25_file_ranking(&conn, project, "vite api url baked into docker").unwrap();
        assert_eq!(hits[0].file_path, "Dockerfile");
        let hits = lexical::bm25_file_ranking(&conn, project, "deploy backend job").unwrap();
        assert_eq!(hits[0].file_path, ".github/workflows/deploy.yml");
        // Without an embedder the code file waits for a vector; config never does.
        let pending = queries::list_files_with_unembedded_chunks(&conn, project).unwrap();
        assert_eq!(pending, ["main.rs".to_string()].into_iter().collect());
    }
}
