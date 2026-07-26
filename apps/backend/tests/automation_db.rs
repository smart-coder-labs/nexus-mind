use nexusmind::db::{connection, migrations, queries};
use rusqlite::Connection;

fn setup() -> Connection {
    let conn = connection::connect(":memory:").expect("connect in-memory database");
    migrations::run(&conn).expect("apply migrations to the fixture");
    conn
}

fn bootstrap(conn: &Connection) -> (String, String, String) {
    let (org, user, _) = queries::bootstrap(
        conn,
        "Automation Org",
        "automation-org",
        "admin@automation.example",
        "Admin",
    )
    .expect("bootstrap automation organization");
    let project = queries::create_project(conn, &org.id, "automation-project", None, None)
        .expect("create automation project");
    (org.id, user.id, project.id)
}

#[test]
fn v57_rejects_runs_bound_to_another_organization_project() {
    let conn = setup();
    let (org_id, user_id, _project_id) = bootstrap(&conn);
    let other_project = queries::create_project(
        &conn,
        &queries::create_org(
            &conn,
            "Other Org",
            "other-org",
            "admin@other.example",
            "Other Admin",
        )
        .expect("create other organization")
        .0
        .id,
        "other-project",
        None,
        None,
    )
    .expect("create other project");

    let result = queries::create_automation_run(
        &conn,
        "run-cross-org",
        &org_id,
        Some(&other_project.id),
        &user_id,
        "profile-version-1",
        7,
    );

    assert!(result.is_err(), "a run cannot bind a project from another organization");
}

#[test]
fn receipts_are_immutable_and_callbacks_are_idempotent() {
    let conn = setup();
    let (org_id, user_id, project_id) = bootstrap(&conn);
    queries::create_automation_run(
        &conn,
        "run-1",
        &org_id,
        Some(&project_id),
        &user_id,
        "profile-version-1",
        7,
    )
    .expect("create automation run");
    queries::create_automation_attempt(&conn, "attempt-1", "run-1")
        .expect("create automation attempt");

    assert!(queries::record_automation_callback(
        &conn,
        &org_id,
        "attempt-1",
        "callback-1",
        "payload-sha-256",
    )
    .expect("record first callback"));
    assert!(!queries::record_automation_callback(
        &conn,
        &org_id,
        "attempt-1",
        "callback-1",
        "payload-sha-256",
    )
    .expect("replay the same callback"));

    let conflicting_replay = queries::record_automation_callback(
        &conn,
        &org_id,
        "attempt-1",
        "callback-1",
        "different-payload-sha-256",
    );
    assert!(
        conflicting_replay.is_err(),
        "a replay with the same callback id but different evidence must be rejected"
    );

    let receipt_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM automation_receipts", [], |row| row.get(0))
        .expect("count receipts");
    assert_eq!(receipt_count, 1, "a callback replay must not create a second receipt");

    let rewrite = conn.execute(
        "UPDATE automation_receipts SET payload_hash = 'rewritten' WHERE callback_id = 'callback-1'",
        [],
    );
    assert!(rewrite.is_err(), "automation receipts must be append-only");
}

#[test]
fn revoked_attempt_denies_later_callbacks_without_removing_receipts() {
    let conn = setup();
    let (org_id, user_id, project_id) = bootstrap(&conn);
    queries::create_automation_run(
        &conn,
        "run-revoked",
        &org_id,
        Some(&project_id),
        &user_id,
        "profile-version-1",
        7,
    )
    .expect("create automation run");
    queries::create_automation_attempt(&conn, "attempt-revoked", "run-revoked")
        .expect("create automation attempt");
    queries::record_automation_callback(
        &conn,
        &org_id,
        "attempt-revoked",
        "callback-before-revoke",
        "before-revoke-sha-256",
    )
    .expect("record callback before revocation");

    assert!(queries::revoke_automation_attempt(
        &conn,
        &org_id,
        "attempt-revoked",
        "profile_revoked",
    )
    .expect("revoke active attempt"));

    let denied = queries::record_automation_callback(
        &conn,
        &org_id,
        "attempt-revoked",
        "callback-after-revoke",
        "after-revoke-sha-256",
    );
    assert!(denied.is_err(), "revocation must deny later callbacks");

    let retained: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM automation_receipts WHERE callback_id = 'callback-before-revoke'",
            [],
            |row| row.get(0),
        )
        .expect("count retained receipt");
    assert_eq!(retained, 1, "revocation must retain prior receipts");
}
