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

/// Task pods run in parallel; the node has room for a few installs at once.
const CONCURRENCY: usize = 2;

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
    let total = tasks.len();
    let outcomes: Vec<_> = futures_util::stream::iter(tasks.iter())
        .map(|task| replay_task(&store, &org_id, &token, task))
        .buffer_unordered(CONCURRENCY)
        .inspect(|outcome| println!("{}", serde_json::to_string(outcome).unwrap_or_default()))
        .collect()
        .await;
    let passed = outcomes.iter().filter(|o| o.passed).count();
    let errors = outcomes.iter().filter(|o| o.error.is_some()).count();
    println!(
        "{}",
        serde_json::json!({"summary": {"tasks": total, "passed": passed, "not_passed": total - passed - errors, "errors": errors}})
    );
    Ok(())
}
