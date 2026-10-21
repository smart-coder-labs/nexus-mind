//! Durable handoff from the API eval queue to the single autonomous worker.
//! The evaluator itself is the existing restart-safe runner on the shared PVC.

use crate::store::sqlite::SqliteStore;
use rusqlite::OptionalExtension;
const EVAL_DIR: &str = "/data/factory-evals";

/// Claim at most one queued run and launch it out of the worker's claim loop.
/// The database transition is conditional, so overlapping worker ticks cannot
/// launch the same run twice.
pub fn spawn_one(store: SqliteStore) {
    let claim = {
        let db = store.conn();
        let Ok(conn) = db.lock() else { return };
        let tx = match conn.unchecked_transaction() {
            Ok(tx) => tx,
            Err(err) => { tracing::error!("Factory eval claim transaction failed: {err}"); return }
        };
        let queued: Option<(String, String, String)> = match tx.query_row(
            "SELECT id,org_id,set_ids_json FROM factory_eval_runs WHERE status='queued' ORDER BY created_at LIMIT 1",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional() {
            Ok(value) => value,
            Err(err) => { tracing::error!("Factory eval queue read failed: {err}"); return }
        };
        let Some((id, org_id, set_ids_json)) = queued else { return };
        let set_ids: Vec<String> = match serde_json::from_str(&set_ids_json) {
            Ok(ids) if !ids.is_empty() && ids.iter().all(|id| ["docs", "tests", "ui"].contains(&id.as_str())) => ids,
            _ => {
                let _ = tx.execute("UPDATE factory_eval_runs SET status='failed',error='invalid stored eval set selection',finished_at=datetime('now') WHERE id=?1", [&id]);
                let _ = tx.commit();
                return;
            }
        };
        match tx.execute("UPDATE factory_eval_runs SET status='running',started_at=datetime('now') WHERE id=?1 AND status='queued'", [&id]) {
            Ok(1) => {},
            Ok(_) => return,
            Err(err) => { tracing::error!("Factory eval claim update failed: {err}"); return }
        }
        if let Err(err) = tx.commit() {
            tracing::error!("Factory eval claim commit failed: {err}");
            return;
        }
        (id, org_id, set_ids)
    };

    tokio::spawn(async move {
        let (id, org_id, set_ids) = claim;
        tracing::info!(eval_id = %id, "Starting queued factory specialist eval");
        let mut command = tokio::process::Command::new("node");
        command
            .arg(format!("{EVAL_DIR}/run.mjs"))
            .arg("--org").arg(&org_id)
            .arg("--dir").arg(EVAL_DIR);
        for set_id in &set_ids { command.arg("--set").arg(set_id); }
        let runner = command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .status();
        let result = runner.await;
        let (status, error) = match result {
            Ok(exit) if exit.success() => {
                let unfinished = std::fs::read_dir(EVAL_DIR).ok().into_iter().flatten().filter_map(Result::ok).any(|entry| {
                    let path = entry.path();
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                    let selected = set_ids.iter().any(|id| name == format!("{id}.jsonl") || name.ends_with(&format!("-{id}.jsonl")));
                    selected && name.ends_with(".jsonl") && !name.ends_with(".results.jsonl") && !name.starts_with('.')
                        && !path.with_extension("done").is_file()
                });
                if unfinished { ("failed", Some("runner exited before every frozen set completed".to_owned())) }
                else { ("complete", None) }
            }
            Ok(exit) => ("failed", Some(format!("runner exited with status {}", exit.code().unwrap_or(-1)))),
            Err(err) => ("failed", Some(format!("runner process could not start: {err}"))),
        };
        let mut summaries = serde_json::Map::new();
        if let Ok(entries) = std::fs::read_dir(EVAL_DIR) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
                if let Some(set) = name.strip_suffix(".summary.json").and_then(|set| set_ids.iter().find(|id| set == id.as_str() || set.ends_with(&format!("-{id}")))) {
                    if let Ok(contents) = std::fs::read_to_string(path) {
                        if let Ok(value) = serde_json::from_str(&contents) { summaries.insert(set.clone(), value); }
                    }
                }
            }
        }
        let summary_value = if set_ids.len() == 1 {
            summaries.iter().find(|(name, _)| name == &set_ids[0] || name.ends_with(&format!("-{}", set_ids[0]))).map(|(_, value)| value.clone()).unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::Value::Object(summaries)
        };
        let summary_json = summary_value.to_string();
        let db = store.conn();
        if let Ok(conn) = db.lock() {
            if let Err(err) = conn.execute(
                "UPDATE factory_eval_runs SET status=?2,error=?3,summary_json=?4,finished_at=datetime('now') WHERE id=?1 AND status='running'",
                rusqlite::params![id, status, error, summary_json],
            ) {
                tracing::error!(eval_id = %id, "Failed to persist factory eval result: {err}");
            }
        }
        tracing::info!(eval_id = %id, status, "Factory specialist eval finished");
    });
}
