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
#[derive(Default)]
struct Remote {
    branch: Option<String>,
    pull: Option<Value>,
    prs: u32,
    refs: u32,
    conclusion: Option<String>,
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
    let value = if path == "/repos/acme/api" {
        json!({"default_branch":"main"})
    } else if path.ends_with("/branches/main") {
        json!({"commit":{"sha":"a".repeat(40)}})
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
        let pull = json!({"number":42,"html_url":"https://github.com/acme/api/pull/42","head":{"sha":remote.branch}});
        remote.pull = Some(pull.clone());
        pull
    } else {
        panic!("Unexpected upstream operation {method} {uri}")
    };
    Json(value)
}
#[tokio::test]
async fn publishes_exact_git_objects_once_and_rejects_revocation() {
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
        writable: false,
    };
    workflows::report_inventory(&c, &org.id, rid, wid, &[repo.clone()]).unwrap();
    repo.writable = true;
    workflows::confirm_inventory(&c, &org.id, wid, 1, &[repo]).unwrap();
    let secret = crate::crypto::encrypt("test-github-token").unwrap();
    c.execute("INSERT INTO autonomous_agent_connectors(id,org_id,kind,name,secret_ciphertext,metadata_json,scopes_json,health,created_by) VALUES ('github',?1,'target_secret','GitHub',?2,?3,'[\"github:write\"]','ready',?4)",params![org.id,secret,json!({"purpose":"factory_workflow_github","repositories":["acme/api"]}).to_string(),user.id]).unwrap();
    let definition:Definition=serde_json::from_value(json!({"name":"Delivery","workspace_id":wid,"steps":[
        {"id":"qa","name":"QA","kind":"check","adapter":"qa"},
        {"id":"gate","name":"Gate","kind":"gate","adapter":"gate","depends_on":["qa"]},
        {"id":"publish","name":"Publish","kind":"publish","adapter":"publisher","depends_on":["gate"],"options":{"connector_id":"github","github_required_checks":["ci"]}}
    ]})).unwrap();
    let d = workflows::create_definition(&c, &org.id, &user.id, &definition).unwrap();
    let run = workflows::start_run(
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
    let result = publish_using(&store, &org.id, rid, id, "publish", &request, &client)
        .await
        .unwrap();
    assert_eq!(result["pull_requests"][0]["head_sha"], head);
    assert_eq!(result["pull_requests"][0]["ci"]["status"], "passed");
    assert!(!result.to_string().contains("test-github-token"));
    publish_using(&store, &org.id, rid, id, "publish", &request, &client)
        .await
        .unwrap();
    assert_eq!(state.lock().unwrap().prs, 1);
    assert_eq!(state.lock().unwrap().refs, 1);
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
    {
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
    std::env::remove_var("NEXUSMIND_TOKEN_ENCRYPTION_KEY");
}
