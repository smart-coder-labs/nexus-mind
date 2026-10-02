//! Replays golden tasks through the F1 verification gate (see
//! `automation::golden_replay`). Runs inside the worker container, which has the
//! sandbox ServiceAccount, the database and the server's GitHub login:
//!
//!   factory-golden-replay <org_id> < tasks.jsonl
//!
//! Each stdin line is a golden task (`id`, `repository`, `merge_sha`,
//! `changed_files`; other fields are ignored). One JSON outcome per task is
//! printed, then a summary line.

use futures_util::StreamExt;
use nexusmind::{
    automation::golden_replay::{replay_task, GoldenTask},
    db,
    store::sqlite::SqliteStore,
};
use std::io::BufRead;

/// One task pod at a time: the node's CPU requests leave room for a single
/// sandbox pod, and a second one stays Pending until its ready timeout fails it.
const CONCURRENCY: usize = 1;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let org_id = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: factory-golden-replay <org_id> < tasks.jsonl"))?;
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "/data/nexusmind.db".into());
    let store = SqliteStore::new(db::connection::connect(&db_path)?);
    let token = nexusmind::automation::worker::server_github_token().await?;
    let tasks: Vec<GoldenTask> = std::io::stdin()
        .lock()
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(&line))
        .collect::<Result<_, _>>()?;
    // One run label per task id: a duplicate would share a pod slot, and the
    // orphan cleanup of one would delete the other's pod.
    let mut seen = std::collections::HashSet::new();
    let tasks: Vec<GoldenTask> = tasks
        .into_iter()
        .filter(|task| seen.insert(task.id.clone()))
        .collect();
    let total = tasks.len();
    let outcomes: Vec<_> = futures_util::stream::iter(tasks.iter())
        .map(|task| replay_task(&store, &org_id, &token, task))
        .buffer_unordered(CONCURRENCY)
        .inspect(|outcome| println!("{}", serde_json::to_string(outcome).unwrap_or_default()))
        .collect()
        .await;
    let errors = outcomes.iter().filter(|o| o.error.is_some()).count();
    let skipped = outcomes
        .iter()
        .filter(|o| o.error.is_none() && o.skipped)
        .count();
    let passed = outcomes
        .iter()
        .filter(|o| o.error.is_none() && o.passed)
        .count();
    let failed = total - errors - skipped - passed;
    println!(
        "{}",
        serde_json::json!({"summary": {"tasks": total, "passed": passed, "failed": failed, "skipped_no_evidence": skipped, "errors": errors}})
    );
    Ok(())
}
