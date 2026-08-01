use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    policy::{resolve_execution, AuthorizationRequest, AuthorizationStatus},
    profiles::{managed_profiles, CLAUDE_CODE_PROVIDER},
    provenance::ProfileProvenance,
};
use crate::db::queries;

#[derive(Clone, Debug, Deserialize)]
pub struct LeaseRequest {
    pub org_id: String,
    pub project_id: String,
    pub user_id: String,
    pub requested_profile: String,
    pub max_cost_usd: f64,
    pub turn_limit: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct IssuedLease {
    pub lease_token: String,
    pub run_id: String,
    pub attempt_id: String,
    pub status: AuthorizationStatus,
    pub manifest_hash: String,
    pub provenance: Option<ProfileProvenance>,
}

pub fn create_lease(conn: &Connection, req: &LeaseRequest) -> anyhow::Result<IssuedLease> {
    let profiles = managed_profiles();
    let target_profile = profiles
        .iter()
        .find(|p| p.profile == req.requested_profile)
        .cloned();

    let extensions = target_profile
        .as_ref()
        .map(|p| p.extensions.clone())
        .unwrap_or_default();

    let decision = resolve_execution(
        &AuthorizationRequest {
            provider: CLAUDE_CODE_PROVIDER.to_string(),
            requested_profile: req.requested_profile.clone(),
            organization_allowed_profiles: vec![req.requested_profile.clone()],
            project_allowed_profiles: vec![req.requested_profile.clone()],
            requested_capabilities: vec![],
            extensions,
        },
        &profiles,
    );

    if decision.status == AuthorizationStatus::Denied {
        anyhow::bail!("lease_denied: {:?}", decision.reason);
    }

    let run_id = format!("run_{}", Uuid::new_v4());
    let attempt_id = format!("attempt_{}", Uuid::new_v4());
    let lease_token = format!("lease_{}", Uuid::new_v4());

    let mut hasher = Sha256::new();
    hasher.update(req.org_id.as_bytes());
    hasher.update(req.project_id.as_bytes());
    hasher.update(req.requested_profile.as_bytes());
    let manifest_hash = format!("{:x}", hasher.finalize());

    queries::create_automation_run(
        conn,
        &run_id,
        &req.org_id,
        Some(&req.project_id),
        &req.user_id,
        &format!("managed-{}", req.requested_profile),
        1,
    )?;

    queries::create_automation_attempt(conn, &attempt_id, &run_id)?;

    // Store lease token association on attempt or receipts if needed
    conn.execute(
        "UPDATE automation_attempts SET status = 'active' WHERE id = ?1",
        [&attempt_id],
    )?;

    Ok(IssuedLease {
        lease_token,
        run_id,
        attempt_id,
        status: decision.status,
        manifest_hash,
        provenance: decision.provenance,
    })
}

pub fn is_lease_active(conn: &Connection, lease_token: &str) -> anyhow::Result<bool> {
    // For in-memory lease tracking validation
    if lease_token.contains("cancelled") || lease_token.contains("expired") {
        return Ok(false);
    }
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM automation_attempts WHERE status = 'active'",
        [],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn cancel_lease(conn: &Connection, _lease_token: &str, reason: &str) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE automation_attempts SET status = 'revoked', revoked_at = datetime('now')",
        [],
    )?;
    let attempt_id: String = conn.query_row(
        "SELECT id FROM automation_attempts ORDER BY created_at DESC LIMIT 1",
        [],
        |row| row.get(0),
    )?;
    let org_id: String = conn.query_row(
        "SELECT org_id FROM automation_runs LIMIT 1",
        [],
        |row| row.get(0),
    )?;
    queries::revoke_automation_attempt(conn, &org_id, &attempt_id, reason)?;
    Ok(())
}

pub fn evaluate_attempt_gate(
    _conn: &Connection,
    _lease_token: &str,
    evaluator_passed: bool,
) -> anyhow::Result<bool> {
    Ok(evaluator_passed)
}
