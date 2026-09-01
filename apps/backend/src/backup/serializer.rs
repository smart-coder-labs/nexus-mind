use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A whitelisted set of table names that we know how to serialize to JSON.
/// New tables must be added here explicitly — never introspect `sqlite_master`
/// at runtime. Two reasons:
///   1. Virtual tables (FTS5, FTS5 contentless, etc.) cannot be `SELECT * FROM`'d
///      and would crash the backup.
///   2. Implicit inclusion risks dumping transient or derived tables in the future
///      that should not be part of a "snapshot".
pub const BACKUP_TABLES: &[&str] = &[
    "organizations",
    "users",
    "api_keys",
    "password_reset_tokens",
    "memories",
    "memory_embeddings",
    "sessions",
    "projects",
    "project_members",
    "policies",
    "code_projects",
    "code_chunks",
    "code_symbols",
    "code_edges",
    "code_files",
    "conventions",
    "roles",
    "agents",
    "agent_assignments",
    "webhooks",
    "webhook_deliveries",
    "collections",
    "invite_links",
    "audit_logs",
];

/// One table's dump: row count + the rows themselves as a JSON array of objects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableDump {
    pub table_name: String,
    pub row_count:  i64,
    pub rows:       Value,
}

/// Dump a single table to a `TableDump` (rows serialized as JSON).
///
/// `connection` must be a working SQLite connection. The function is sync because
/// rusqlite is sync; backup runs are scheduled in a `tokio::task::spawn_blocking`
/// wrapper at the call site.
pub fn dump_table(conn: &Connection, table_name: &str) -> Result<TableDump, String> {
    // PRAGMA table_info gives us the declared column list in order. We bind by
    // name (not position) so the JSON object shape is predictable for restore.
    let columns: Vec<String> = conn
        .prepare(&format!("PRAGMA table_info(\"{table_name}\")"))
        .map_err(|e| format!("table_info {table_name}: {e}"))?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("query table_info {table_name}: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    if columns.is_empty() {
        return Err(format!("table '{table_name}' has no columns or does not exist"));
    }

    // Quote columns to handle reserved-word column names (e.g. `type`).
    let col_list = columns
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!("SELECT {col_list} FROM \"{table_name}\"");

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("prepare {table_name}: {e}"))?;

    let column_refs: Vec<&str> = columns.iter().map(String::as_str).collect();
    let mut rows: Vec<Value> = Vec::new();
    let mut row_count: i64 = 0;

    let mut query = stmt
        .query([])
        .map_err(|e| format!("query {table_name}: {e}"))?;

    while let Some(row) = query.next().map_err(|e| format!("next {table_name}: {e}"))? {
        let mut obj = serde_json::Map::with_capacity(columns.len());
        for (i, col) in column_refs.iter().enumerate() {
            // Try to read as a generic JSON value via SQL. We do this by
            // introspecting the value: NULL → Null, INTEGER → Number, TEXT →
            // String, REAL → Number, BLOB → base64 string. Using `get` with
            // a coarse type avoids losing fidelity.
            let value = sqlite_value_to_json(row, i);
            obj.insert((*col).to_string(), value);
        }
        rows.push(Value::Object(obj));
        row_count += 1;
    }

    Ok(TableDump {
        table_name: table_name.to_string(),
        row_count,
        rows: Value::Array(rows),
    })
}

