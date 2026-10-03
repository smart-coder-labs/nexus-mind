//! ContextPack builder ("Bibliotecario", factory F2).
//!
//! Turns a task description into the context a coding model starts from: the
//! files BM25 ranks for it (each with its best matching chunks), plus the files
//! one relative import away from the top ones. The `ContextPack` contract is
//! the auditable index of that selection (path, symbol, kind, reason, content
//! hash); the evidence carries the code itself, so an agent does not reopen
//! every listed file (the failure #54 measured on `locate_code`).

use super::{graph, lexical};
use crate::factory::contracts::{
    ArtifactKind, ContextArtifact, ContextPack, Contract, PackRepository, SchemaV1,
};
use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Defaults and hard caps for a pack.
pub const DEFAULT_MAX_FILES: usize = 8;
pub const MAX_FILES: usize = 20;
pub const DEFAULT_MAX_BYTES: usize = 24_000;
pub const MAX_BYTES: usize = 64_000;
/// Matching chunks kept per ranked file.
const CHUNKS_PER_FILE: usize = 2;
/// Top files whose imports are followed, and neighbours added at most.
const EXPANSION_SEEDS: usize = 3;
const MAX_NEIGHBOURS: usize = 3;

pub struct PackRequest<'a> {
    pub query: &'a str,
    /// The factory task this pack is for; a fresh id when absent.
    pub task_id: Option<&'a str>,
    /// The commit the task targets; the index's own commit when absent.
    pub commit: Option<&'a str>,
    pub max_files: usize,
    pub max_bytes: usize,
}

/// The code behind one artifact of the pack (same `content_hash`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Evidence {
    /// Index into `pack.artifacts`.
    pub artifact: usize,
    pub path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub content: String,
    /// Cut to fit the pack's byte budget.
    pub truncated: bool,
}

/// The index the pack was built from.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IndexInfo {
    pub commit: Option<String>,
    pub last_indexed: Option<String>,
    /// The task targets a different commit than the index holds: code may have
    /// moved since.
    pub stale: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PackResponse {
    pub pack: ContextPack,
    pub evidence: Vec<Evidence>,
    pub index: IndexInfo,
}

struct Selected {
    chunk_id: i64,
    symbol: Option<String>,
    kind: ArtifactKind,
    reason: String,
    score: Option<f64>,
}

pub fn build(
    conn: &Connection,
    code_project_id: i64,
    request: &PackRequest<'_>,
) -> anyhow::Result<PackResponse> {
    let max_files = request.max_files.clamp(1, MAX_FILES);
    let budget = request.max_bytes.clamp(1, MAX_BYTES);
    let task_id = match request.task_id {
        Some(id) => id.to_string(),
        None => uuid::Uuid::new_v4().to_string(),
    };
    let (indexed_commit, last_indexed) = index_state(conn, code_project_id)?;
    let commit = request
        .commit
        .map(str::to_string)
        .or_else(|| indexed_commit.clone())
        .ok_or_else(|| anyhow::anyhow!("index_commit_unknown"))?;
    let stale = indexed_commit
        .as_deref()
        .is_some_and(|indexed| indexed != commit);

    let chunks =
        lexical::bm25_chunk_ranking(conn, code_project_id, request.query, lexical::CONFIG_WEIGHT)?;
    let files = lexical::files_of(&chunks);
    let terms: Vec<String> = lexical::match_expression(request.query)
        .map(|e| {
            e.split(" OR ")
                .map(|t| t.trim_matches('"').to_string())
                .collect()
        })
        .unwrap_or_default();

    // Ranked files first, leaving room for the import neighbours of the top ones.
    let ranked_slots = max_files
        .saturating_sub(MAX_NEIGHBOURS.min(max_files / 3))
        .max(1);
    let mut selected: Vec<Selected> = Vec::new();
    let mut chosen_files: Vec<String> = Vec::new();
    for (rank, file) in files.iter().take(ranked_slots).enumerate() {
        chosen_files.push(file.file_path.clone());
        for chunk in chunks
            .iter()
            .filter(|c| c.file_path == file.file_path)
            .take(CHUNKS_PER_FILE)
        {
            let matched = matched_terms(conn, chunk.chunk_id, &terms)?;
            selected.push(Selected {
                chunk_id: chunk.chunk_id,
                symbol: chunk.symbol.clone(),
                kind: kind_of(&chunk.file_path, chunk.lexical_only),
                reason: format!(
                    "rank {} for the task text; matches {}",
                    rank + 1,
                    if matched.is_empty() {
                        "its terms".to_string()
                    } else {
                        matched.join(", ")
                    }
                ),
                score: Some(f64::from(chunk.score)),
            });
        }
    }
    let mut neighbours = 0;
    'seeds: for seed in files.iter().take(EXPANSION_SEEDS.min(ranked_slots)) {
        for (path, relation) in graph::import_neighbours(conn, code_project_id, &seed.file_path)? {
            if neighbours >= MAX_NEIGHBOURS || chosen_files.len() >= max_files {
                break 'seeds;
            }
            if chosen_files.contains(&path) {
                continue;
            }
            let Some((chunk_id, symbol, lexical_only)) = first_chunk(conn, code_project_id, &path)?
            else {
                continue;
            };
            chosen_files.push(path.clone());
            neighbours += 1;
            selected.push(Selected {
                chunk_id,
                symbol,
                kind: kind_of(&path, lexical_only),
                reason: match relation {
                    graph::Relation::ImportedBySeed => format!("imported by {}", seed.file_path),
                    graph::Relation::ImportsSeed => format!("imports {}", seed.file_path),
                },
                score: None,
            });
        }
    }

    let mut artifacts = Vec::new();
    let mut evidence = Vec::new();
    let mut remaining = budget;
    for item in selected {
        if remaining == 0 {
            break;
        }
        let (path, start_line, end_line, content) = chunk_body(conn, item.chunk_id)?;
        let content_hash = format!("sha256:{}", hex::encode(Sha256::digest(content.as_bytes())));
        let (content, truncated) = cut(&content, remaining);
        remaining -= content.len();
        evidence.push(Evidence {
            artifact: artifacts.len(),
            path: path.clone(),
            start_line,
            end_line,
            content,
            truncated,
        });
        artifacts.push(ContextArtifact {
            path,
            symbol: item.symbol,
            kind: item.kind,
            reason: item.reason,
            content_hash,
            retrieval_score: item.score,
        });
    }

    let pack = ContextPack {
        schema_version: SchemaV1,
        task_id,
        repository: PackRepository {
            commit,
            branch: None,
        },
        artifacts,
        constraints: Vec::new(),
        acceptance_tests: Vec::new(),
    };
    pack.validate()
        .map_err(|e| anyhow::anyhow!("invalid_context_pack: {e}"))?;
    Ok(PackResponse {
        pack,
        evidence,
        index: IndexInfo {
            commit: indexed_commit,
            last_indexed,
            stale,
        },
    })
}

