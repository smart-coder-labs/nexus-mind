//! Lexical (BM25) leg of code retrieval over `code_chunks_fts` (migration v82).
//!
//! FTS5 tokenizes on non-alphanumerics, so `createRefund` or `SaleDetailPage`
//! would be one token a natural-language query never matches. Each chunk is
//! therefore indexed as identifier terms: camelCase, PascalCase, snake_case and
//! kebab-case split into lowercase words. The table is contentless: it holds the
//! index only, never a second copy of the code.

use super::FileHit;
use rusqlite::Connection;

/// Lowercase words of `text` with identifiers split at case and digit
/// boundaries: `parseHTTPResponse2` → `parse http response 2`.
pub fn identifier_terms(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    for word in text.split(|c: char| !c.is_alphanumeric()) {
        let chars: Vec<char> = word.chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            let boundary = i > 0 && {
                let prev = chars[i - 1];
                let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
                (c.is_uppercase() && (prev.is_lowercase() || prev.is_numeric()))
                    || (c.is_uppercase() && prev.is_uppercase() && next_lower)
                    || (c.is_numeric() != prev.is_numeric())
            };
            if boundary || (i == 0 && !out.is_empty()) {
                out.push(' ');
            }
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// Adds a chunk to the lexical index (its rowid is the chunk id).
pub fn index_chunk(
    conn: &Connection,
    chunk_id: i64,
    file_path: &str,
    symbol: Option<&str>,
    content: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO code_chunks_fts (rowid, path, symbol, body) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            chunk_id,
            identifier_terms(file_path),
            identifier_terms(symbol.unwrap_or_default()),
            identifier_terms(content),
        ],
    )?;
    Ok(())
}

/// Rebuilds the whole lexical index from `code_chunks`. Rows only enter the
/// index through `insert_code_chunk`, so chunks written any other way (a backup
/// restore, a binary from before v82) are missing until this runs.
pub fn rebuild_index(conn: &Connection) -> anyhow::Result<usize> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("INSERT INTO code_chunks_fts(code_chunks_fts) VALUES('delete-all')", [])?;
    let mut indexed = 0;
    {
        let mut stmt = tx.prepare("SELECT id, file_path, symbol, content FROM code_chunks")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            index_chunk(
                &tx,
                row.get(0)?,
                &row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?.as_deref(),
                &row.get::<_, String>(3)?,
            )?;
            indexed += 1;
        }
    }
    tx.commit()?;
    Ok(indexed)
}

/// Rebuilds the lexical index when its document count differs from the chunk
/// count (two counts; run at startup and after a restore). Returns whether it
/// rebuilt.
pub fn ensure_index(conn: &Connection) -> anyhow::Result<bool> {
    let chunks: i64 = conn.query_row("SELECT count(*) FROM code_chunks", [], |r| r.get(0))?;
    let indexed: i64 =
        conn.query_row("SELECT count(*) FROM code_chunks_fts_docsize", [], |r| r.get(0))?;
    if chunks == indexed {
        return Ok(false);
    }
    tracing::warn!(chunks, indexed, "lexical code index out of step; rebuilding");
    rebuild_index(conn)?;
    Ok(true)
}

/// Most bytes of a query that are read, and most distinct terms kept: the
/// expression is built and matched while the database is locked.
pub const MAX_QUERY_BYTES: usize = 4096;
pub const MAX_TERMS: usize = 64;

/// The FTS5 query for a natural-language query: its first [`MAX_TERMS`] distinct
/// terms, quoted (so no FTS syntax from the user is interpreted), any of which
/// may match.
pub fn match_expression(query: &str) -> Option<String> {
    let mut end = query.len().min(MAX_QUERY_BYTES);
    while !query.is_char_boundary(end) {
        end -= 1;
    }
    let mut seen = std::collections::HashSet::new();
    let mut terms: Vec<String> = Vec::new();
    for term in identifier_terms(&query[..end]).split_whitespace() {
        if terms.len() == MAX_TERMS {
            break;
        }
        if term.chars().count() < 2 || STOPWORDS.contains(&term) {
            continue;
        }
        if seen.insert(term.to_string()) {
            terms.push(term.to_string());
        }
    }
    (!terms.is_empty()).then(|| {
        terms
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(" OR ")
    })
}

/// Column weights for `bm25()`: path, symbol, body.
const WEIGHTS: (f64, f64, f64) = (3.0, 4.0, 1.0);
/// Chunks read before files are deduplicated.
const CHUNK_LIMIT: i64 = 1000;

/// Score multiplier for config (lexical-only) chunks: a manifest or workflow has
/// to match clearly better than code to outrank it, so `"icons"` in a web
/// manifest does not beat the component a UI question is about. Calibrated on
/// the golden questions (2026-10-02): 0.7 removed every code-question
/// regression config caused at 1.0 and kept one of two infra questions at
/// rank 1; 0.5 and below buried both. Recalibrate as infra questions grow.
pub const CONFIG_WEIGHT: f32 = 0.7;

