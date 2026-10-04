//! Jev shadow decisions over real pull requests (factory F3, ADR 7d6f870f).
//!
//! Usage (in the worker container, which has the GitHub and Jev credentials):
//!   factory-shadow backfill <org_id> <owner/repo> [--since YYYY-MM-DD] [--max N]
//!   factory-shadow refresh  <org_id>
//!   factory-shadow report   <org_id>
//!
//! `backfill` asks Jev about already merged PRs (their outcome is known at once
//! when the 7-day window has passed). `refresh` settles pending decisions,
//! including those the PR reviewer records live. `report` prints the per-class
//! false-low-risk rate against the OD-5 bar. Nothing here changes what the
//! factory does. `DB_PATH` defaults to /data/nexusmind.db.

use nexusmind::{
    automation::{connectors, merge_gate, shadow},
    db::{connection::connect, factory_queries},
    factory::jev,
    store::sqlite::SqliteStore,
};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|at| args.get(at + 1).cloned())
}

fn canonical_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.hyphenated().to_string() == value)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(command), Some(org_id)) = (args.get(1), args.get(2)) else {
        anyhow::bail!("usage: factory-shadow <backfill|refresh|report> <org_id> ...");
    };
    if !canonical_uuid(org_id) {
        anyhow::bail!("org_id must be a canonical UUID");
    }
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "/data/nexusmind.db".into());
    let store = SqliteStore::new(connect(&db_path)?);
    match command.as_str() {
        "report" => report(&store, org_id),
        "refresh" => {
            let token = nexusmind::automation::worker::server_github_token().await?;
            refresh(&store, org_id, &token).await
        }
        "backfill" => {
            let repository = args.get(3).ok_or_else(|| anyhow::anyhow!("missing owner/repo"))?;
            connectors::validate_repository(repository)?;
            let since = match arg(&args, "--since") {
                Some(day) => {
                    chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d")?;
                    format!("{day}T00:00:00Z")
                }
                None => (chrono::Utc::now() - chrono::Duration::days(90))
                    .format("%Y-%m-%dT00:00:00Z")
                    .to_string(),
            };
            let max: usize = arg(&args, "--max").map_or(Ok(200), |m| m.parse())?;
            let key = shadow::shadow_key().ok_or_else(|| anyhow::anyhow!("TYPESAFE_API_KEY not set"))?;
            if !shadow::shadow_enabled_for(org_id) {
                anyhow::bail!("org {org_id} is not in FACTORY_JEV_SHADOW_ORGS: its PR data may not go to the decision model");
            }
            let token = nexusmind::automation::worker::server_github_token().await?;
            backfill(&store, org_id, &token, &key, repository, &since, max).await
        }
        other => anyhow::bail!("unknown command {other}"),
    }
}

async fn backfill(
    store: &SqliteStore,
    org_id: &str,
    token: &str,
    key: &str,
    repository: &str,
    since: &str,
    max: usize,
) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let pulls = connectors::list_merged_pulls(token, repository, since, max).await?;
    let (mut recorded, mut skipped, mut failed) = (0, 0, 0);
    for pull in &pulls {
        let number = pull.get("number").and_then(|v| v.as_i64()).unwrap_or_default();
        let Some((title, body, head_sha)) = shadow::pull_summary(pull) else {
            failed += 1;
            continue;
        };
        let exists = {
            let db = store.conn();
            let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
            factory_queries::shadow_decision_exists(&conn, org_id, repository, number, &head_sha)?
        };
        if exists {
            skipped += 1;
            continue;
        }
        let result: anyhow::Result<()> = async {
            let files = connectors::list_github_pull_files(token, repository, number).await?;
            let floor = merge_gate::auto_merge_path_verdict(&files).err();
            let paths: Vec<String> = files.into_iter().map(|f| f.filename).collect();
            let change = jev::Change { title: &title, description: &body, paths: &paths };
            let (answer, latency_ms) = jev::decide(&client, key, &change).await?;
            let (outcome, signals) = shadow::collect_outcome(token, repository, pull).await?;
            let db = store.conn();
            let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
            factory_queries::record_shadow_decision(
                &conn,
                org_id,
                &factory_queries::ShadowDecision {
                    provider: "jev",
                    repository,
                    pull_number: number,
                    head_sha: &head_sha,
                    answer: &answer,
                    verdict: jev::verdict(&answer),
                    floor: floor.as_deref(),
                    latency_ms,
                },
            )?;
            let id = factory_queries::shadow_decision_id(&conn, org_id, repository, number, &head_sha)?
                .ok_or_else(|| anyhow::anyhow!("shadow_decision_missing"))?;
            factory_queries::set_shadow_outcome(
                &conn,
                org_id,
                &id,
                pull.get("merged_at").and_then(|v| v.as_str()),
                pull.get("merge_commit_sha").and_then(|v| v.as_str()),
                outcome,
                &signals,
            )?;
            println!(
                "{}",
                serde_json::json!({"pull": number, "class": answer.task_class, "risk": answer.risk,
                    "verdict": jev::verdict(&answer), "floor": floor, "outcome": outcome, "signals": signals})
            );
            Ok(())
        }
        .await;
        match result {
            Ok(()) => recorded += 1,
            Err(error) => {
                failed += 1;
                eprintln!("pull {number}: {error:#}");
            }
        }
    }
    println!(
        "{}",
        serde_json::json!({"summary": {"repository": repository, "merged_pulls": pulls.len(),
            "recorded": recorded, "already_recorded": skipped, "failed": failed}})
    );
    Ok(())
}

async fn refresh(store: &SqliteStore, org_id: &str, token: &str) -> anyhow::Result<()> {
    let summary = shadow::refresh_pending(store, org_id, token).await?;
    println!("{}", serde_json::json!({"summary": summary}));
    Ok(())
}

fn report(store: &SqliteStore, org_id: &str) -> anyhow::Result<()> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
    for class in factory_queries::shadow_report(&conn, org_id)? {
        println!("{}", serde_json::to_string(&class)?);
    }
    Ok(())
}
