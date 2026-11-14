use super::*;
use axum::{
    extract::State,
    http::{Method, Uri},
    Json, Router,
};
use sha1::{Digest, Sha1};
use std::sync::{Arc, Mutex};
fn hash(kind: &str, data: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(format!("{kind} {}\0", data.len()));
    h.update(data);
    hex::encode(h.finalize())
}
#[test]
fn qa_notes_preserve_failures_and_only_report_current_heads() {
    let mut detail = json!({"id":"run-qa","execution":{"revision":3,"heads":{"api":"a".repeat(40)}},"steps":[
        {"id":"functional","result":{"qa_report_only":true,"heads":{"api":"a".repeat(40)},"evidence":[
            {"check":"functional","status":"fail","artifact_digest":"d".repeat(64)},
            {"check":"functional","status":"pass","artifact_digest":"e".repeat(64)}]}}
    ]});
    let notes = qa_notes(&detail);
    assert!(notes.contains("QA findings (non-blocking)"));
    assert!(notes.contains("not marked as passed"));
    assert!(notes.contains("`functional` / `functional`: **failed**"));
    assert!(notes.contains(&"d".repeat(64)));
    assert!(!notes.contains(&"e".repeat(64)));
    detail["steps"][0]["result"]["heads"]["api"] = json!("b".repeat(40));
    assert!(qa_notes(&detail).is_empty(), "stale QA must not annotate another commit");
}

#[test]
fn qa_pr_annotations_update_idempotently_without_overwriting_human_notes() {
    let body = "Human PR description";
    let first = with_qa_notes(body, "First QA report\n").unwrap() + "\nHuman follow-up";
    let updated = with_qa_notes(&first, "Latest QA report\n").unwrap();
    assert!(updated.starts_with(body));
    assert!(updated.ends_with("Human follow-up"));
    assert!(!updated.contains("First QA report"));
    assert_eq!(updated.matches("nexusmind:qa:start").count(), 1);
    assert_eq!(with_qa_notes(&updated, "Latest QA report\n").unwrap(), updated);
    let cleared = with_qa_notes(&updated, "").unwrap();
    assert!(!cleared.contains("Latest QA report"));
    assert!(cleared.contains("Human follow-up"));
    assert!(with_qa_notes("<!-- nexusmind:qa:start -->incomplete", "report").is_err());
}