/// BM25 ranking of a project's files: each file scored by its best chunk.
pub fn bm25_file_ranking(
    conn: &Connection,
    code_project_id: i64,
    query: &str,
) -> anyhow::Result<Vec<FileHit>> {
    bm25_file_ranking_weighted(conn, code_project_id, query, CONFIG_WEIGHT)
}

/// One chunk matched by BM25, config weight applied: score in (0, 1], relative to
/// the best match of the query (higher is better).
#[derive(Clone, Debug, PartialEq)]
pub struct ChunkHit {
    pub chunk_id: i64,
    pub file_path: String,
    pub symbol: Option<String>,
    pub lexical_only: bool,
    pub score: f32,
}

/// BM25 ranking of a project's chunks (best first, at most [`CHUNK_LIMIT`]).
pub fn bm25_chunk_ranking(
    conn: &Connection,
    code_project_id: i64,
    query: &str,
    config_weight: f32,
) -> anyhow::Result<Vec<ChunkHit>> {
    let Some(expression) = match_expression(query) else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare(
        "SELECT c.id, c.file_path, c.symbol, bm25(code_chunks_fts, ?3, ?4, ?5) AS rank, c.lexical_only
         FROM code_chunks_fts
         JOIN code_chunks c ON c.id = code_chunks_fts.rowid
         WHERE code_chunks_fts MATCH ?1 AND c.code_project_id = ?2
         ORDER BY rank
         LIMIT ?6",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![
            expression,
            code_project_id,
            WEIGHTS.0,
            WEIGHTS.1,
            WEIGHTS.2,
            CHUNK_LIMIT
        ],
        |r| {
            let lexical_only: bool = r.get(4)?;
            // bm25() is lower-is-better (negative).
            let rank: f64 = r.get(3)?;
            let weight = if lexical_only { config_weight } else { 1.0 };
            Ok(ChunkHit {
                chunk_id: r.get(0)?,
                file_path: r.get(1)?,
                symbol: r.get(2)?,
                lexical_only,
                score: -rank as f32 * weight,
            })
        },
    )?;
    let mut hits = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    // Raw bm25() values carry corpus-wide statistics (IDF and lengths over every
    // tenant's chunks); scores leave this function relative to the best hit.
    let best = hits.iter().map(|h| h.score).fold(0.0_f32, f32::max);
    if best > 0.0 {
        for hit in &mut hits {
            hit.score /= best;
        }
    }
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.chunk_id.cmp(&b.chunk_id))
    });
    Ok(hits)
}

/// Files of a chunk ranking, each scored by its best chunk. Best first; ties by path.
pub fn files_of(chunks: &[ChunkHit]) -> Vec<FileHit> {
    let mut best: std::collections::HashMap<&str, (f32, Option<&str>)> =
        std::collections::HashMap::new();
    for chunk in chunks {
        let entry = best.entry(&chunk.file_path).or_insert((f32::MIN, None));
        if chunk.score > entry.0 {
            *entry = (chunk.score, chunk.symbol.as_deref());
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
    hits
}

/// [`bm25_file_ranking`] with an explicit config weight (the eval varies it).
pub fn bm25_file_ranking_weighted(
    conn: &Connection,
    code_project_id: i64,
    query: &str,
    config_weight: f32,
) -> anyhow::Result<Vec<FileHit>> {
    Ok(files_of(&bm25_chunk_ranking(
        conn,
        code_project_id,
        query,
        config_weight,
    )?))
}

/// Words too common in task titles to discriminate between files.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "in", "into", "is", "it", "of",
    "on", "or", "the", "to", "with", "when", "without", "not", "no", "so", "that", "this", "add",
    "use", "make", "new", "via",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_split_into_words() {
        assert_eq!(identifier_terms("createRefund"), "create refund");
        assert_eq!(identifier_terms("SaleDetailPage"), "sale detail page");
        assert_eq!(
            identifier_terms("parseHTTPResponse2"),
            "parse http response 2"
        );
        assert_eq!(
            identifier_terms("src/api/sale_detail-page.tsx"),
            "src api sale detail page tsx"
        );
        assert_eq!(identifier_terms("Ünïcode café"), "ünïcode café");
    }

    #[test]
    fn huge_queries_are_capped_in_bytes_and_terms() {
        let words: Vec<String> = (0..10_000).map(|i| format!("w{i:05}x")).collect();
        let expression = match_expression(&words.join(" ")).unwrap();
        assert_eq!(expression.split(" OR ").count(), MAX_TERMS);
        // A multi-byte query over the cap is cut on a character boundary.
        let long = match_expression(&"é".repeat(5000)).unwrap();
        assert_eq!(long.trim_matches('"').chars().count(), MAX_QUERY_BYTES / 2);
    }

    #[test]
    fn queries_are_quoted_terms_joined_by_or() {
        assert_eq!(
            match_expression("Raise a refund from the saleDetail page").as_deref(),
            Some("\"raise\" OR \"refund\" OR \"sale\" OR \"detail\" OR \"page\"")
        );
        // FTS syntax in the query is never interpreted.
        assert_eq!(
            match_expression("NEAR(x y) OR \"drop\" *").as_deref(),
            Some("\"near\" OR \"drop\"")
        );
        assert_eq!(match_expression("a of the"), None);
    }
}
