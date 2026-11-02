//! Requested-review subscriptions are scoped to org/repo/PR and the current gh
//! account. Explicit removal/closure disables follow-up synchronize events.
use anyhow::Result;
use rusqlite::{params, Connection};
use serde_json::Value;

pub fn migrate(c: &Connection) -> Result<()> {
    if c.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))? >= 95 {
        return Ok(());
    }
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
    tx.execute_batch("CREATE TABLE github_review_subscriptions (
      org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
      repository TEXT NOT NULL,number INTEGER NOT NULL,login TEXT NOT NULL,active INTEGER NOT NULL,
      updated_at TEXT NOT NULL DEFAULT (datetime('now')),PRIMARY KEY(org_id,repository,number,login));
      PRAGMA user_version=95;")?;
    tx.commit()?;
    Ok(())
}
pub fn explicitly_requested(pull: &Value, login: &str) -> bool {
    pull["requested_reviewers"].as_array().is_some_and(|users| {
        users.iter().any(|u| {
            u["login"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(login))
        })
    })
}
pub fn subscribed(c: &Connection, org: &str, repo: &str, number: i64, login: &str) -> Result<bool> {
    let repo = repo.to_ascii_lowercase();
    Ok(c.query_row("SELECT EXISTS(SELECT 1 FROM github_review_subscriptions WHERE org_id=?1 AND repository=?2 AND number=?3 AND login=?4 AND active=1)",params![org,repo,number,login.to_ascii_lowercase()],|r|r.get(0))?)
}
pub fn set(
    c: &Connection,
    org: &str,
    repo: &str,
    number: i64,
    login: &str,
    active: bool,
) -> Result<()> {
    let repo = repo.to_ascii_lowercase();
    c.execute("INSERT INTO github_review_subscriptions(org_id,repository,number,login,active) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(org_id,repository,number,login) DO UPDATE SET active=excluded.active,updated_at=datetime('now')",params![org,repo,number,login.to_ascii_lowercase(),active])?;
    Ok(())
}
pub fn cancel_obsolete(
    c: &Connection,
    org: &str,
    repo: &str,
    number: i64,
    keep_head: Option<&str>,
) -> Result<()> {
    let repo = repo.to_ascii_lowercase();
    let ids = {
        let mut s=c.prepare("SELECT DISTINCT r.id FROM autonomous_agent_runs r JOIN autonomous_agent_work_items w ON w.run_id=r.id WHERE r.org_id=?1 AND lower(w.repository)=?2 AND w.kind='github_pr' AND w.external_number=?3 AND r.trigger_kind='github_webhook' AND r.status IN ('queued','leased','running') AND (?4 IS NULL OR w.head_sha!=?4)")?;
        let rows = s
            .query_map(params![org, repo, number, keep_head], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for id in ids {
        super::queries::cancel_autonomous_agent_run(c, org, &id)?;
    }
    Ok(())
}
pub fn route(c: &Connection, org: &str, repo: &str, payload: &Value, login: &str) -> Result<bool> {
    let Some(number) = payload["pull_request"]["number"]
        .as_i64()
        .filter(|n| *n > 0)
    else {
        return Ok(false);
    };
    let action = payload["action"].as_str().unwrap_or_default();
    let addressed = payload["requested_reviewer"]["login"]
        .as_str()
        .is_some_and(|s| s.eq_ignore_ascii_case(login));
    if action == "closed" || action == "review_request_removed" && addressed {
        if action == "closed" {
            c.execute("UPDATE github_review_subscriptions SET active=0 WHERE org_id=?1 AND repository=?2 AND number=?3",params![org,repo.to_ascii_lowercase(),number])?;
        } else {
            set(c, org, repo, number, login, false)?;
        }
        cancel_obsolete(c, org, repo, number, None)?;
        return Ok(false);
    }
    let pull = &payload["pull_request"];
    if pull["state"] != "open"
        || pull["user"]["login"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case(login))
        || pull.pointer("/head/repo/fork").and_then(Value::as_bool) != Some(false)
    {
        return Ok(false);
    }
    let requested = explicitly_requested(pull, login) || action == "review_requested" && addressed;
    if requested {
        set(c, org, repo, number, login, true)?;
    }
    Ok([
        "opened",
        "reopened",
        "synchronize",
        "ready_for_review",
        "review_requested",
    ]
    .contains(&action)
        && (requested || subscribed(c, org, repo, number, login)?))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn follows_only_requested_account_and_respects_removal() {
        let c = crate::db::connection::connect(":memory:").unwrap();
        crate::db::migrations::run(&c).unwrap();
        let (o, _, _) =
            crate::db::queries::bootstrap(&c, "Org", "org", "a@test.com", "Owner").unwrap();
        let mut p = json!({"action":"opened","pull_request":{"number":4,"state":"open","user":{"login":"author"},"head":{"repo":{"fork":false}},"requested_reviewers":[]}});
        assert!(!route(&c, &o.id, "acme/api", &p, "reviewer").unwrap());
        p["action"] = json!("review_requested");
        p["requested_reviewer"] = json!({"login":"Reviewer"});
        assert!(route(&c, &o.id, "acme/api", &p, "reviewer").unwrap());
        p["action"] = json!("synchronize");
        p.as_object_mut().unwrap().remove("requested_reviewer");
        assert!(route(&c, &o.id, "acme/api", &p, "reviewer").unwrap());
        assert!(!route(&c, &o.id, "acme/api", &p, "another-user").unwrap());
        p["action"] = json!("review_request_removed");
        p["requested_reviewer"] = json!({"login":"reviewer"});
        assert!(!route(&c, &o.id, "acme/api", &p, "reviewer").unwrap());
        p["action"] = json!("synchronize");
        assert!(!route(&c, &o.id, "acme/api", &p, "reviewer").unwrap());
    }
}