#[derive(Default)]
struct Remote {
    branch: Option<String>,
    pull: Option<Value>,
    prs: u32,
    refs: u32,
    body_updates: u32,
    conclusion: Option<String>,
    login: Option<String>,
    /// Advanced default-branch head and the paths its new commits touched.
    advanced: Option<(String, Vec<&'static str>)>,
}
async fn github(
    State(state): State<Arc<Mutex<Remote>>>,
    method: Method,
    uri: Uri,
    body: axum::body::Bytes,
) -> Json<Value> {
    let data: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let path = uri.path();
    let mut remote = state.lock().unwrap();
    let value = if path == "/user" {
        json!({"login":remote.login.as_deref().unwrap_or("test-bot")})
    } else if path == "/repos/acme/api" {
        json!({"default_branch":"main","permissions":{"push":true}})
    } else if path.ends_with("/branches/main") {
        let head = remote
            .advanced
            .as_ref()
            .map_or("a".repeat(40), |(sha, _)| sha.clone());
        json!({"commit":{"sha":head}})
    } else if path.contains("/compare/") {
        let (_, files) = remote.advanced.as_ref().unwrap();
        json!({"status":"ahead","files":files.iter().map(|f| json!({"filename":f})).collect::<Vec<_>>()})
    } else if path.contains("/git/commits/") {
        json!({"tree":{"sha":"c".repeat(40)}})
    } else if path.ends_with("/git/blobs") {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data["content"].as_str().unwrap())
            .unwrap();
        json!({"sha":hash("blob",&bytes)})
    } else if path.ends_with("/git/trees") {
        let mut bytes = Vec::new();
        for e in data["tree"].as_array().unwrap() {
            bytes.extend(
                format!(
                    "{} {}\0",
                    e["mode"].as_str().unwrap().trim_start_matches('0'),
                    e["path"].as_str().unwrap()
                )
                .as_bytes(),
            );
            bytes.extend(hex::decode(e["sha"].as_str().unwrap()).unwrap());
        }
        json!({"sha":hash("tree",&bytes)})
    } else if path.ends_with("/git/commits") {
        let text=format!("tree {}\nparent {}\nauthor NexusMind <factory@nexusmind.local> 946684800 +0000\ncommitter NexusMind <factory@nexusmind.local> 946684800 +0000\n\n{}",data["tree"].as_str().unwrap(),data["parents"][0].as_str().unwrap(),data["message"].as_str().unwrap());
        json!({"sha":hash("commit",text.as_bytes())})
    } else if path.contains("/git/ref/heads/") {
        remote
            .branch
            .as_ref()
            .map(|sha| json!({"object":{"sha":sha}}))
            .unwrap_or(Value::Null)
    } else if path.ends_with("/git/refs") {
        remote.refs += 1;
        remote.branch = Some(data["sha"].as_str().unwrap().into());
        json!({"object":{"sha":remote.branch}})
    } else if path.ends_with("/pulls/42") {
        if method == Method::PATCH {
            remote.body_updates += 1;
            remote.pull.as_mut().unwrap()["body"] = data["body"].clone();
        }
        remote.pull.clone().unwrap_or(Value::Null)
    } else if path.ends_with("/check-runs") {
        json!({"check_runs":[{"name":"ci","status":"completed","conclusion":remote.conclusion.as_deref().unwrap_or("success")}]})
    } else if path.ends_with("/status") {
        json!({"statuses":[]})
    } else if path.ends_with("/pulls") && method == Method::GET {
        json!(remote.pull.iter().collect::<Vec<_>>())
    } else if path.ends_with("/pulls") {
        remote.prs += 1;
        assert_eq!(data["draft"], true);
        let pull = json!({"number":42,"html_url":"https://github.com/acme/api/pull/42","head":{"sha":remote.branch},"body":data["body"]});
        remote.pull = Some(pull.clone());
        pull
    } else {
        panic!("Unexpected upstream operation {method} {uri}")
    };
    Json(value)
}
#[tokio::test]
async fn publishes_exact_git_objects_once_and_rejects_revocation() {
    exercise_publication(false).await;
    exercise_publication(true).await;
}
async fn exercise_publication(backend: bool) {
    std::env::set_var(
        "NEXUSMIND_TOKEN_ENCRYPTION_KEY",
        "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
    );
    let c = crate::db::connection::connect(":memory:").unwrap();
    crate::db::migrations::run(&c).unwrap();
    let (org, user, _) =
        crate::db::queries::bootstrap(&c, "Test", "test", "owner@test.com", "Owner").unwrap();
    let runner = workflows::register_runner(&c, &org.id, "Runner").unwrap();
    let rid = runner["id"].as_str().unwrap();
    let w = workflows::create_workspace(&c, &org.id, rid, "App", "/work/app").unwrap();
    let wid = w["id"].as_str().unwrap();
    let mut repo = Repository {
        path: "api".into(),
        remote: "git@github.com:acme/api.git".into(),
        head_sha: "a".repeat(40),
        branch: None,
        writable: false,
    };
    workflows::report_inventory(&c, &org.id, rid, wid, &[repo.clone()]).unwrap();
    repo.writable = true;
    workflows::confirm_inventory(&c, &org.id, wid, 1, &[repo]).unwrap();
    let secret = crate::crypto::encrypt("test-github-token").unwrap();
    c.execute("INSERT INTO autonomous_agent_connectors(id,org_id,kind,name,secret_ciphertext,metadata_json,scopes_json,health,created_by) VALUES ('github',?1,'target_secret','GitHub',?2,?3,'[\"github:write\"]','ready',?4)",params![org.id,secret,json!({"purpose":"factory_workflow_github","repositories":["acme/api"]}).to_string(),user.id]).unwrap();
    let mut definition:Definition=serde_json::from_value(json!({"name":"Delivery","workspace_id":wid,"steps":[
        {"id":"qa","name":"Verification","kind":"check","adapter":"qa"},
        {"id":"functional","name":"Functional QA","kind":"check","adapter":"isolated-functional-qa","options":{"required_checks":["functional"]}},
        {"id":"gate","name":"Gate","kind":"gate","adapter":"gate","depends_on":["qa","functional"]},
        {"id":"publish","name":"Publish","kind":"publish","adapter":"publisher","depends_on":["gate"],"options":{"connector_id":"github","github_required_checks":["ci"]}}
    ]})).unwrap();
    if backend {
        std::env::set_var("FACTORY_GITHUB_PUBLISH_ORG_ID", &org.id);
        definition
            .steps
            .iter_mut()
            .find(|s| s.id == "publish")
            .unwrap()
            .options = json!({"github_auth":"server_gh_cli","github_login":"test-bot","github_required_checks":["ci"]});
    }
    let d = workflows::create_definition(&c, &org.id, &user.id, &definition).unwrap();
    let run = workflows::start_pinned_run(
        &c,
        &org.id,
        d["id"].as_str().unwrap(),
        "test:publish",
        &json!({"title":"Fix"}),
    )
    .unwrap();
    let id = run["id"].as_str().unwrap();
    let content = b"fixed\n";
    let blob = hash("blob", content);
    let mut tree_data = b"100644 code.txt\0".to_vec();
    tree_data.extend(hex::decode(blob).unwrap());
    let tree = hash("tree", &tree_data);
    let message = format!("NexusMind {id} implement attempt 1\n");
    let raw=format!("tree {tree}\nparent {}\nauthor NexusMind <factory@nexusmind.local> 946684800 +0000\ncommitter NexusMind <factory@nexusmind.local> 946684800 +0000\n\n{message}","a".repeat(40));
    let head = hash("commit", raw.as_bytes());
    let heads = json!({"api":head});
    c.execute(
        "UPDATE factory_workflow_runs SET heads_json=?1,change_revision=1 WHERE id=?2",
        params![heads.to_string(), id],
    )
    .unwrap();
    for step in ["qa", "gate"] {
        c.execute("UPDATE factory_workflow_steps SET status='succeeded',result_json=?1 WHERE run_id=?2 AND step_id=?3",params![json!({"heads":heads,"evidence":[{"check":"tests","status":"pass","commits":heads},{"check":"build","status":"pass","commits":heads}]}).to_string(),id,step]).unwrap();
    }
    c.execute("UPDATE factory_workflow_steps SET status='succeeded',result_json=?1 WHERE run_id=?2 AND step_id='functional'",params![json!({"heads":heads,"qa_report_only":true,"evidence":[{"check":"functional","status":"fail","commits":heads,"artifact_digest":"d".repeat(64)}]}).to_string(),id]).unwrap();
    for action in ["publish", "open_pr"] {
        workflow_execution::approve(&c, &org.id, &user.id, id, action, 1).unwrap();
    }
    let job = workflows::claim(
        &c,
        &org.id,
        rid,
        &["adapter:publisher".into()],
        chrono::Utc::now().timestamp(),
    )
    .unwrap()
    .unwrap();
    let request = Request {
        lease_token: job["lease_token"].as_str().unwrap().into(),
        revision: 1,
        repositories: vec![Export {
            path: "api".into(),
            commits: vec![Commit {
                sha: head.clone(),
                tree_sha: tree,
                parent_sha: "a".repeat(40),
                message,
                files: vec![File {
                    path: "code.txt".into(),
                    mode: "100644".into(),
                    content_base64: Some(base64::engine::general_purpose::STANDARD.encode(content)),
                }],
            }],
        }],
    };
    let store = SqliteStore::new(c);
    let state = Arc::new(Mutex::new(Remote::default()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Router::new().fallback(github).with_state(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut client = Github::new("test-github-token".into()).unwrap();
    client.base = format!("http://{address}");
    if backend {
        state.lock().unwrap().login = Some("different-account".into());
        assert_eq!(
            publish_using(&store, &org.id, rid, id, "publish", &request, &client)
                .await
                .unwrap_err()
                .to_string(),
            "backend_github_identity_changed"
        );
        assert_eq!(state.lock().unwrap().prs, 0);
        state.lock().unwrap().login = None;
    }
    // Main advanced over a file this change also edits: re-verify on fresh sources.
    state.lock().unwrap().advanced = Some(("b".repeat(40), vec!["docs.md", "code.txt"]));
    assert_eq!(
        publish_using(&store, &org.id, rid, id, "publish", &request, &client)
            .await
            .unwrap_err()
            .to_string(),
        "base_moved_reverify"
    );
    assert_eq!(state.lock().unwrap().prs, 0);
    // Disjoint advance: the verified commits still publish.
    state.lock().unwrap().advanced = Some(("b".repeat(40), vec!["docs.md"]));
    let result = publish_using(&store, &org.id, rid, id, "publish", &request, &client)
        .await
        .unwrap();
    assert_eq!(result["pull_requests"][0]["head_sha"], head);
    assert_eq!(result["pull_requests"][0]["ci"]["status"], "passed");
    assert!(!result.to_string().contains("test-github-token"));
    {
        let mut remote = state.lock().unwrap();
        let body = remote.pull.as_ref().unwrap()["body"].as_str().unwrap();
        assert!(body.contains("QA findings (non-blocking)"));
        assert!(body.contains("`functional` / `functional`: **failed**"));
        let body = body.replace("QA findings (non-blocking)", "outdated QA notes") + "\nHuman reviewer note";
        remote.pull.as_mut().unwrap()["body"] = json!(body);
    }
    publish_using(&store, &org.id, rid, id, "publish", &request, &client)
        .await
        .unwrap();
    assert_eq!(state.lock().unwrap().prs, 1);
    assert_eq!(state.lock().unwrap().refs, 1);
    {
        let remote = state.lock().unwrap();
        assert_eq!(remote.body_updates, 1);
        let body = remote.pull.as_ref().unwrap()["body"].as_str().unwrap();
        assert!(body.contains("Human reviewer note"));
        assert!(!body.contains("outdated QA notes"));
        assert_eq!(body.matches("nexusmind:qa:start").count(), 1);
    }
    for conclusion in ["skipped", "neutral", "failure"] {
        state.lock().unwrap().conclusion = Some(conclusion.into());
        assert_eq!(
            publish_using(&store, &org.id, rid, id, "publish", &request, &client)
                .await
                .unwrap_err()
                .to_string(),
            "required_ci_failed"
        );
        let h = store.conn();
        let c = h.lock().unwrap();
        let status: String = c
            .query_row(
                "SELECT state FROM factory_workflow_publications WHERE run_id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "awaiting_ci");
    }
    assert_eq!(state.lock().unwrap().prs, 1);
    if backend {
        std::env::set_var("FACTORY_GITHUB_PUBLISH_ORG_ID", "another-org");
    } else {
        let h = store.conn();
        let c = h.lock().unwrap();
        c.execute(
            "UPDATE autonomous_agent_connectors SET health='revoked' WHERE id='github'",
            [],
        )
        .unwrap();
    }
    assert!(
        publish_using(&store, &org.id, rid, id, "publish", &request, &client)
            .await
            .is_err()
    );
    server.abort();
    if backend {
        std::env::remove_var("FACTORY_GITHUB_PUBLISH_ORG_ID");
    }
    std::env::remove_var("NEXUSMIND_TOKEN_ENCRYPTION_KEY");
}
