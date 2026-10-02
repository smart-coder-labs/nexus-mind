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

/// The FTS5 query for a natural-language query: its distinct terms, quoted (so
/// no FTS syntax from the user is interpreted), any of which may match.
pub fn match_expression(query: &str) -> Option<String> {
    let mut terms: Vec<String> = Vec::new();
    for term in identifier_terms(query).split_whitespace() {
        if term.chars().count() < 2 || STOPWORDS.contains(&term) {
            continue;
        }
        let term = term.to_string();
        if !terms.contains(&term) {
            terms.push(term);
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

/// BM25 ranking of a project's files: each file scored by its best chunk.
pub fn bm25_file_ranking(
    conn: &Connection,
    code_project_id: i64,
    query: &str,
) -> anyhow::Result<Vec<FileHit>> {
    let Some(expression) = match_expression(query) else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare(
        "SELECT c.file_path, c.symbol, bm25(code_chunks_fts, ?3, ?4, ?5) AS rank
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
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, f64>(2)?,
            ))
        },
    )?;
    let mut hits: Vec<FileHit> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in rows {
        let (file_path, symbol, rank) = row?;
        // Rows come best first; bm25() is lower-is-better.
        if seen.insert(file_path.clone()) {
            hits.push(FileHit {
                file_path,
                top_symbol: symbol,
                score: -rank as f32,
            });
        }
    }
    Ok(hits)
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