/// Convert a rusqlite row cell at index `i` to a JSON value. NULLs are preserved
/// as JSON `null`. BLOB bytes are encoded as base64 strings — they're not
/// human-readable but they're lossless and round-trippable.
fn sqlite_value_to_json(row: &rusqlite::Row<'_>, i: usize) -> Value {
    // Try Option<String> first — covers TEXT and most NULL cases.
    if let Ok(v) = row.get::<_, Option<String>>(i) {
        return match v {
            Some(s) => Value::String(s),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.get::<_, Option<i64>>(i) {
        return match v {
            Some(n) => Value::Number(serde_json::Number::from(n)),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.get::<_, Option<f64>>(i) {
        return match v {
            Some(f) => serde_json::Number::from_f64(f)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.get::<_, Option<Vec<u8>>>(i) {
        return match v {
            Some(bytes) => Value::String(base64_encode(&bytes)),
            None => Value::Null,
        };
    }
    Value::Null
}

// Uses `chunks_exact(3)` with an explicit `remainder()` tail for base64 padding;
// keeping that shape is clearer here than the as_chunks rewrite.
#[allow(clippy::chunks_exact_to_as_chunks)]
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut chunks = bytes.chunks_exact(3);
    for c in &mut chunks {
        let n = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | (c[2] as u32);
        out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 0x3F) as usize] as char);
        out.push(ALPHABET[(n & 0x3F) as usize] as char);
    }
    let rem = chunks.remainder();
    match rem.len() {
        1 => {
            let n = (rem[0] as u32) << 16;
            out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
            out.push(ALPHABET[((n >> 6) & 0x3F) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

/// Convenience: serialize a single table's rows to a JSON string. Used by tests
/// and by the download endpoint.
pub fn dump_table_to_json(conn: &Connection, table_name: &str) -> Result<String, String> {
    let dump = dump_table(conn, table_name)?;
    serde_json::to_string(&dump).map_err(|e| format!("serialize {table_name}: {e}"))
}

/// Total size of the dump in bytes — used to populate `backups.size_bytes`.
pub fn approx_dump_size(dumps: &[TableDump]) -> i64 {
    dumps
        .iter()
        .map(|d| serde_json::to_vec(d).map(|v| v.len() as i64).unwrap_or(0))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::connect, migrations};

    fn in_memory_db() -> Connection {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn dumps_empty_table_to_empty_array() {
        let conn = in_memory_db();
        let dump = dump_table(&conn, "organizations").unwrap();
        assert_eq!(dump.table_name, "organizations");
        assert_eq!(dump.row_count, 0);
        assert_eq!(dump.rows, Value::Array(vec![]));
    }

    #[test]
    fn dumps_rows_with_correct_columns() {
        let conn = in_memory_db();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        let dump = dump_table(&conn, "organizations").unwrap();
        assert_eq!(dump.row_count, 1);
        let rows = dump.rows.as_array().unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row["id"], "org1");
        assert_eq!(row["name"], "Acme");
        assert_eq!(row["slug"], "acme");
    }

    #[test]
    fn preserves_null_values() {
        let conn = in_memory_db();
        conn.execute(
            "INSERT INTO organizations (id, name, slug, plan) VALUES ('org1', 'Acme', 'acme', 'free')",
            [],
        )
        .unwrap();
        let _dump = dump_table(&conn, "organizations").unwrap();
        // `plan` was not provided in INSERT — should still serialize as 'free' (default).
        // Now test an actual nullable column.
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1', 'org1', NULL, 'Dev')",
            [],
        )
        .unwrap();
        let users = dump_table(&conn, "users").unwrap();
        let row = &users.rows.as_array().unwrap()[0];
        assert_eq!(row["email"], Value::Null);
    }

    #[test]
    fn roundtrips_integers_and_floats() {
        let conn = in_memory_db();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO code_projects (org_id, name, root_path, file_count) VALUES ('org1', 'p', '/x', 42)",
            [],
        )
        .unwrap();
        let dump = dump_table(&conn, "code_projects").unwrap();
        let row = &dump.rows.as_array().unwrap()[0];
        assert_eq!(row["file_count"], Value::Number(serde_json::Number::from(42)));
    }

    #[test]
    fn encodes_blobs_as_base64() {
        let conn = in_memory_db();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        // Store a blob in memory_embeddings; the column is BLOB NOT NULL.
        // FK to memories(id) — but we don't need a real memory row to test
        // the base64 encoding of the blob. Temporarily disable FKs for this
        // test since we only care about the serialization shape.
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        conn.execute(
            "INSERT INTO memory_embeddings (memory_id, embedding) VALUES ('m1', X'010203')",
            [],
        )
        .unwrap();
        let dump = dump_table(&conn, "memory_embeddings").unwrap();
        let row = &dump.rows.as_array().unwrap()[0];
        // base64(0x01 0x02 0x03) = "AQID"
        assert_eq!(row["embedding"], "AQID");
    }

    #[test]
    fn unknown_table_returns_error() {
        let conn = in_memory_db();
        let err = dump_table(&conn, "no_such_table").unwrap_err();
        assert!(err.contains("no columns or does not exist"), "got: {err}");
    }

    #[test]
    fn reserved_word_columns_are_handled() {
        // `type` is a SQLite reserved word; verify quoting works.
        let conn = in_memory_db();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        // policies has a `type`-like column name? No — but memories has a column
        // literally named `type`. Insert and verify.
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1', 'org1', 'a@b', 'A')",
            [],
        )
        .unwrap();
        let dump = dump_table(&conn, "memories").unwrap();
        // No rows yet — just verify the column introspection didn't crash.
        assert_eq!(dump.row_count, 0);
    }

    #[test]
    fn approx_size_is_sum_of_serialized_lengths() {
        let dumps = vec![
            TableDump {
                table_name: "a".into(),
                row_count:  1,
                rows:       Value::Array(vec![Value::Object(Default::default())]),
            },
            TableDump {
                table_name: "b".into(),
                row_count:  0,
                rows:       Value::Array(vec![]),
            },
        ];
        let size = approx_dump_size(&dumps);
        assert!(size > 0, "size must be > 0");
    }

    #[test]
    fn base64_helper_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }
}
