//! Frozen eval of the factory specialists against the generic resolver (F4
//! exit; see `automation::specialist_eval` and docs/factory/specialist-eval.md).
//! Runs inside the worker container, which has the sandbox ServiceAccount, the
//! egress proxy key, the database and the server's GitHub login:
//!
//!   factory-specialist-eval <org_id> [--arms baseline,specialist]
//!       [--max-turns 150] [--wall-time-secs 3600] < tasks.jsonl
//!
//! Each stdin line is an eval task. One JSON outcome per task and arm is printed
//! as it finishes, then a summary line. Nothing is published.

use nexusmind::{
    automation::specialist_eval::{run_eval, summarize, validate_task, Arm, EvalTask, SandboxEvalRunner},
    db,
    store::sqlite::SqliteStore,
};
use std::io::BufRead;

const USAGE: &str = "usage: factory-specialist-eval <org_id> [--arms baseline,specialist] [--max-turns N] [--wall-time-secs N] < tasks.jsonl";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let org_id = args.next().filter(|a| !a.starts_with("--")).ok_or_else(|| anyhow::anyhow!(USAGE))?;
    let mut arms = vec![Arm::Baseline, Arm::Specialist];
    let mut max_turns: u64 = 150;
    let mut wall_time_secs: u64 = 3600;
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| anyhow::anyhow!(USAGE))?;
        match flag.as_str() {
            "--arms" => {
                arms = value
                    .split(',')
                    .map(|arm| Arm::parse(arm.trim()).ok_or_else(|| anyhow::anyhow!("unknown arm {arm}")))
                    .collect::<anyhow::Result<_>>()?
            }
            "--max-turns" => max_turns = value.parse::<u64>()?.clamp(1, 400),
            "--wall-time-secs" => wall_time_secs = value.parse::<u64>()?.clamp(30, 3600),
            _ => anyhow::bail!(USAGE),
        }
    }
    let tasks: Vec<EvalTask> = std::io::stdin()
        .lock()
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(&line))
        .collect::<Result<_, _>>()?;
    // Every task is checked before the first pod starts; ids are unique so no
    // two attempts share run labels.
    let mut seen = std::collections::HashSet::new();
    for task in &tasks {
        validate_task(task).map_err(|error| anyhow::anyhow!("task {}: {error}", task.id))?;
        if !seen.insert(task.id.clone()) {
            anyhow::bail!("duplicate task id {}", task.id)
        }
    }
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "/data/nexusmind.db".into());
    let runner = SandboxEvalRunner {
        store: SqliteStore::new(db::connection::connect(&db_path)?),
        org_id,
        github_token: nexusmind::automation::worker::server_github_token().await?,
        max_turns,
        wall_time: std::time::Duration::from_secs(wall_time_secs),
    };
    let run = run_eval(&runner, &tasks, &arms, |outcome| {
        println!("{}", serde_json::to_string(outcome).unwrap_or_default())
    })
    .await;
    println!(
        "{}",
        serde_json::json!({ "summary": summarize(&run.outcomes), "stopped_at": run.stopped_at })
    );
    match run.stopped_at {
        // Resume by feeding the tasks from this id onwards.
        Some(id) => anyhow::bail!("provider error at task {id} (usage limit or API error); the summary covers the tasks before it"),
        None => Ok(()),
    }
}
