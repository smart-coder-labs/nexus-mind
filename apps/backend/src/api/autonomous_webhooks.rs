use crate::{
    db::{github_review_hooks, queries},
    models::types::ApiError,
    store::sqlite::SqliteStore,
};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
fn reject(status: StatusCode, code: &str) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            error: code.replace('_', " "),
            code: code.into(),
        }),
    )
}
pub(crate) fn verify_signature(secret: &str, body: &[u8], header: &str) -> bool {
    let Some(value) = header.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(signature) = hex::decode(value) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&signature).is_ok()
}
fn github_trigger<'a>(
    event: &str,
    payload: &'a serde_json::Value,
) -> Option<(&'static str, i64, Option<&'a str>)> {
    match (event, payload["action"].as_str()) {
        ("issues", Some("opened" | "reopened" | "labeled")) => payload
            .pointer("/issue/number")
            .and_then(|v| v.as_i64())
            .map(|n| ("github_issue", n, None)),
        (
            "pull_request",
            Some("opened" | "reopened" | "synchronize" | "ready_for_review" | "review_requested"),
        ) if payload
            .pointer("/pull_request/head/repo/fork")
            .and_then(|v| v.as_bool())
            != Some(true) =>
        {
            payload
                .pointer("/pull_request/number")
                .and_then(|v| v.as_i64())
                .map(|n| {
                    (
                        "github_pr",
                        n,
                        payload
                            .pointer("/pull_request/head/sha")
                            .and_then(|v| v.as_str()),
                    )
                })
        }
        _ => None,
    }
}
fn closed_issue_status(payload: &serde_json::Value) -> Option<&'static str> {
    if payload["action"] != "closed" || payload["issue"]["state"] != "closed" { return None }
    match payload["issue"]["state_reason"].as_str() {
        Some("completed") => Some("resolved"),
        Some("not_planned") => Some("ignored"),
        _ => None, // A missing/unknown reason is not proof of resolution.
    }
}
fn stale_issue_event(payload: &serde_json::Value, live: &serde_json::Value) -> bool {
    live["state"] != "closed" || live["state_reason"] != payload["issue"]["state_reason"]
}
fn stale_review_event(payload: &serde_json::Value, live: &serde_json::Value, login: &str) -> bool {
    let action = payload["action"].as_str().unwrap_or_default();
    let stale = match action {
        "closed" => live["state"] != "closed",
        "review_request_removed" => github_review_hooks::explicitly_requested(live, login),
        "review_requested" => {
            !github_review_hooks::explicitly_requested(live, login)
                || live.pointer("/head/sha") != payload.pointer("/pull_request/head/sha")
        }
        _ => live.pointer("/head/sha") != payload.pointer("/pull_request/head/sha"),
    };
    stale
        || (live["state"] == "closed" && !matches!(action, "closed" | "review_request_removed"))
        || (!matches!(action, "closed" | "review_request_removed")
            && github_review_hooks::explicitly_requested(&payload["pull_request"], login)
            && !github_review_hooks::explicitly_requested(live, login))
}
pub async fn github_webhook(
    State(store): State<SqliteStore>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    handle(store, None, headers, body).await
}
pub async fn github_cli_webhook(
    State(store): State<SqliteStore>,
    Path(connector): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    handle(store, Some(connector), headers, body).await
}
async fn handle(
    store: SqliteStore,
    selector: Option<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if body.len() > 2 * 1024 * 1024 {
        return Err(reject(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"));
    }
    let delivery = headers
        .get("x-github-delivery")
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty() && v.len() <= 128)
        .ok_or_else(|| reject(StatusCode::BAD_REQUEST, "missing_delivery_id"))?;
    let event = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| reject(StatusCode::BAD_REQUEST, "missing_event"))?;
    if ![
        "issues",
        "pull_request",
        "pull_request_review",
        "installation",
        "installation_repositories",
    ]
    .contains(&event)
    {
        return Ok(StatusCode::ACCEPTED);
    }
    let payload: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|_| reject(StatusCode::BAD_REQUEST, "invalid_json"))?;
    let repository = payload
        .pointer("/repository/full_name")
        .and_then(|v| v.as_str());
    let (org, connector, raw, secret) = {
        let h = store.conn();
        let c = h
            .lock()
            .map_err(|_| reject(StatusCode::INTERNAL_SERVER_ERROR, "database_lock"))?;
        if let Some(id) = selector {
            let org:Option<String>=c.query_row("SELECT org_id FROM autonomous_agent_connectors WHERE id=?1 AND kind='target_secret' AND health!='revoked' AND json_extract(metadata_json,'$.purpose')='github_cli_webhook'",[&id],|r|r.get(0)).ok();
            let org = org.ok_or_else(|| reject(StatusCode::NOT_FOUND, "webhook_not_found"))?;
            let (meta, secret) = queries::get_autonomous_agent_connector_secret(&c, &org, &id)
                .map_err(|_| {
                    reject(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "webhook_secret_unavailable",
                    )
                })?
                .ok_or_else(|| reject(StatusCode::NOT_FOUND, "webhook_not_found"))?;
            if !meta.metadata["repository"]
                .as_str()
                .zip(repository)
                .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
            {
                return Err(reject(StatusCode::FORBIDDEN, "webhook_repository_scope"));
            }
            if event != "pull_request" && !(event == "issues" && payload["action"] == "closed") {
                return Ok(StatusCode::ACCEPTED);
            }
            (org, id, secret.clone(), secret)
        } else {
            let installation = payload
                .pointer("/installation/id")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| reject(StatusCode::BAD_REQUEST, "missing_installation"))?;
            let (org, id, raw) = queries::find_github_app_webhook_connector(&c, installation)
                .map_err(|_| reject(StatusCode::INTERNAL_SERVER_ERROR, "database_error"))?
                .ok_or_else(|| reject(StatusCode::NOT_FOUND, "installation_not_found"))?;
            let secrets: serde_json::Value = serde_json::from_str(&raw)
                .map_err(|_| reject(StatusCode::SERVICE_UNAVAILABLE, "github_secret_invalid"))?;
            let secret = secrets["webhook_secret"]
                .as_str()
                .ok_or_else(|| {
                    reject(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "github_webhook_secret_missing",
                    )
                })?
                .to_string();
            (org, id, raw, secret)
        }
    };
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| reject(StatusCode::UNAUTHORIZED, "missing_signature"))?;
    if !verify_signature(&secret, &body, signature) {
        return Err(reject(StatusCode::UNAUTHORIZED, "invalid_signature"));
    }
    let (reviewer, stale) = if event == "pull_request" {
        let repo =
            repository.ok_or_else(|| reject(StatusCode::BAD_REQUEST, "repository_missing"))?;
        crate::automation::connectors::validate_repository(repo)
            .map_err(|_| reject(StatusCode::BAD_REQUEST, "invalid_repository"))?;
        let number = payload
            .pointer("/pull_request/number")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| reject(StatusCode::BAD_REQUEST, "pull_request_number_missing"))?;
        let token = crate::automation::worker::server_github_token()
            .await
            .map_err(|_| reject(StatusCode::SERVICE_UNAVAILABLE, "github_cli_unavailable"))?;
        let login = crate::automation::worker::github_identity_for_token(&token)
            .await
            .map_err(|_| {
                reject(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "github_cli_identity_unavailable",
                )
            })?;
        let live = crate::automation::connectors::get_github_pull(&token, repo, number)
            .await
            .map_err(|_| reject(StatusCode::SERVICE_UNAVAILABLE, "github_pull_unavailable"))?;
        let stale = stale_review_event(&payload, &live, &login);
        (Some(login), stale)
    } else if event == "issues" && closed_issue_status(&payload).is_some() {
        let repo = repository.ok_or_else(|| reject(StatusCode::BAD_REQUEST, "repository_missing"))?;
        crate::automation::connectors::validate_repository(repo).map_err(|_| reject(StatusCode::BAD_REQUEST, "invalid_repository"))?;
        let number = payload["issue"]["number"].as_i64().filter(|n| *n>0)
            .ok_or_else(|| reject(StatusCode::BAD_REQUEST, "issue_number_missing"))?;
        let token = crate::automation::worker::server_github_token().await
            .map_err(|_| reject(StatusCode::SERVICE_UNAVAILABLE, "github_cli_unavailable"))?;
        let live = crate::automation::connectors::get_github_issue(&token, repo, number).await
            .map_err(|_| reject(StatusCode::SERVICE_UNAVAILABLE, "github_issue_unavailable"))?;
        (None, stale_issue_event(&payload, &live))
    } else {
        (None, false)
    };
    let h = store.conn();
    let c = h
        .lock()
        .map_err(|_| reject(StatusCode::INTERNAL_SERVER_ERROR, "database_lock"))?;
    let current =
        queries::get_autonomous_agent_connector_secret(&c, &org, &connector).map_err(|_| {
            reject(
                StatusCode::SERVICE_UNAVAILABLE,
                "webhook_secret_unavailable",
            )
        })?;
    if !current.is_some_and(|(_, secret)| secret == raw) {
        return Err(reject(
            StatusCode::UNAUTHORIZED,
            "webhook_revoked_or_rotated",
        ));
    }
    let tx = rusqlite::Transaction::new_unchecked(&c, rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| reject(StatusCode::INTERNAL_SERVER_ERROR, "database_error"))?;
    let payload_hash = hex::encode(Sha256::digest(&body));
    process_delivery(
        &tx,
        &org,
        &connector,
        delivery,
        event,
        &payload,
        reviewer.as_deref(),
        &payload_hash,
        !stale,
    )
    .map_err(|e| {
        if e.to_string() == "github_delivery_payload_mismatch" {
            reject(StatusCode::CONFLICT, "delivery_payload_mismatch")
        } else {
            reject(StatusCode::INTERNAL_SERVER_ERROR, "trigger_enqueue_failed")
        }
    })?;
    tx.commit()
        .map_err(|_| reject(StatusCode::INTERNAL_SERVER_ERROR, "database_error"))?;
    Ok(StatusCode::ACCEPTED)
}
#[allow(clippy::too_many_arguments)]
fn process_delivery(
    c: &rusqlite::Connection,
    org: &str,
    connector: &str,
    delivery: &str,
    event: &str,
    payload: &serde_json::Value,
    reviewer: Option<&str>,
    hash: &str,
    current: bool,
) -> anyhow::Result<()> {
    let repository = payload
        .pointer("/repository/full_name")
        .and_then(|v| v.as_str());
    let action = payload["action"].as_str();
    if !queries::record_github_webhook_delivery(
        c, org, connector, delivery, event, action, repository, hash,
    )? {
        return Ok(());
    }
    if !current {
        return Ok(());
    }
    let Some(repo) = repository else {
        return Ok(());
    };
    crate::automation::connectors::validate_repository(repo)?;
    if event == "issues" {
        if let Some(status) = closed_issue_status(payload) {
            let number = payload["issue"]["number"].as_i64().filter(|n| *n>0)
                .ok_or_else(|| anyhow::anyhow!("issue_number_missing"))?;
            // Derive the delivery URL from the repository already scoped to the signed connector.
            queries::close_findings_for_github_issue(c, org, &format!("https://github.com/{repo}/issues/{number}"), status)?;
        }
    }
    if event == "pull_request" {
        let Some(login) = reviewer else { return Ok(()) };
        if !github_review_hooks::route(c, org, repo, payload, login)? {
            return Ok(());
        }
    }
    if let Some((kind, number, head)) = github_trigger(event, payload) {
        if kind == "github_pr" {
            if !head.is_some_and(crate::factory::workflows::valid_sha) {
                return Ok(());
            }
            github_review_hooks::cancel_obsolete(c, org, repo, number, head)?;
        }
        let addressed = payload["requested_reviewer"]["login"].as_str()
            .zip(reviewer).is_some_and(|(a,b)| a.eq_ignore_ascii_case(b));
        let origin = if event == "pull_request" && action == Some("review_requested") && addressed {
            queries::GithubRunOrigin::RequestedReview(delivery)
        } else { queries::GithubRunOrigin::Webhook(delivery) };
        queries::enqueue_github_webhook_agents(c, org, origin, repo, kind, number, head, hash)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn requested_reviews_are_atomic_idempotent_and_superseded_by_new_heads() {
        use serde_json::json;
        let c = crate::db::connection::connect(":memory:").unwrap();
        crate::db::migrations::run(&c).unwrap();
        let (o, u, _) = queries::bootstrap(&c, "Org", "org", "owner@test.com", "Owner").unwrap();
        c.execute("INSERT INTO autonomous_agent_definitions(id,org_id,template_key,template_version,name,status,current_revision,created_by) VALUES ('reviewer',?1,'github_pr_reviewer',1,'Reviewer','enabled',1,?2)",rusqlite::params![o.id,u.id]).unwrap();
        c.execute("INSERT INTO autonomous_agent_revisions(id,definition_id,revision,config_json,config_hash,capabilities_json,budgets_json,validation_status,created_by) VALUES ('rev','reviewer',1,'{\"repository\":\"acme/api\"}','hash','[]','{}','pending',?1)",[&u.id]).unwrap();
        c.execute("INSERT INTO autonomous_agent_validations(id,org_id,definition_id,revision_id,config_hash,status,result_json,validated_by) VALUES ('valid',?1,'reviewer','rev','hash','valid','{}',?2)",rusqlite::params![o.id,u.id]).unwrap();
        assert_eq!(queries::list_github_reconciliation_sources(&c).unwrap().len(),1,"Validation comes from the append-only ledger, not the immutable pending column");
        c.execute("INSERT INTO autonomous_agent_connectors(id,org_id,kind,name,health,metadata_json,created_by) VALUES ('hook',?1,'target_secret','Hook','ready','{}',?2)",rusqlite::params![o.id,u.id]).unwrap();
        let apply = |delivery: &str, payload: &serde_json::Value, current: bool| {
            let tx =
                rusqlite::Transaction::new_unchecked(&c, rusqlite::TransactionBehavior::Immediate)
                    .unwrap();
            let hash = hex::encode(Sha256::digest(serde_json::to_vec(payload).unwrap()));
            process_delivery(
                &tx,
                &o.id,
                "hook",
                delivery,
                "pull_request",
                payload,
                Some("cli-user"),
                &hash,
                current,
            )?;
            tx.commit()?;
            Ok::<(), anyhow::Error>(())
        };
        let mut p = json!({"action":"opened","repository":{"full_name":"acme/api"},"pull_request":{"number":12,"state":"open","user":{"login":"author"},"requested_reviewers":[],"head":{"sha":"a".repeat(40),"repo":{"fork":false}}}});
        apply("opened", &p, true).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM autonomous_agent_runs", [], |r| r
                .get::<_, u32>(0))
                .unwrap(),
            0
        );
        p["action"] = json!("review_requested");
        p["requested_reviewer"] = json!({"login":"CLI-User"});
        p["pull_request"]["requested_reviewers"] = json!([{"login":"cli-user"}]);
        apply("requested", &p, true).unwrap();
        apply("requested", &p, true).unwrap();
        apply("different-delivery-same-head", &p, true).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM autonomous_agent_runs", [], |r| r
                .get::<_, u32>(0))
                .unwrap(),
            1
        );
        let first=queries::list_autonomous_agent_runs(&c,&o.id,None).unwrap().remove(0);
        assert_eq!(first.trigger_origin.as_ref().unwrap()["delivery_id"],"requested");
        assert_eq!(first.trigger_kind,"github_webhook");
        let (active, automation): (String, String) = c
            .query_row(
                "SELECT id,automation_run_id FROM autonomous_agent_runs LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        c.execute(
            "INSERT INTO automation_attempts(id,run_id) VALUES ('old-attempt',?1)",
            [&automation],
        )
        .unwrap();
        c.execute("INSERT INTO autonomous_agent_leases(id,run_id,attempt_id,worker_id,claim_token_hash,expires_at) VALUES ('old-lease',?1,'old-attempt','worker','hash',datetime('now','+2 minutes'))",[&active]).unwrap();
        c.execute(
            "UPDATE autonomous_agent_runs SET status='running' WHERE id=?1",
            [&active],
        )
        .unwrap();
        let stale = p.clone();
        p["action"] = json!("synchronize");
        p["pull_request"]["head"]["sha"] = json!("b".repeat(40));
        apply("new-head", &p, true).unwrap();
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM autonomous_agent_runs WHERE status='queued'",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM autonomous_agent_runs WHERE status='cancelled'",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            c.query_row(
                "SELECT status FROM automation_attempts WHERE id='old-attempt'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "revoked"
        );
        apply("late-old-head", &stale, false).unwrap();
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM autonomous_agent_runs WHERE status='queued'",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
            1
        );
        assert!(apply("requested", &p, true).is_err());
        p["action"] = json!("review_request_removed");
        p["pull_request"]["requested_reviewers"] = json!([]);
        apply("removed", &p, true).unwrap();
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM autonomous_agent_runs WHERE status='queued'",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
            0
        );
        p["action"] = json!("review_requested");
        p["repository"]["full_name"] = json!("ACME/API");
        p["pull_request"]["requested_reviewers"] = json!([{"login":"CLI-USER"}]);
        apply("requested-again", &p, true).unwrap();
        apply("another-delivery", &p, true).unwrap();
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM autonomous_agent_runs WHERE status='queued'",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
            1
        );
        c.execute("UPDATE autonomous_agent_runs SET status='succeeded' WHERE status='queued'", []).unwrap();
        p["action"] = json!("synchronize");
        apply("after-completed-sync", &p, true).unwrap();
        p["action"] = json!("review_requested");
        p["requested_reviewer"] = json!({"login":"another-user"});
        apply("requested-other-account", &p, true).unwrap();
        assert_eq!(c.query_row("SELECT count(*) FROM autonomous_agent_runs WHERE status='queued'", [], |r|r.get::<_,i64>(0)).unwrap(), 0);
        p["requested_reviewer"] = json!({"login":"cli-user"});
        apply("explicit-rereview", &p, true).unwrap();
        apply("explicit-rereview", &p, true).unwrap();
        apply("same-head-active-request", &p, true).unwrap();
        assert_eq!(c.query_row("SELECT count(*) FROM autonomous_agent_runs WHERE status='queued'", [], |r|r.get::<_,i64>(0)).unwrap(), 1, "An explicit new request permits a completed SHA, not a duplicate active run");
        c.execute("UPDATE autonomous_agent_runs SET status='succeeded' WHERE status='queued'",[]).unwrap();
        apply("another-explicit-request-identical-body", &p, true).unwrap();
        assert_eq!(c.query_row("SELECT count(*) FROM autonomous_agent_runs r LEFT JOIN autonomous_agent_work_items w ON w.run_id=r.id WHERE w.id IS NULL",[],|r|r.get::<_,i64>(0)).unwrap(),0,"A new delivery with identical bytes still needs its own work item");
        assert!(queries::enqueue_github_webhook_agents(&c,&o.id,queries::GithubRunOrigin::Webhook("not-received"),"acme/api","github_pr",13,Some(&"c".repeat(40)),"no-evidence").is_err());
        queries::enqueue_github_webhook_agents(&c,&o.id,queries::GithubRunOrigin::Reconciliation("scan-test"),"acme/api","github_pr",13,Some(&"c".repeat(40)),"scan-hash").unwrap();
        let polled=queries::list_autonomous_agent_runs(&c,&o.id,None).unwrap().into_iter().find(|r|r.trigger_kind=="reconcile").unwrap();
        assert_eq!(polled.trigger_origin.as_ref().unwrap()["kind"],"github_reconciliation");assert_eq!(polled.trigger_origin.as_ref().unwrap()["scan_id"],"scan-test");
        github_review_hooks::cancel_obsolete(&c,&o.id,"acme/api",13,None).unwrap();
        assert_eq!(queries::get_autonomous_agent_run(&c,&o.id,&polled.id).unwrap().unwrap().status,"cancelled");
        let old=queries::enqueue_autonomous_agent_run(&c,&o.id,"reviewer","github_webhook","legacy-no-source",None,None).unwrap().unwrap();assert!(old.trigger_origin.is_none());
        let (other,_,_)=queries::create_org(&c,"Other","other","other@test.com","Owner").unwrap();
        for (id,org,channel,url) in [
            ("linked",&o.id,"github_issue","https://github.com/acme/api/issues/7"),
            ("other-org",&other.id,"github_issue","https://github.com/acme/api/issues/7"),
            ("other-issue",&o.id,"github_issue","https://github.com/acme/api/issues/8"),
            ("other-repo",&o.id,"github_issue","https://github.com/acme/other/issues/7"),
            ("not-issue",&o.id,"slack","https://github.com/acme/api/issues/7"),
        ] {
            c.execute("INSERT INTO autonomous_agent_findings(id,org_id,definition_id,run_id,fingerprint,title,severity,summary) VALUES(?1,?2,'reviewer',?3,?1,'Finding','high','Summary')",rusqlite::params![id,org,old.id]).unwrap();
            c.execute("INSERT INTO autonomous_agent_deliveries(id,org_id,run_id,finding_id,channel,idempotency_key,status,external_url) VALUES(?1,?2,?3,?1,?4,?1,'delivered',?5)",rusqlite::params![id,org,old.id,channel,url]).unwrap();
        }
        let status=|id:&str|c.query_row("SELECT status FROM autonomous_agent_findings WHERE id=?1",[id],|r|r.get::<_,String>(0)).unwrap();
        let close=|delivery:&str,payload:&serde_json::Value,current:bool|{
            let tx=rusqlite::Transaction::new_unchecked(&c,rusqlite::TransactionBehavior::Immediate).unwrap();
            let hash=hex::encode(Sha256::digest(serde_json::to_vec(payload).unwrap()));
            process_delivery(&tx,&o.id,"hook",delivery,"issues",payload,None,&hash,current)?;
            tx.commit()?;
            Ok::<(),anyhow::Error>(())
        };
        let mut issue=json!({"action":"closed","repository":{"full_name":"ACME/API"},"issue":{"number":7,"state":"closed","state_reason":"completed","html_url":"https://github.com/acme/other/issues/7"}});
        close("stale-close",&issue,false).unwrap();assert_eq!(status("linked"),"open");
        close("resolved-close",&issue,true).unwrap();close("resolved-close",&issue,true).unwrap();assert_eq!(status("linked"),"resolved");
        issue["issue"]["state_reason"]=json!("not_planned");
        assert!(close("resolved-close",&issue,true).is_err());assert_eq!(status("linked"),"resolved");
        close("archived-close",&issue,true).unwrap();assert_eq!(status("linked"),"ignored");
        issue["issue"]["state_reason"]=serde_json::Value::Null;
        close("unknown-close",&issue,true).unwrap();assert_eq!(status("linked"),"ignored");
        for id in ["other-org","other-issue","other-repo","not-issue"] {assert_eq!(status(id),"open");}
    }
    #[test]
    fn issue_close_reason_and_live_state_are_required() {
        let mut p=json!({"action":"closed","issue":{"state":"closed","state_reason":"completed"}});
        assert_eq!(closed_issue_status(&p),Some("resolved"));
        assert!(!stale_issue_event(&p,&p["issue"]));
        assert!(stale_issue_event(&p,&json!({"state":"open","state_reason":"reopened"})));
        assert!(stale_issue_event(&p,&json!({"state":"closed","state_reason":"not_planned"})));
        p["issue"]["state_reason"]=json!("not_planned");assert_eq!(closed_issue_status(&p),Some("ignored"));
        p["issue"]["state_reason"]=serde_json::Value::Null;assert!(closed_issue_status(&p).is_none());
        p["issue"]["state_reason"]=json!("completed");p["action"]=json!("reopened");assert!(closed_issue_status(&p).is_none());
    }
    #[test]
    fn live_assignment_and_closed_state_fence_out_of_order_events() {
        let mut payload = json!({"action":"review_requested","pull_request":{"head":{"sha":"a".repeat(40)},"requested_reviewers":[{"login":"CLI"}]}});
        let mut live = json!({"state":"open","head":{"sha":"a".repeat(40)},"requested_reviewers":[{"login":"CLI"}]});
        assert!(!stale_review_event(&payload, &live, "cli"));
        live["requested_reviewers"] = json!([]);
        assert!(stale_review_event(&payload, &live, "cli"));
        payload["action"] = json!("review_request_removed");
        assert!(!stale_review_event(&payload, &live, "cli"));
        live["requested_reviewers"] = json!([{"login":"CLI"}]);
        assert!(stale_review_event(&payload, &live, "cli"));
        payload["action"] = json!("synchronize");
        payload["pull_request"]["requested_reviewers"] = json!([]);
        live["requested_reviewers"] = json!([]);
        assert!(!stale_review_event(&payload, &live, "cli"));
        live["head"]["sha"] = json!("b".repeat(40));
        assert!(stale_review_event(&payload, &live, "cli"));
        live["head"] = payload["pull_request"]["head"].clone();
        live["state"] = json!("closed");
        assert!(stale_review_event(&payload, &live, "cli"));
        payload["action"] = json!("closed");
        assert!(!stale_review_event(&payload, &live, "cli"));
    }
    #[test]
    fn github_hmac_rejects_tampering() {
        let mut mac = Hmac::<Sha256>::new_from_slice(b"long-enough-webhook-secret").unwrap();
        mac.update(b"payload");
        let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert!(verify_signature(
            "long-enough-webhook-secret",
            b"payload",
            &signature
        ));
        assert!(!verify_signature(
            "long-enough-webhook-secret",
            b"changed",
            &signature
        ));
        assert!(!verify_signature(
            "long-enough-webhook-secret",
            b"payload",
            "sha1=bad"
        ));
    }
    #[test]
    fn github_trigger_accepts_only_supported_actions_and_rejects_forks() {
        assert!(
            github_trigger("issues", &json!({"action":"edited","issue":{"number":7}})).is_none()
        );
        assert!(github_trigger("pull_request",&json!({"action":"opened","pull_request":{"number":9,"head":{"sha":"abc","repo":{"fork":true}}}})).is_none());
        for action in [
            "opened",
            "review_requested",
            "synchronize",
            "ready_for_review",
        ] {
            assert_eq!(
                github_trigger(
                    "pull_request",
                    &json!({"action":action,"pull_request":{"number":9,"head":{"sha":"abc","repo":{"fork":false}}}})
                ),
                Some(("github_pr", 9, Some("abc")))
            );
        }
    }
}
