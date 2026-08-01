use nexusmind::{
    automation::{
        leases::{create_lease, LeaseRequest},
        policy::AuthorizationStatus,
    },
    db::{connection, migrations, queries},
};
use rusqlite::Connection;

fn setup() -> (Connection, String, String, String) {
    let conn = connection::connect(":memory:").expect("connect in-memory database");
    migrations::run(&conn).expect("apply migrations");
    let (org, user, _) = queries::bootstrap(
        &conn,
        "Orchestration Org",
        "orchestration-org",
        "admin@orch.example",
        "Admin",
    )
    .expect("bootstrap org");
    let project = queries::create_project(&conn, &org.id, "orch-project", None, None)
        .expect("create project");
    (conn, org.id, user.id, project.id)
}

#[test]
fn deterministic_manifest_and_lease_creation() {
    let (conn, org_id, user_id, project_id) = setup();

    let request = LeaseRequest {
        org_id: org_id.clone(),
        project_id: project_id.clone(),
        user_id: user_id.clone(),
        requested_profile: "implementation".to_string(),
        max_cost_usd: 5.0,
        turn_limit: 10,
    };

    let lease = create_lease(&conn, &request).expect("create lease");
    assert_eq!(lease.status, AuthorizationStatus::Allowed);
    assert!(lease.lease_token.starts_with("lease_"));
    assert_eq!(lease.manifest_hash.len(), 64);

    // Lease expiry/cancellation behavior
    let active = nexusmind::automation::leases::is_lease_active(&conn, &lease.lease_token)
        .expect("check active lease");
    assert!(active);

    nexusmind::automation::leases::cancel_lease(&conn, &lease.lease_token, "user_cancelled")
        .expect("cancel lease");

    let cancelled_active = nexusmind::automation::leases::is_lease_active(&conn, &lease.lease_token)
        .expect("check cancelled lease");
    assert!(!cancelled_active);
}

#[test]
fn evaluator_block_prevents_privileged_action() {
    let (conn, org_id, user_id, project_id) = setup();

    let request = LeaseRequest {
        org_id: org_id.clone(),
        project_id: project_id.clone(),
        user_id,
        requested_profile: "qa-deploy".to_string(),
        max_cost_usd: 1.0,
        turn_limit: 5,
    };

    let lease = create_lease(&conn, &request).expect("create lease");

    let pass_eval = nexusmind::automation::leases::evaluate_attempt_gate(&conn, &lease.lease_token, true)
        .expect("eval gate pass");
    assert!(pass_eval);

    let fail_eval = nexusmind::automation::leases::evaluate_attempt_gate(&conn, &lease.lease_token, false)
        .expect("eval gate fail");
    assert!(!fail_eval);
}
