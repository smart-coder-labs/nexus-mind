use crate::backup::client::fetch_table_dump;
use anyhow::{anyhow, Context, Result};
use rusqlite::{Connection, Transaction};
use sqlx::PgPool;
use std::collections::HashSet;

/// The set of tables we are allowed to truncate-and-reload from a backup. Same
/// list as the serializer's whitelist, with virtual tables (memories_fts etc.)
/// excluded automatically — they are derived state, not data we restore.
pub const RESTORABLE_TABLES: &[&str] = &[
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

/// Summary returned by `restore_backup`. Tells the caller how many rows each
/// table received. Useful for audit logging and for the API response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RestoreSummary {
    pub tables:    Vec<RestoredTable>,
    pub total_rows: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RestoredTable {
    pub table_name: String,
    pub row_count:  i64,
}

/// DESTRUCTIVE: drops all rows from each table in `RESTORABLE_TABLES`, then
/// re-inserts them from the backup identified by `backup_id`. Runs inside a
/// single SQLite transaction — either the whole restore commits or none of it
/// does. Foreign keys are checked via `PRAGMA defer_foreign_keys = ON` so the
/// parent rows can be inserted before their children.
pub fn restore_from_dump(
    conn: &mut Connection,
    dumps: &[(String, serde_json::Value)],
) -> Result<RestoreSummary> {
    let allowed: HashSet<&str> = RESTORABLE_TABLES.iter().copied().collect();
    let tx = conn.transaction().context("opening restore transaction")?;

    // Defer FK checks until commit so we can wipe tables in any order.
    tx.execute_batch("PRAGMA defer_foreign_keys = ON;")
        .context("deferring foreign keys")?;

    for table in RESTORABLE_TABLES {
        tx.execute(&format!("DELETE FROM \"{table}\""), [])
            .with_context(|| format!("clearing {table}"))?;
    }

    let mut summary = RestoreSummary { tables: vec![], total_rows: 0 };

    for (table_name, rows_value) in dumps {
        if !allowed.contains(table_name.as_str()) {
            // Defensive: skip any table the caller is not allowed to restore.
            continue;
        }
        let inserted = insert_rows_into_table(&tx, table_name, rows_value)
            .with_context(|| format!("inserting into {table_name}"))?;
        summary.total_rows += inserted;
        summary.tables.push(RestoredTable {
            table_name: table_name.clone(),
            row_count:  inserted,
        });
    }

    tx.commit().context("committing restore transaction")?;
    Ok(summary)
}

fn insert_rows_into_table(
    tx: &Transaction<'_>,
    table_name: &str,
    rows_value: &serde_json::Value,
) -> Result<i64> {
    let rows = rows_value
        .as_array()
        .ok_or_else(|| anyhow!("backup payload for {table_name} is not an array"))?;

    if rows.is_empty() {
        return Ok(0);
    }

    // Pull the column list from the first row to drive INSERT. All rows in a
    // single backup are expected to have the same shape; if a later row is
    // missing a key, we bind NULL.
    let first = rows
        .first()
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow!("first row of {table_name} is not an object"))?;
    let columns: Vec<&str> = first.keys().map(String::as_str).collect();

    let placeholders = (1..=columns.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let col_list = columns
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("INSERT INTO \"{table_name}\" ({col_list}) VALUES ({placeholders})");

    let mut stmt = tx.prepare(&sql).with_context(|| format!("preparing {table_name}"))?;
    let mut inserted: i64 = 0;
    for row in rows {
        let obj = row
            .as_object()
            .ok_or_else(|| anyhow!("row in {table_name} is not an object"))?;
        let mut bound: Vec<Box<dyn rusqlite::ToSql>> = Vec::with_capacity(columns.len());
        for col in &columns {
            let v = obj.get(*col).cloned().unwrap_or(serde_json::Value::Null);
            bound.push(json_to_tosql(v));
        }
        let param_refs: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
        stmt.execute(param_refs.as_slice())
            .with_context(|| format!("inserting row into {table_name}"))?;
        inserted += 1;
    }
    Ok(inserted)
}

fn json_to_tosql(v: serde_json::Value) -> Box<dyn rusqlite::ToSql> {
    match v {
        serde_json::Value::Null => Box::new(Option::<String>::None),
        serde_json::Value::Bool(b) => Box::new(b as i64),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Box::new(i)
            } else if let Some(f) = n.as_f64() {
                Box::new(f)
            } else {
                Box::new(n.to_string())
            }
        }
        serde_json::Value::String(s) => Box::new(s),
        // Arrays and objects are stored as their JSON string representation.
        other => Box::new(other.to_string()),
    }
}

/// Build the `[(table_name, rows_value); N]` vector needed by `restore_from_dump`
/// from a backup stored in Postgres. Tables not present in the backup are
/// skipped silently — restore only re-inserts what was captured.
pub async fn fetch_restore_payload(
    pool: &PgPool,
    backup_id: uuid::Uuid,
) -> Result<Vec<(String, serde_json::Value)>> {
    use crate::backup::client::list_tables_for_backup;

    let table_list = list_tables_for_backup(pool, backup_id).await?;
    let mut payload = Vec::with_capacity(table_list.len());
    for (table_name, _row_count) in table_list {
        if let Some(rows) = fetch_table_dump(pool, backup_id, &table_name).await? {
            payload.push((table_name, rows));
        }
    }
    Ok(payload)
}