/// What an artifact is, from its path (config chunks are lexical-only).
pub fn kind_of(path: &str, lexical_only: bool) -> ArtifactKind {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    if lower.ends_with(".graphql")
        || lower.ends_with(".gql")
        || lower.ends_with(".sql")
        || lower.contains("/migrations/")
        || name.contains(".schema.")
    {
        return ArtifactKind::Schema;
    }
    if lexical_only {
        return ArtifactKind::Config;
    }
    if lower.contains("/tests/")
        || lower.contains("__tests__")
        || lower.starts_with("tests/")
        || lower.starts_with("e2e/")
        || lower.contains("/e2e/")
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.contains("_test.")
    {
        return ArtifactKind::Test;
    }
    if lower.ends_with(".md") || lower.ends_with(".mdx") {
        return ArtifactKind::Documentation;
    }
    ArtifactKind::Code
}

/// The query terms found in a chunk's path, symbol or body, in query order.
fn matched_terms(
    conn: &Connection,
    chunk_id: i64,
    terms: &[String],
) -> anyhow::Result<Vec<String>> {
    let (path, symbol, content): (String, Option<String>, String) = conn.query_row(
        "SELECT file_path, symbol, content FROM code_chunks WHERE id = ?1",
        [chunk_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let haystack = format!(
        " {} {} {} ",
        lexical::identifier_terms(&path),
        lexical::identifier_terms(symbol.as_deref().unwrap_or_default()),
        lexical::identifier_terms(&content)
    );
    Ok(terms
        .iter()
        .filter(|t| haystack.contains(&format!(" {t} ")))
        .cloned()
        .collect())
}

fn first_chunk(
    conn: &Connection,
    code_project_id: i64,
    path: &str,
) -> anyhow::Result<Option<(i64, Option<String>, bool)>> {
    let mut stmt = conn.prepare(
        "SELECT id, symbol, lexical_only FROM code_chunks
         WHERE code_project_id = ?1 AND file_path = ?2 ORDER BY start_line, id LIMIT 1",
    )?;
    let mut rows = stmt.query(rusqlite::params![code_project_id, path])?;
    Ok(match rows.next()? {
        Some(r) => Some((r.get(0)?, r.get(1)?, r.get(2)?)),
        None => None,
    })
}

fn chunk_body(conn: &Connection, chunk_id: i64) -> anyhow::Result<(String, i64, i64, String)> {
    Ok(conn.query_row(
        "SELECT file_path, start_line, end_line, content FROM code_chunks WHERE id = ?1",
        [chunk_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?)
}

fn index_state(
    conn: &Connection,
    code_project_id: i64,
) -> anyhow::Result<(Option<String>, Option<String>)> {
    Ok(conn.query_row(
        "SELECT indexed_commit, last_indexed FROM code_projects WHERE id = ?1",
        [code_project_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?)
}

/// `text` within `limit` bytes, cut on a character boundary.
fn cut(text: &str, limit: usize) -> (String, bool) {
    if text.len() <= limit {
        return (text.to_string(), false);
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::connect, migrations, queries};

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn repo() -> (tempfile::TempDir, Connection, i64) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/pages")).unwrap();
        std::fs::create_dir_all(root.join("src/api")).unwrap();
        std::fs::write(
            root.join("src/pages/SaleDetail.tsx"),
            "import { createRefund } from '../api/refunds';\nexport function SaleDetail() { return createRefund(); }\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/api/refunds.ts"),
            "export async function createRefund(id: string) { return post('/refunds', id); }\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/pages/SaleDetail.test.tsx"),
            "test('sale detail renders', () => {});\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/util.ts"),
            "export const pad = (n: number) => n;\n",
        )
        .unwrap();
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, _, _) = queries::bootstrap(&conn, "Acme", "acme", "a@acme.com", "A").unwrap();
        let db = std::sync::Arc::new(std::sync::Mutex::new(conn));
        crate::indexer::index_project(&org.id, "app", root.to_str().unwrap(), &db, None, false)
            .unwrap();
        let conn = std::sync::Arc::try_unwrap(db)
            .unwrap()
            .into_inner()
            .unwrap();
        let id = conn
            .query_row("SELECT id FROM code_projects WHERE name = 'app'", [], |r| {
                r.get(0)
            })
            .unwrap();
        (dir, conn, id)
    }

    fn request(query: &str) -> PackRequest<'_> {
        PackRequest {
            query,
            task_id: None,
            commit: Some(COMMIT),
            max_files: DEFAULT_MAX_FILES,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }

    #[test]
    fn a_pack_carries_ranked_code_with_reasons_hashes_and_bodies() {
        let (_dir, conn, project) = repo();
        let response = build(&conn, project, &request("refund from the sale detail page")).unwrap();
        let pack = &response.pack;
        assert_eq!(pack.repository.commit, COMMIT);
        let paths: Vec<&str> = pack.artifacts.iter().map(|a| a.path.as_str()).collect();
        assert!(paths.contains(&"src/pages/SaleDetail.tsx"), "{paths:?}");
        assert!(paths.contains(&"src/api/refunds.ts"), "{paths:?}");
        assert!(!paths.contains(&"src/util.ts"), "unrelated file: {paths:?}");
        let first = &pack.artifacts[0];
        assert!(first.reason.starts_with("rank 1"), "{}", first.reason);
        assert!(first.content_hash.starts_with("sha256:"));
        // Every artifact has its code, hashed the same way.
        assert_eq!(response.evidence.len(), pack.artifacts.len());
        for e in &response.evidence {
            let artifact = &pack.artifacts[e.artifact];
            assert_eq!(e.path, artifact.path);
            assert!(!e.content.is_empty());
        }
        let test = pack
            .artifacts
            .iter()
            .find(|a| a.path.ends_with(".test.tsx"))
            .unwrap();
        assert_eq!(test.kind, ArtifactKind::Test);
        assert!(!response.index.stale, "no indexed commit to compare with");
    }

    #[test]
    fn imports_of_the_top_file_are_added_with_their_relation() {
        let (_dir, conn, project) = repo();
        // Only the page matches; the API file comes in through its import.
        let response = build(&conn, project, &request("SaleDetail")).unwrap();
        let api = response
            .pack
            .artifacts
            .iter()
            .find(|a| a.path == "src/api/refunds.ts")
            .expect("imported file expanded");
        assert_eq!(api.reason, "imported by src/pages/SaleDetail.tsx");
        assert!(api.retrieval_score.is_none());
    }

    #[test]
    fn the_byte_budget_cuts_evidence_on_a_character_boundary() {
        let (_dir, conn, project) = repo();
        let mut req = request("refund sale detail");
        req.max_bytes = 40;
        let response = build(&conn, project, &req).unwrap();
        let total: usize = response.evidence.iter().map(|e| e.content.len()).sum();
        assert!(total <= 40, "{total}");
        assert!(response.evidence.iter().any(|e| e.truncated));
    }

    #[test]
    fn a_pack_needs_a_commit_and_flags_a_stale_index() {
        let (_dir, conn, project) = repo();
        let mut req = request("refund");
        req.commit = None;
        assert_eq!(
            build(&conn, project, &req).unwrap_err().to_string(),
            "index_commit_unknown"
        );
        conn.execute("UPDATE code_projects SET indexed_commit = ?1", [COMMIT])
            .unwrap();
        let fresh = build(&conn, project, &req).unwrap();
        assert_eq!(fresh.pack.repository.commit, COMMIT);
        assert!(!fresh.index.stale);
        let other = "fedcba9876543210fedcba9876543210fedcba98";
        req.commit = Some(other);
        assert!(build(&conn, project, &req).unwrap().index.stale);
    }

    #[test]
    fn kinds_follow_the_path() {
        assert_eq!(kind_of("src/a.ts", false), ArtifactKind::Code);
        assert_eq!(kind_of("src/a.spec.ts", false), ArtifactKind::Test);
        assert_eq!(kind_of("e2e/specs/x.ts", false), ArtifactKind::Test);
        assert_eq!(
            kind_of("apps/backend/tests/api.rs", false),
            ArtifactKind::Test
        );
        assert_eq!(kind_of("schema/api.graphql", true), ArtifactKind::Schema);
        assert_eq!(
            kind_of(".github/workflows/ci.yml", true),
            ArtifactKind::Config
        );
    }
}
