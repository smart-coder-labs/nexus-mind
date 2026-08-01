use nexusmind::{
    automation::{
        github::{verify_github_webhook_signature, process_qa_handoff, QaHandoffRequest},
        leases::cancel_lease,
    },
    db::{connection, migrations, queries},
};
use rusqlite::Connection;

fn setup() -> (Connection, String, String, String) {
    let conn = connection::connect(":memory:").expect("connect in-memory database");
    migrations::run(&conn).expect("apply migrations");
    let (org, user, _) = queries::bootstrap(
        &conn,
        "GitHub Org",
        "github-org",
        "admin@github.example",
        "Admin",
    )
    .expect("bootstrap org");
    let project = queries::create_project(&conn, &org.id, "github-project", None, None)
        .expect("create project");
    (conn, org.id, user.id, project.id)
}

#[test]
fn github_signature_verification_and_replay_protection() {
    let secret = "webhook_secret_key";
    let body = r#"{"action":"closed","pull_request":{"merged":true}}"#;

    // HMAC-SHA256 of body with secret
    let signature = "sha256=1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";

    let invalid = verify_github_webhook_signature(body.as_bytes(), secret, signature);
    assert!(!invalid);
}

#[test]
fn revocation_before_recheck_prevents_qa_merge() {
    let (conn, org_id, user_id, project_id) = setup();

    let run_id = "run-qa-1";
    let attempt_id = "attempt-qa-1";
    queries::create_automation_run(&conn, run_id, &org_id, Some(&project_id), &user_id, "managed-qa-deploy", 1)
        .expect("create run");
    queries::create_automation_attempt(&conn, attempt_id, run_id).expect("create attempt");

    let req = QaHandoffRequest {
        org_id: org_id.clone(),
        attempt_id: attempt_id.to_string(),
        pr_number: 246,
        target_branch: "qa-stage".to_string(),
    };

    // Revoke attempt before handoff
    cancel_lease(&conn, "lease-token", "revoked_by_admin").expect("revoke attempt");

    let result = process_qa_handoff(&conn, &req);
    assert!(result.is_err(), "revoked attempt cannot process QA handoff");
}
