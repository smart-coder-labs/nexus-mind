use crate::{
    automation::agent_runtime::{plan, prepare_config, RuntimeConfig},
    db::{connection, migrations, queries, workflows},
    factory::workflows::{Definition, Repository},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn setup() -> (rusqlite::Connection, String, String) {
    let conn = connection::connect(":memory:").unwrap();
    migrations::run(&conn).unwrap();
    let (org, user, _) =
        queries::bootstrap(&conn, "Runtime", "runtime", "runtime@example.com", "Owner").unwrap();
    (conn, org.id, user.id)
}

#[test]
fn catalog_resources_require_scope_approval_hash_target_and_visibility() {
    let (conn, org, user) = setup();
    let harness = queries::create_harness(
        &conn,
        &org,
        &user,
        &serde_json::from_value(json!({"slug":"rust-review","name":"Rust review"})).unwrap(),
    )
    .unwrap();
    let content = "---\nname: rust-review\ndescription: Review Rust\n---\nReview ownership";
    let version = queries::publish_harness_version(&conn, &org, &user, &harness.id, &serde_json::from_value(json!({
        "version":"1.0.0", "manifest": {"schema_version":"1.1","format":"skill","targets":["pi"],
        "components":[{"kind":"file","path":"rust/SKILL.md","media_type":"text/markdown","size_bytes":content.len(),"sha256":format!("sha256:{}",hex::encode(Sha256::digest(content.as_bytes()))),"content":content}],
        "provenance":{"source":"test"},"security":{"requires_approval":true}}})).unwrap()).unwrap();
    let mut config = json!({"executor":"pi","runtime":{"nexusmind_profile":"only_context","harnesses":[{"id":harness.id,"version":"1.0.0","manifest_hash":version.manifest_hash,"components":["rust/SKILL.md"]}]},"runtime_resolved":{"components":[{"id":"forged"}]}});
    assert_eq!(
        prepare_config(&conn, &org, &user, &config)
            .unwrap_err()
            .to_string(),
        "approval_required"
    );
    queries::create_harness_approval(&conn, &org, &user, Some(&user), &harness.id, "1.0.0", &serde_json::from_value(json!({"target_tool":"pi","target_scope":"agent-runtime","manifest_hash":version.manifest_hash})).unwrap()).unwrap();
    let resolved = prepare_config(&conn, &org, &user, &config).unwrap();
    assert_eq!(
        resolved["runtime_resolved"]["components"][0]["files"]["SKILL.md"],
        content
    );
    assert!(!resolved["runtime_resolved"].to_string().contains("forged"));
    let runtime: RuntimeConfig =
        serde_json::from_value(resolved["runtime_resolved"].clone()).unwrap();
    let pi = plan(
        &runtime,
        "pi",
        "/runtime",
        json!({"command":"nexusmind-mcp","env":{}}),
    )
    .unwrap();
    assert!(pi.files["/runtime/mcp.json"].contains("only_context"));
    assert!(prepare_config(&conn, "another-org", &user, &config).is_err());
    config["runtime"]["harnesses"][0]["manifest_hash"] = json!("changed");
    assert!(prepare_config(&conn, &org, &user, &config).is_err());
    config["runtime"]["harnesses"][0]["manifest_hash"] = json!(version.manifest_hash);
    config["executor"] = json!("claude");
    assert_eq!(
        prepare_config(&conn, &org, &user, &config)
            .unwrap_err()
            .to_string(),
        "harness_executor_incompatible"
    );
}

#[test]
fn workflow_overrides_are_pinned_without_mutating_agent_revision() {
    let (conn, org, user) = setup();
    let request = serde_json::from_value(json!({"name":"Reviewer","template_key":"github_pr_reviewer","config":{"executor":"pi","isolation":"local","runtime":{"nexusmind_profile":"essential","components":[]}},"budgets":{}})).unwrap();
    let agent = queries::create_autonomous_agent_definition(&conn, &org, &user, &request).unwrap();
    let runner = workflows::register_runner(&conn, &org, "Local").unwrap();
    let workspace = workflows::create_workspace(
        &conn,
        &org,
        runner["id"].as_str().unwrap(),
        "App",
        "/work/app",
    )
    .unwrap();
    let wid = workspace["id"].as_str().unwrap();
    let mut repo = Repository {
        path: "api".into(),
        remote: "git@github.com:acme/api.git".into(),
        head_sha: "a".repeat(40),
        writable: false,
    };
    workflows::report_inventory(
        &conn,
        &org,
        runner["id"].as_str().unwrap(),
        wid,
        &[repo.clone()],
    )
    .unwrap();
    repo.writable = true;
    workflows::confirm_inventory(&conn, &org, wid, 1, &[repo]).unwrap();
    let definition: Definition = serde_json::from_value(json!({"name":"Review","workspace_id":wid,"steps":[{"id":"review","name":"Review","kind":"review","adapter":"pi-reviewer","options":{"agent_id":agent.definition.id,"runtime":{"nexusmind_profile":"only_context"},"runtime_snapshot":{"forged":true}}}]})).unwrap();
    let saved = workflows::create_definition(&conn, &org, &user, &definition).unwrap();
    let snapshot = &saved["definition"]["steps"][0]["options"]["runtime_snapshot"];
    assert_eq!(snapshot["agent_revision_id"], agent.revision.id);
    assert_eq!(
        snapshot["config"]["runtime_resolved"]["nexusmind_profile"],
        "only_context"
    );
    assert_eq!(snapshot["forged"], Value::Null);
    let unchanged = queries::get_autonomous_agent_detail(&conn, &org, &agent.definition.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        unchanged.revision.config["runtime"]["nexusmind_profile"],
        "essential"
    );
    let capabilities = saved["definition"]["steps"][0]["capabilities"]
        .as_array()
        .unwrap();
    assert!(capabilities.contains(&json!("agent-runtime:v1")));
    assert!(capabilities.contains(&json!("executor:pi")));
    let run = workflows::start_run(
        &conn,
        &org,
        saved["id"].as_str().unwrap(),
        "manual:runtime-test",
        &json!({"title":"Review"}),
    )
    .unwrap();
    assert_eq!(
        run["snapshot"]["definition"]["steps"][0]["options"]["runtime_snapshot"],
        *snapshot
    );
}
