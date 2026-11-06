//! Dependency expansion over the code graph's file-level `imports` edges
//! (factory F2). Only relative imports resolve to files; aliased and package
//! imports end at `External` nodes and are not followed.

use rusqlite::Connection;

/// How a neighbour relates to the file it was expanded from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    /// The seed imports this file.
    ImportedBySeed,
    /// This file imports the seed.
    ImportsSeed,
}

/// Files one import away from `seed`, both directions, sorted by path.
pub fn import_neighbours(
    conn: &Connection,
    code_project_id: i64,
    seed: &str,
) -> anyhow::Result<Vec<(String, Relation)>> {
    let node = format!("file::{seed}");
    let mut stmt = conn.prepare(
        "SELECT substr(t.qualified_name, 7), 0
         FROM code_edges e
         JOIN code_symbols f ON f.id = e.from_symbol_id
         JOIN code_symbols t ON t.id = e.to_symbol_id
         WHERE e.code_project_id = ?1 AND e.edge_type = 'imports'
           AND f.qualified_name = ?2 AND t.symbol_type = 'File'
         UNION
         SELECT substr(f.qualified_name, 7), 1
         FROM code_edges e
         JOIN code_symbols f ON f.id = e.from_symbol_id
         JOIN code_symbols t ON t.id = e.to_symbol_id
         WHERE e.code_project_id = ?1 AND e.edge_type = 'imports'
           AND t.qualified_name = ?2 AND f.symbol_type = 'File'
         ORDER BY 1",
    )?;
    let rows = stmt.query_map(rusqlite::params![code_project_id, node], |r| {
        let relation = if r.get::<_, i64>(1)? == 0 {
            Relation::ImportedBySeed
        } else {
            Relation::ImportsSeed
        };
        Ok((r.get::<_, String>(0)?, relation))
    })?;
    let mut out: Vec<(String, Relation)> = Vec::new();
    for row in rows {
        let (path, relation) = row?;
        if path != seed && !out.iter().any(|(p, _)| *p == path) {
            out.push((path, relation));
        }
    }
    Ok(out)
}
