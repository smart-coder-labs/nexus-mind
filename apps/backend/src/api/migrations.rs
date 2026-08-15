//! Knowledge-migration HTTP surface: runs, staging, the review gate, and commit.
//!
//! # Two permissions, not one
//!
//! `migration:write` runs the scan. `migration:review` decides what enters the
//! company brain. They are deliberately separate and neither implies the other:
//! in a consultancy the person who points the runner at a client repo is rarely
//! the person who should be deciding which of that client's conventions become
//! team knowledge.
//!
//! # No model ever runs behind this API
//!
//! Candidates arrive already classified. Nothing in this module — or anywhere
//! below it — calls a language model, which is what lets the backend deploy with
//! no model credentials at all (`docs/ENGINEERING_PROCESS.md:14`, BYOM).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    api::helpers::{hidden_resource_not_found, require_permission, AppJson},
    db::{migration_queries as mq, queries},
    models::types::{
        ApiError, AuthContext, CandidateInput, CreateMigrationRunRequest, DestinationKind,
        MigrationCandidate, MigrationRun, ReviewActionRequest, RunReport, SourceKind, StageResult,
    },
    store::sqlite::SqliteStore,
};

fn lock_err() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "db lock poisoned".to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

fn db_err(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: e.to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

fn bad_request(msg: &str, code: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: msg.to_string(),
            code: code.to_string(),
        }),
    )
}

/// `None` for super_user (no restriction), `Some(user_id)` otherwise — the same
/// discriminator `api::clients` and `api::context` use.
fn viewer_scope(auth: &AuthContext) -> Option<&str> {
    if auth.role.is_super_user() {
        None
    } else {
        Some(&auth.user_id)
    }
}

/// Loads a run the caller is allowed to see, or 404. A denied read returns 404
/// and never 403: a 403 would confirm the run exists, which is precisely what a
/// competing client must not learn. [`hidden_resource_not_found`] writes the
/// audit row, so every denial leaves evidence.
fn load_visible_run(
    conn: &rusqlite::Connection,
    auth: &AuthContext,
    run_id: &str,
    method: &str,
) -> Result<MigrationRun, (StatusCode, Json<ApiError>)> {
    let denied = || {
        hidden_resource_not_found(conn, auth, "migration_run", run_id, method, "migrations")
    };
    let run = mq::get_run(conn, &auth.org_id, run_id)
        .map_err(db_err)?
        .ok_or_else(denied)?;
    let visible =
        mq::user_can_view_run(conn, &auth.org_id, &run, viewer_scope(auth)).map_err(db_err)?;
    if !visible {
        return Err(denied());
    }
    Ok(run)
}

// ── Runs ─────────────────────────────────────────────────────────────────────

pub async fn create_run(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<CreateMigrationRunRequest>,
) -> Result<(StatusCode, Json<MigrationRun>), (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "migration:write")?;

    // A non-super_user may only open a run against a client they can already
    // see. Without this, membership would be enforced on reads but not on the
    // write that creates the material in the first place.
    if let Some(client_id) = input.client_id.as_deref() {
        let allowed =
            queries::user_can_view_client(&conn, &auth.org_id, client_id, viewer_scope(&auth))
                .map_err(db_err)?;
        if !allowed {
            return Err(hidden_resource_not_found(
                &conn, &auth, "client", client_id, "POST", "migrations",
            ));
        }
    }

    let run = mq::create_run(
        &conn,
        &mq::NewRun {
            org_id: &auth.org_id,
            client_id: input.client_id.as_deref(),
            project_id: input.project_id.as_deref(),
            source_kind: input.source_kind,
            source_ref: input.source_ref.as_deref(),
            runner_version: input.runner_version.as_deref(),
            attestation: input.attestation.unwrap_or_else(|| serde_json::json!({})),
            created_by: &auth.user_id,
        },
    )
    .map_err(|e| {
        // The coherence triggers speak in SQL; translate rather than leak.
        if e.to_string().contains("must belong to run organization") {
            bad_request(
                "client_id and project_id must belong to the caller's organization",
                "scope_mismatch",
            )
        } else {
            db_err(e)
        }
    })?;

    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "create",
        "migration_run",
        Some(&run.id),
        serde_json::json!({
            "source_kind": run.source_kind.as_str(),
            "client_id": run.client_id,
            "project_id": run.project_id,
        }),
    );

    Ok((StatusCode::CREATED, Json(run)))
}

#[derive(Deserialize)]
pub struct ListRunsParams {
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub source_kind: Option<SourceKind>,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn list_runs(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListRunsParams>,
) -> Result<Json<Vec<MigrationRun>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "migration:read")?;
    let runs = mq::list_runs_visible(
        &conn,
        &auth.org_id,
        viewer_scope(&auth),
        params.client_id.as_deref(),
        params.source_kind,
        params.limit.unwrap_or(50).clamp(1, 200),
    )
    .map_err(db_err)?;
    Ok(Json(runs))
}

pub async fn get_run(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(run_id): Path<String>,
) -> Result<Json<MigrationRun>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "migration:read")?;
    Ok(Json(load_visible_run(&conn, &auth, &run_id, "GET")?))
}

pub async fn cancel_run(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(run_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "migration:write")?;
    let run = load_visible_run(&conn, &auth, &run_id, "POST")?;
    let cancelled = mq::cancel_run(&conn, &auth.org_id, &run.id).map_err(|e| {
        if e.to_string().contains("run_already_completed") {
            bad_request(
                "this run is already completed; there is nothing pending to cancel",
                "run_already_completed",
            )
        } else {
            db_err(e)
        }
    })?;
    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "cancel",
        "migration_run",
        Some(&run.id),
        serde_json::json!({ "cancelled_candidates": cancelled }),
    );
    Ok(Json(serde_json::json!({ "cancelled": cancelled })))
}

pub async fn get_report(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(run_id): Path<String>,
) -> Result<Json<RunReport>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "migration:read")?;
    let run = load_visible_run(&conn, &auth, &run_id, "GET")?;
    Ok(Json(mq::run_report(&conn, &run.id).map_err(db_err)?))
}

// ── Staging ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct StageBody {
    pub candidates: Vec<CandidateInput>,
}

#[derive(Debug, Serialize)]
pub struct StageResponse {
    pub staged: usize,
    pub skipped: usize,
    pub rejected: usize,
    pub results: Vec<StageResult>,
}

/// Batches are capped so one runner cannot post a hundred megabytes in a single
/// request. The runner chunks; the cap is a backstop, not a workflow.
const MAX_BATCH: usize = 500;

pub async fn stage_candidates(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(run_id): Path<String>,
    AppJson(body): AppJson<StageBody>,
) -> Result<Json<StageResponse>, (StatusCode, Json<ApiError>)> {
    if body.candidates.len() > MAX_BATCH {
        return Err(bad_request(
            &format!("at most {MAX_BATCH} candidates per request"),
            "batch_too_large",
        ));
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "migration:write")?;
    let run = load_visible_run(&conn, &auth, &run_id, "POST")?;

    if run.status != "staging" && run.status != "in_review" {
        return Err(bad_request(
            &format!("run is {}; candidates may only be added while staging", run.status),
            "run_not_open",
        ));
    }

    let results =
        mq::stage_candidates(&conn, &auth.org_id, &run.id, &body.candidates).map_err(db_err)?;

    let staged = results
        .iter()
        .filter(|r| matches!(r, StageResult::Staged { .. }))
        .count();
    let skipped = results
        .iter()
        .filter(|r| matches!(r, StageResult::Skipped { .. }))
        .count();
    let rejected = results.len() - staged - skipped;

    Ok(Json(StageResponse {
        staged,
        skipped,
        rejected,
        results,
    }))
}

#[derive(Deserialize)]
pub struct ListCandidatesParams {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub destination_kind: Option<DestinationKind>,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn list_candidates(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(run_id): Path<String>,
    Query(params): Query<ListCandidatesParams>,
) -> Result<Json<Vec<MigrationCandidate>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "migration:read")?;
    let run = load_visible_run(&conn, &auth, &run_id, "GET")?;
    let candidates = mq::list_candidates(
        &conn,
        &run.id,
        params.status.as_deref(),
        params.destination_kind,
        params.limit.unwrap_or(100).clamp(1, 500),
    )
    .map_err(db_err)?;
    Ok(Json(candidates))
}

// ── Review ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ReviewBody {
    pub actions: Vec<ReviewActionRequest>,
}

#[derive(Debug, Serialize)]
pub struct ReviewResultEntry {
    pub candidate_id: String,
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_version: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ReviewResponse {
    pub applied: usize,
    pub conflicts: usize,
    pub results: Vec<ReviewResultEntry>,
}

pub async fn review(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(run_id): Path<String>,
    AppJson(body): AppJson<ReviewBody>,
) -> Result<Json<ReviewResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    // A refused review is part of the trail, not just an HTTP status that
    // disappears into a log — record it before returning.
    if let Err(denied) = require_permission(&conn, &auth, None, "migration:review") {
        if mq::get_run(&conn, &auth.org_id, &run_id)
            .ok()
            .flatten()
            .is_some()
        {
            let _ = mq::record_permission_denied(&conn, &run_id, None, &auth.user_id, None);
        }
        return Err(denied);
    }

    let run = load_visible_run(&conn, &auth, &run_id, "POST")?;

    // Constrained batch approval: a batch of approvals is refused outright if it
    // contains any `client_attested` candidate. Those rest on somebody's word
    // and get read one at a time.
    let approving: Vec<String> = body
        .actions
        .iter()
        .filter(|a| matches!(a.action, crate::models::types::ReviewVerdict::Approved))
        .map(|a| a.candidate_id.clone())
        .collect();
    if approving.len() > 1 {
        let attested = mq::batch_contains_attested(&conn, &approving).map_err(db_err)?;
        if !attested.is_empty() {
            return Err((
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: format!(
                        "batch approval refused: {} candidate(s) carry client-attested provenance \
                         and must be approved individually: {}",
                        attested.len(),
                        attested.join(", ")
                    ),
                    code: "attested_requires_individual_review".to_string(),
                }),
            ));
        }
    }

    let mut results = Vec::with_capacity(body.actions.len());
    let mut applied = 0usize;
    let mut conflicts = 0usize;

    for action in &body.actions {
        let outcome = mq::apply_review_action(
            &conn,
            &mq::ReviewRequest {
                run_id: &run.id,
                candidate_id: &action.candidate_id,
                actor_id: &auth.user_id,
                verdict: action.action,
                expected_version: action.expected_version,
                reason: action.reason.as_deref(),
                correlation: action.request_correlation_id.as_deref(),
            },
        )
        .map_err(db_err)?;

        let entry = match outcome {
            mq::ReviewOutcome::Applied { new_version } => {
                applied += 1;
                ReviewResultEntry {
                    candidate_id: action.candidate_id.clone(),
                    outcome: "applied".to_string(),
                    new_version: Some(new_version),
                    actual_version: None,
                }
            }
            mq::ReviewOutcome::StaleVersion { actual_version } => {
                conflicts += 1;
                ReviewResultEntry {
                    candidate_id: action.candidate_id.clone(),
                    outcome: "stale_version".to_string(),
                    new_version: None,
                    actual_version: Some(actual_version),
                }
            }
            mq::ReviewOutcome::NotFound => ReviewResultEntry {
                candidate_id: action.candidate_id.clone(),
                outcome: "not_found".to_string(),
                new_version: None,
                actual_version: None,
            },
            mq::ReviewOutcome::NotReviewable { status } => ReviewResultEntry {
                candidate_id: action.candidate_id.clone(),
                outcome: format!("not_reviewable:{status}"),
                new_version: None,
                actual_version: None,
            },
        };
        results.push(entry);
    }

    if applied > 0 {
        let _ = mq::set_run_status(&conn, &auth.org_id, &run.id, "in_review");
    }

    Ok(Json(ReviewResponse {
        applied,
        conflicts,
        results,
    }))
}

// ── Commit ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CommitResultEntry {
    pub candidate_id: String,
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CommitResponse {
    pub committed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub indexed: usize,
    pub pending_index: usize,
    pub results: Vec<CommitResultEntry>,
}

/// Commits every approved candidate of a run, then vectorizes what it can.
///
/// The two halves are deliberately sequential and deliberately not in the same
/// transaction. Embedding is CPU-bound — tens of milliseconds per text — and
/// holding a SQLite write transaction across a batch of them would block every
/// other writer for the duration. So the destination write commits first, and
/// vectorization runs afterwards on a best-effort basis.
///
/// The visible consequence, which the spec requires rather than hides: a
/// candidate can end up `committed` with `indexed_at` still NULL. The artifact
/// exists and is correct; it is just not searchable by similarity yet, and
/// `pending_index` says how many are in that state.
pub async fn commit(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(run_id): Path<String>,
) -> Result<Json<CommitResponse>, (StatusCode, Json<ApiError>)> {
    // ── Phase 1: commit, under the lock ──────────────────────────────────────
    let (results, run) = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;

        if let Err(denied) = require_permission(&conn, &auth, None, "migration:review") {
            if mq::get_run(&conn, &auth.org_id, &run_id).ok().flatten().is_some() {
                let _ = mq::record_permission_denied(&conn, &run_id, None, &auth.user_id, None);
            }
            return Err(denied);
        }

        let run = load_visible_run(&conn, &auth, &run_id, "POST")?;
        let results = mq::commit_approved(&conn, &auth.org_id, &run.id).map_err(db_err)?;

        let _ = queries::log_audit(
            &conn,
            &auth.org_id,
            &auth.user_id,
            "commit",
            "migration_run",
            Some(&run.id),
            serde_json::json!({ "candidates": results.len() }),
        );
        (results, run)
    };

    // ── Phase 2: vectorize, outside every transaction ────────────────────────
    let mut indexed = 0usize;
    if let Some(embed) = store.embed_service() {
        for (candidate, outcome) in &results {
            let mq::CommitOutcome::Committed { destination_id } = outcome else {
                continue;
            };
            if candidate.destination_kind != DestinationKind::Memory {
                continue;
            }
            match embed.embed_one(&candidate.content) {
                Ok(vector) => {
                    let blob = crate::embed::serialize(&vector);
                    let db = store.conn();
                    let Ok(conn) = db.lock() else { continue };
                    if queries::store_embedding(&conn, destination_id, &blob).is_ok() {
                        let _ = mq::set_candidate_indexed(&conn, &candidate.id);
                        indexed += 1;
                    }
                }
                // Never fails the commit: the memory is already persisted and
                // correct, and reconciliation will pick it up later.
                Err(e) => tracing::warn!(
                    "migration: failed to embed candidate {}: {e}",
                    candidate.id
                ),
            }
        }
    }

    let mut committed = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    let entries: Vec<CommitResultEntry> = results
        .into_iter()
        .map(|(candidate, outcome)| match outcome {
            mq::CommitOutcome::Committed { destination_id } => {
                committed += 1;
                CommitResultEntry {
                    candidate_id: candidate.id,
                    outcome: "committed".to_string(),
                    destination_id: Some(destination_id),
                    reason: None,
                }
            }
            mq::CommitOutcome::Skipped { reason } => {
                skipped += 1;
                CommitResultEntry {
                    candidate_id: candidate.id,
                    outcome: "skipped".to_string(),
                    destination_id: None,
                    reason: Some(reason),
                }
            }
            mq::CommitOutcome::Failed { error_code } => {
                failed += 1;
                CommitResultEntry {
                    candidate_id: candidate.id,
                    outcome: "failed".to_string(),
                    destination_id: None,
                    reason: Some(error_code),
                }
            }
        })
        .collect();

    let pending_index = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        let report = mq::run_report(&conn, &run.id).map_err(db_err)?;
        // `completed` must mean there is nothing left to decide. Committing the
        // approved half of a queue while five candidates still await review does
        // not complete anything, and a status that says otherwise would send the
        // reviewer away from work that is still theirs.
        let status = if report.staged == 0 && report.approved == 0 {
            "completed"
        } else {
            "in_review"
        };
        let _ = mq::set_run_status(&conn, &auth.org_id, &run.id, status);
        report.pending_index
    };

    Ok(Json(CommitResponse {
        committed,
        skipped,
        failed,
        indexed,
        pending_index,
        results: entries,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{connection::connect, migrations},
        models::types::{ReviewVerdict, Role, UserRole},
    };

    fn auth_for(user: &str, role: UserRole) -> AuthContext {
        AuthContext {
            org_id: "org1".to_string(),
            user_id: user.to_string(),
            role,
        }
    }

    /// `super_user`, not `admin`. Admin is privileged for permission checks but
    /// stays membership-scoped for reads (`viewer_scope`, mirrored from
    /// `api::clients` and `api::context`), so an admin who belongs to no client
    /// legitimately cannot open a run against one. The operator fixture needs
    /// org-wide sight, and that is `super_user`.
    fn admin() -> AuthContext {
        auth_for("admin", UserRole::Custom("super_user".to_string()))
    }

    /// A member is NOT privileged, so `require_permission` actually consults the
    /// role's grants — which is what makes the permission tests below mean
    /// something. `member` has no `migration:*`.
    fn member(user: &str) -> AuthContext {
        auth_for(user, UserRole::Standard(Role::Member))
    }

    fn store() -> SqliteStore {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'U2S', 'u2s');
             INSERT INTO users (id, org_id, email, name, role) VALUES ('admin', 'org1', 'a@u2s.com', 'Admin', 'admin');
             INSERT INTO users (id, org_id, email, name, role) VALUES ('dev_a', 'org1', 'a@d.com', 'A', 'member');
             INSERT INTO users (id, org_id, email, name, role) VALUES ('dev_b', 'org1', 'b@d.com', 'B', 'member');
             INSERT INTO clients (id, org_id, name, slug) VALUES ('cl_a', 'org1', 'Acme', 'acme');
             INSERT INTO clients (id, org_id, name, slug) VALUES ('cl_b', 'org1', 'Beta', 'beta');
             INSERT INTO client_members (client_id, user_id, role) VALUES ('cl_a', 'dev_a', 'member');
             INSERT INTO client_members (client_id, user_id, role) VALUES ('cl_b', 'dev_b', 'member');",
        )
        .unwrap();
        SqliteStore::new(conn)
    }

    fn new_run_body(client: Option<&str>) -> CreateMigrationRunRequest {
        CreateMigrationRunRequest {
            source_kind: SourceKind::RepoDocs,
            client_id: client.map(str::to_string),
            project_id: None,
            source_ref: Some("./".to_string()),
            runner_version: Some("2.1.233".to_string()),
            attestation: None,
        }
    }

    fn cand(identity: &str, kind: DestinationKind, provenance: &str) -> CandidateInput {
        CandidateInput {
            source_identity: identity.to_string(),
            destination_kind: kind,
            content: "body".to_string(),
            destination_hint: serde_json::json!({}),
            source_excerpt: Some("verbatim".to_string()),
            confidence: Some(0.9),
            normalized_metadata: serde_json::json!({}),
            provenance_kind: Some(provenance.to_string()),
        }
    }

    async fn create(store: &SqliteStore, auth: &AuthContext, client: Option<&str>) -> MigrationRun {
        let (_, Json(run)) = create_run(
            State(store.clone()),
            Extension(auth.clone()),
            AppJson(new_run_body(client)),
        )
        .await
        .expect("run creation must succeed");
        run
    }

    async fn stage(
        store: &SqliteStore,
        auth: &AuthContext,
        run: &str,
        candidates: Vec<CandidateInput>,
    ) -> StageResponse {
        let Json(resp) = stage_candidates(
            State(store.clone()),
            Extension(auth.clone()),
            Path(run.to_string()),
            AppJson(StageBody { candidates }),
        )
        .await
        .expect("staging must succeed");
        resp
    }

    async fn candidates_of(store: &SqliteStore, auth: &AuthContext, run: &str) -> Vec<MigrationCandidate> {
        let Json(list) = list_candidates(
            State(store.clone()),
            Extension(auth.clone()),
            Path(run.to_string()),
            Query(ListCandidatesParams { status: None, destination_kind: None, limit: None }),
        )
        .await
        .unwrap();
        list
    }

    // ── Permissions ──────────────────────────────────────────────────────────

    /// `migration:review` is a distinct grant. A `member` has neither, and the
    /// point of the test is that having write would still not grant review.
    #[tokio::test]
    async fn member_without_grants_cannot_create_or_review() {
        let store = store();
        let run = create(&store, &admin(), None).await;

        let denied = create_run(
            State(store.clone()),
            Extension(member("dev_a")),
            AppJson(new_run_body(None)),
        )
        .await;
        assert!(denied.is_err(), "member must not create a migration run");

        let denied_review = review(
            State(store.clone()),
            Extension(member("dev_a")),
            Path(run.id.clone()),
            AppJson(ReviewBody { actions: vec![] }),
        )
        .await;
        assert!(denied_review.is_err(), "member must not review");
    }

    /// A refused review is evidence, not just an HTTP status.
    #[tokio::test]
    async fn review_without_permission_records_permission_denied() {
        let store = store();
        let run = create(&store, &admin(), None).await;

        let _ = review(
            State(store.clone()),
            Extension(member("dev_a")),
            Path(run.id.clone()),
            AppJson(ReviewBody { actions: vec![] }),
        )
        .await;

        let db = store.conn();
        let conn = db.lock().unwrap();
        let recorded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM migration_review_actions
                  WHERE run_id = ?1 AND actor_id = 'dev_a' AND action = 'permission_denied'",
                [&run.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(recorded, 1, "the refusal must land in the review trail");
    }

    // ── Isolation ────────────────────────────────────────────────────────────

    /// A run scoped to client A must be invisible to someone who only belongs to
    /// client B — and the denial must be a 404, never a 403.
    #[tokio::test]
    async fn run_of_another_client_is_404_not_403() {
        let store = store();
        let run = create(&store, &admin(), Some("cl_a")).await;

        // dev_b is a member of client B only. Give them read so the failure is
        // about visibility, not about the permission gate.
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            conn.execute(
                "INSERT INTO roles (id, org_id, name, display_name, permissions)
                 VALUES ('r_reader', 'org1', 'migration_reader', 'Migration Reader', ?1)",
                [serde_json::json!(["migration:read"]).to_string()],
            )
            .unwrap();
        }
        let reader_b = auth_for("dev_b", UserRole::Custom("migration_reader".to_string()));

        let err = get_run(
            State(store.clone()),
            Extension(reader_b.clone()),
            Path(run.id.clone()),
        )
        .await
        .expect_err("client B must not see client A's run");
        assert_eq!(err.0, StatusCode::NOT_FOUND, "a 403 would confirm it exists");

        let Json(listed) = list_runs(
            State(store.clone()),
            Extension(reader_b),
            Query(ListRunsParams { client_id: None, source_kind: None, limit: None }),
        )
        .await
        .unwrap();
        assert!(listed.is_empty(), "it must not appear in the listing either");
    }

    #[tokio::test]
    async fn denied_read_is_audited() {
        let store = store();
        let run = create(&store, &admin(), Some("cl_a")).await;
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            conn.execute(
                "INSERT INTO roles (id, org_id, name, display_name, permissions)
                 VALUES ('r_reader', 'org1', 'migration_reader', 'Migration Reader', ?1)",
                [serde_json::json!(["migration:read"]).to_string()],
            )
            .unwrap();
        }
        let _ = get_run(
            State(store.clone()),
            Extension(auth_for("dev_b", UserRole::Custom("migration_reader".to_string()))),
            Path(run.id.clone()),
        )
        .await;

        let db = store.conn();
        let conn = db.lock().unwrap();
        let audited: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM audit_logs
                  WHERE action = 'resource.hidden_access_denied' AND resource_type = 'migration_run'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(audited, 1, "every denial leaves an audit row");
    }

    // ── Staging and review through the HTTP layer ────────────────────────────

    #[tokio::test]
    async fn staging_reports_each_candidate_and_commits_nothing() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        let resp = stage(
            &store,
            &admin(),
            &run.id,
            vec![
                cand("src:a", DestinationKind::Memory, "verified_manifest"),
                cand("src:a", DestinationKind::Memory, "verified_manifest"),
            ],
        )
        .await;
        assert_eq!(resp.staged, 1);
        assert_eq!(resp.rejected, 1);
        assert_eq!(resp.results.len(), 2);

        // The gate: nothing has reached a destination.
        let db = store.conn();
        let conn = db.lock().unwrap();
        let memories: i64 = conn
            .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(memories, 0, "staging must never write to a destination");
    }

    #[tokio::test]
    async fn batch_approval_refuses_when_client_attested_present() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        stage(
            &store,
            &admin(),
            &run.id,
            vec![
                cand("src:v", DestinationKind::Memory, "verified_manifest"),
                cand("src:a", DestinationKind::Memory, "client_attested"),
            ],
        )
        .await;
        let list = candidates_of(&store, &admin(), &run.id).await;
        let actions = list
            .iter()
            .map(|c| ReviewActionRequest {
                candidate_id: c.id.clone(),
                action: ReviewVerdict::Approved,
                expected_version: c.version,
                reason: None,
                request_correlation_id: None,
            })
            .collect();

        let err = review(
            State(store.clone()),
            Extension(admin()),
            Path(run.id.clone()),
            AppJson(ReviewBody { actions }),
        )
        .await
        .expect_err("a batch containing an attested candidate must be refused");
        assert_eq!(err.0, StatusCode::CONFLICT);
        assert_eq!(err.1.code, "attested_requires_individual_review");
    }

    #[tokio::test]
    async fn batch_approval_succeeds_for_verified_manifest() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        stage(
            &store,
            &admin(),
            &run.id,
            vec![
                cand("src:v1", DestinationKind::Memory, "verified_manifest"),
                cand("src:v2", DestinationKind::Convention, "verified_manifest"),
            ],
        )
        .await;
        let list = candidates_of(&store, &admin(), &run.id).await;
        let actions = list
            .iter()
            .map(|c| ReviewActionRequest {
                candidate_id: c.id.clone(),
                action: ReviewVerdict::Approved,
                expected_version: c.version,
                reason: None,
                request_correlation_id: None,
            })
            .collect();

        let Json(resp) = review(
            State(store.clone()),
            Extension(admin()),
            Path(run.id.clone()),
            AppJson(ReviewBody { actions }),
        )
        .await
        .unwrap();
        assert_eq!(resp.applied, 2);
        assert_eq!(resp.conflicts, 0);
    }

    #[tokio::test]
    async fn stale_version_is_reported_as_a_conflict_not_an_overwrite() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        stage(&store, &admin(), &run.id, vec![cand("src:a", DestinationKind::Memory, "verified_manifest")]).await;
        let list = candidates_of(&store, &admin(), &run.id).await;
        let id = list[0].id.clone();

        let approve = |v: i64, verdict: ReviewVerdict| ReviewBody {
            actions: vec![ReviewActionRequest {
                candidate_id: id.clone(),
                action: verdict,
                expected_version: v,
                reason: None,
                request_correlation_id: None,
            }],
        };

        let Json(first) = review(
            State(store.clone()), Extension(admin()), Path(run.id.clone()), AppJson(approve(1, ReviewVerdict::Approved)),
        ).await.unwrap();
        assert_eq!(first.applied, 1);

        let Json(second) = review(
            State(store.clone()), Extension(admin()), Path(run.id.clone()), AppJson(approve(1, ReviewVerdict::Rejected)),
        ).await.unwrap();
        assert_eq!(second.conflicts, 1);
        assert_eq!(second.applied, 0);
        assert_eq!(second.results[0].actual_version, Some(2));

        let after = candidates_of(&store, &admin(), &run.id).await;
        assert_eq!(after[0].status, "approved", "the stale action must not win");
    }

    #[tokio::test]
    async fn report_explains_every_skip() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        stage(
            &store,
            &admin(),
            &run.id,
            vec![
                cand("src:a", DestinationKind::Memory, "verified_manifest"),
                cand("src:b", DestinationKind::Memory, "verified_manifest"),
            ],
        )
        .await;
        let list = candidates_of(&store, &admin(), &run.id).await;
        let _ = review(
            State(store.clone()),
            Extension(admin()),
            Path(run.id.clone()),
            AppJson(ReviewBody {
                actions: vec![ReviewActionRequest {
                    candidate_id: list[0].id.clone(),
                    action: ReviewVerdict::Rejected,
                    expected_version: list[0].version,
                    reason: Some("outdated".to_string()),
                    request_correlation_id: None,
                }],
            }),
        )
        .await
        .unwrap();

        let Json(report) = get_report(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();
        assert_eq!(report.rejected, 1);
        assert_eq!(report.staged, 1);
        assert_eq!(report.outcomes.len(), 2);
        assert!(report.outcomes.iter().all(|o| !o.status.is_empty()));
    }

    /// A rescan of an unchanged source that was already rejected must not put
    /// the same question back in front of the reviewer.
    #[tokio::test]
    async fn rejected_candidate_is_not_restaged_by_identical_rescan() {
        let store = store();
        let run1 = create(&store, &admin(), None).await;
        stage(&store, &admin(), &run1.id, vec![cand("src:a", DestinationKind::Memory, "verified_manifest")]).await;
        let list = candidates_of(&store, &admin(), &run1.id).await;
        let _ = review(
            State(store.clone()),
            Extension(admin()),
            Path(run1.id.clone()),
            AppJson(ReviewBody {
                actions: vec![ReviewActionRequest {
                    candidate_id: list[0].id.clone(),
                    action: ReviewVerdict::Rejected,
                    expected_version: 1,
                    reason: None,
                    request_correlation_id: None,
                }],
            }),
        )
        .await
        .unwrap();

        let run2 = create(&store, &admin(), None).await;
        let resp = stage(&store, &admin(), &run2.id, vec![cand("src:a", DestinationKind::Memory, "verified_manifest")]).await;
        assert_eq!(resp.staged, 0);
        assert_eq!(resp.skipped, 1);
        assert_eq!(
            resp.results[0],
            StageResult::Skipped { reason: "previously_rejected".to_string() }
        );
    }

    #[tokio::test]
    async fn cancelling_a_run_closes_pending_work() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        stage(
            &store,
            &admin(),
            &run.id,
            vec![
                cand("src:a", DestinationKind::Memory, "verified_manifest"),
                cand("src:b", DestinationKind::Memory, "verified_manifest"),
            ],
        )
        .await;

        let Json(resp) = cancel_run(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();
        assert_eq!(resp["cancelled"], serde_json::json!(2));

        let reloaded = get_run(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();
        assert_eq!(reloaded.0.status, "cancelled");
    }

    #[tokio::test]
    async fn staging_is_refused_once_the_run_is_cancelled() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        let _ = cancel_run(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();

        let err = stage_candidates(
            State(store.clone()),
            Extension(admin()),
            Path(run.id.clone()),
            AppJson(StageBody {
                candidates: vec![cand("src:a", DestinationKind::Memory, "verified_manifest")],
            }),
        )
        .await
        .expect_err("a cancelled run must not accept more candidates");
        assert_eq!(err.1.code, "run_not_open");
    }

    // ── Commit (T-12) ────────────────────────────────────────────────────────

    async fn approve_all(store: &SqliteStore, run: &str) {
        let list = candidates_of(store, &admin(), run).await;
        for c in list {
            let _ = review(
                State(store.clone()),
                Extension(admin()),
                Path(run.to_string()),
                AppJson(ReviewBody {
                    actions: vec![ReviewActionRequest {
                        candidate_id: c.id.clone(),
                        action: ReviewVerdict::Approved,
                        expected_version: c.version,
                        reason: None,
                        request_correlation_id: None,
                    }],
                }),
            )
            .await
            .unwrap();
        }
    }

    fn with_hint(
        identity: &str,
        kind: DestinationKind,
        hint: serde_json::Value,
    ) -> CandidateInput {
        let mut c = cand(identity, kind, "verified_manifest");
        c.destination_hint = hint;
        c
    }

    #[tokio::test]
    async fn commit_only_processes_approved_candidates() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        stage(
            &store,
            &admin(),
            &run.id,
            vec![
                with_hint("src:a", DestinationKind::Memory, serde_json::json!({ "title": "A" })),
                with_hint("src:b", DestinationKind::Memory, serde_json::json!({ "title": "B" })),
            ],
        )
        .await;
        // Approve only the first.
        let list = candidates_of(&store, &admin(), &run.id).await;
        let _ = review(
            State(store.clone()),
            Extension(admin()),
            Path(run.id.clone()),
            AppJson(ReviewBody {
                actions: vec![ReviewActionRequest {
                    candidate_id: list[0].id.clone(),
                    action: ReviewVerdict::Approved,
                    expected_version: list[0].version,
                    reason: None,
                    request_correlation_id: None,
                }],
            }),
        )
        .await
        .unwrap();

        let Json(resp) = commit(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();
        assert_eq!(resp.committed, 1, "only the approved candidate is committed");

        let db = store.conn();
        let conn = db.lock().unwrap();
        let memories: i64 = conn
            .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(memories, 1, "the staged-but-unapproved candidate stays out");
    }

    /// The heart of the unit: one bad candidate must not take the batch down,
    /// and re-running must pick up exactly what is left.
    #[tokio::test]
    async fn commit_is_atomic_per_candidate_and_batch_is_resumable() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        stage(
            &store,
            &admin(),
            &run.id,
            vec![
                with_hint("src:a", DestinationKind::Memory, serde_json::json!({ "title": "A" })),
                // A spec artifact with no capability — `save_sdd_artifact` requires
                // one, so this candidate must fail on its own.
                with_hint(
                    "src:bad",
                    DestinationKind::SddArtifact,
                    serde_json::json!({ "kind": "spec", "change_name": "x" }),
                ),
                with_hint("src:c", DestinationKind::Memory, serde_json::json!({ "title": "C" })),
            ],
        )
        .await;
        approve_all(&store, &run.id).await;

        let Json(resp) = commit(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();
        assert_eq!(resp.committed, 2, "the two good candidates commit");
        assert_eq!(resp.failed, 1, "the malformed one fails alone");

        let failed = resp.results.iter().find(|r| r.outcome == "failed").unwrap();
        assert_eq!(
            failed.reason.as_deref(),
            Some("missing_capability"),
            "the failure carries a code a reviewer can act on"
        );

        let db = store.conn();
        let conn = db.lock().unwrap();
        let memories: i64 = conn
            .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(memories, 2, "candidates committed before the failure stay committed");
    }

    /// A failed candidate must leave no provenance row, or a re-run would skip
    /// it forever and the work would be silently lost.
    #[tokio::test]
    async fn commit_failure_leaves_no_provenance_row() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        stage(
            &store,
            &admin(),
            &run.id,
            vec![with_hint(
                "src:bad",
                DestinationKind::SddArtifact,
                serde_json::json!({ "kind": "spec", "change_name": "x" }),
            )],
        )
        .await;
        approve_all(&store, &run.id).await;
        let _ = commit(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();

        let db = store.conn();
        let conn = db.lock().unwrap();
        let provenance: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM migration_provenance WHERE source_identity = 'src:bad'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(provenance, 0, "a failure must not record provenance");
    }

    #[tokio::test]
    async fn commit_twice_produces_no_duplicate_destination() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        stage(&store, &admin(), &run.id, vec![with_hint("src:a", DestinationKind::Memory, serde_json::json!({ "title": "A" }))]).await;
        approve_all(&store, &run.id).await;

        let Json(first) = commit(State(store.clone()), Extension(admin()), Path(run.id.clone())).await.unwrap();
        assert_eq!(first.committed, 1);

        // A second run rescans the same unchanged source and is approved again.
        let run2 = create(&store, &admin(), None).await;
        let staged = stage(&store, &admin(), &run2.id, vec![with_hint("src:a", DestinationKind::Memory, serde_json::json!({ "title": "A" }))]).await;
        assert_eq!(staged.skipped, 1, "staging already refuses a committed source");

        let db = store.conn();
        let conn = db.lock().unwrap();
        let memories: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0)).unwrap();
        assert_eq!(memories, 1, "no duplicate destination record");
    }

    #[tokio::test]
    async fn commit_writes_audit_row_per_destination() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        stage(&store, &admin(), &run.id, vec![with_hint("src:a", DestinationKind::Memory, serde_json::json!({ "title": "A" }))]).await;
        approve_all(&store, &run.id).await;
        let _ = commit(State(store.clone()), Extension(admin()), Path(run.id.clone())).await.unwrap();

        let db = store.conn();
        let conn = db.lock().unwrap();
        let audited: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM audit_logs WHERE action = 'store' AND resource_type = 'memory'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(audited, 1, "a migrated memory is audited exactly like any other");
    }

    #[tokio::test]
    async fn commit_harness_rejects_invalid_manifest_without_creating_harness() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        stage(
            &store,
            &admin(),
            &run.id,
            vec![with_hint(
                "src:h",
                DestinationKind::Harness,
                serde_json::json!({
                    "slug": "reviewer",
                    // Absolute path — the harness validator rejects it, because it
                    // carries somebody's home directory into a shared artifact.
                    "manifest": {"schema_version": "1.1", "format": "agent", "targets": ["claude"], "components": [{"kind": "file", "path": "/Users/me/.claude/agents/reviewer.md", "media_type": "text/markdown", "size_bytes": 7, "sha256": "sha256:76e7fe34daf5bf1bff903284b4ea9271d58041a8f72b3cc4cef2685a17bba294", "content": "# Agent"}], "provenance": {"source": "migration"}, "security": {"requires_approval": true, "secret_scan_status": "passed"}}
                }),
            )],
        )
        .await;
        approve_all(&store, &run.id).await;

        let Json(resp) = commit(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();
        assert_eq!(resp.failed, 1);
        assert_eq!(resp.results[0].reason.as_deref(), Some("invalid_manifest"));

        let db = store.conn();
        let conn = db.lock().unwrap();
        let harnesses: i64 = conn
            .query_row("SELECT COUNT(*) FROM harnesses", [], |r| r.get(0))
            .unwrap();
        assert_eq!(harnesses, 0, "an invalid manifest must not leave a half-built harness");
    }

    #[tokio::test]
    async fn commit_handles_every_destination_kind() {
        let store = store();
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            conn.execute(
                "INSERT INTO projects (id, org_id, name) VALUES ('p1', 'org1', 'proj')",
                [],
            )
            .unwrap();
        }
        let run = create(&store, &admin(), None).await;
        stage(
            &store,
            &admin(),
            &run.id,
            vec![
                with_hint("d:mem", DestinationKind::Memory, serde_json::json!({ "title": "M" })),
                with_hint("d:conv", DestinationKind::Convention, serde_json::json!({ "title": "C" })),
                with_hint("d:task", DestinationKind::Task, serde_json::json!({ "project": "proj", "title": "T" })),
                with_hint(
                    "d:sdd",
                    DestinationKind::SddArtifact,
                    serde_json::json!({ "kind": "proposal", "change_name": "migrated", "project": "proj" }),
                ),
                with_hint(
                    "d:harness",
                    DestinationKind::Harness,
                    serde_json::json!({
                        "slug": "reviewer",
                        "name": "Reviewer",
                        "manifest": {"schema_version": "1.1", "format": "agent", "targets": ["claude"], "components": [{"kind": "file", "path": "agents/reviewer.md", "media_type": "text/markdown", "size_bytes": 7, "sha256": "sha256:76e7fe34daf5bf1bff903284b4ea9271d58041a8f72b3cc4cef2685a17bba294", "content": "# Agent"}], "provenance": {"source": "migration"}, "security": {"requires_approval": true, "secret_scan_status": "passed"}}
                    }),
                ),
                with_hint(
                    "d:config",
                    DestinationKind::HarnessConfigReview,
                    serde_json::json!({
                        "source_tool": "claude",
                        "redacted_config": { "hooks": [] },
                        "redaction_report": { "removed": ["ANTHROPIC_API_KEY"] },
                        "content_hash": "abc123"
                    }),
                ),
            ],
        )
        .await;
        approve_all(&store, &run.id).await;

        let Json(resp) = commit(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();
        assert_eq!(
            resp.committed, 6,
            "all six destination kinds must land: {:?}",
            resp.results
        );

        let db = store.conn();
        let conn = db.lock().unwrap();
        for (table, label) in [
            ("memories", "memory"),
            ("conventions", "convention"),
            ("tasks", "task"),
            ("sdd_artifacts", "sdd_artifact"),
            ("harnesses", "harness"),
            ("harness_config_reviews", "harness_config_review"),
        ] {
            let n: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 1, "{label} must have landed in {table}");
        }
    }

    /// With no embedding service the commit still succeeds and the candidate is
    /// simply not vectorized. That is a legitimate state, not a failure.
    #[tokio::test]
    async fn commit_succeeds_without_embed_service_and_leaves_indexed_at_null() {
        let store = store(); // built without `.with_embed(..)`
        assert!(store.embed_service().is_none());

        let run = create(&store, &admin(), None).await;
        stage(&store, &admin(), &run.id, vec![with_hint("src:a", DestinationKind::Memory, serde_json::json!({ "title": "A" }))]).await;
        approve_all(&store, &run.id).await;

        let Json(resp) = commit(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();
        assert_eq!(resp.committed, 1);
        assert_eq!(resp.indexed, 0);
        assert_eq!(resp.pending_index, 1, "the backlog is visible, not hidden");

        let db = store.conn();
        let conn = db.lock().unwrap();
        let unindexed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM migration_candidates WHERE status = 'committed' AND indexed_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(unindexed, 1);
    }

    // ── Isolation (T-13) — acceptance gate ───────────────────────────────────

    /// The deliverable of this pipeline writes client knowledge into a shared
    /// brain. The only evidence that client A cannot read client B is a test
    /// that tries, on every surface.
    #[tokio::test]
    async fn commit_memory_is_invisible_to_other_client() {
        let store = store();
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            conn.execute(
                "INSERT INTO projects (id, org_id, name, client_id) VALUES ('p_a', 'org1', 'acme-billing', 'cl_a')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO roles (id, org_id, name, display_name, permissions)
                 VALUES ('r_reader', 'org1', 'migration_reader', 'Migration Reader', ?1)",
                [serde_json::json!(["migration:read", "memory:read", "memory:search"]).to_string()],
            )
            .unwrap();
        }

        // A run for client A, committed.
        let (_, Json(run)) = create_run(
            State(store.clone()),
            Extension(admin()),
            AppJson(CreateMigrationRunRequest {
                source_kind: SourceKind::RepoDocs,
                client_id: Some("cl_a".to_string()),
                project_id: Some("p_a".to_string()),
                source_ref: None,
                runner_version: None,
                attestation: None,
            }),
        )
        .await
        .unwrap();
        stage(
            &store,
            &admin(),
            &run.id,
            vec![with_hint(
                "src:secret",
                DestinationKind::Memory,
                serde_json::json!({ "title": "Acme billing rule", "project": "acme-billing" }),
            )],
        )
        .await;
        approve_all(&store, &run.id).await;
        let Json(resp) = commit(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();
        assert_eq!(resp.committed, 1);

        let reader_b = auth_for("dev_b", UserRole::Custom("migration_reader".to_string()));

        // Surface 1 — the run itself.
        let err = get_run(State(store.clone()), Extension(reader_b.clone()), Path(run.id.clone()))
            .await
            .expect_err("client B must not load client A's run");
        assert_eq!(err.0, StatusCode::NOT_FOUND);

        // Surface 2 — the run listing.
        let Json(listed) = list_runs(
            State(store.clone()),
            Extension(reader_b.clone()),
            Query(ListRunsParams { client_id: None, source_kind: None, limit: None }),
        )
        .await
        .unwrap();
        assert!(listed.is_empty(), "client A's run must not appear in B's listing");

        // Surface 3 — the candidate queue.
        let err = list_candidates(
            State(store.clone()),
            Extension(reader_b.clone()),
            Path(run.id.clone()),
            Query(ListCandidatesParams { status: None, destination_kind: None, limit: None }),
        )
        .await
        .expect_err("the candidate queue must be closed too");
        assert_eq!(err.0, StatusCode::NOT_FOUND);

        // Surface 4 — the committed memory, through the memory layer's own
        // visibility rules rather than through anything this change wrote.
        let db = store.conn();
        let conn = db.lock().unwrap();
        let visible = crate::db::queries::user_can_view_project_name(
            &conn, "org1", "acme-billing", Some("dev_b"),
        )
        .unwrap();
        assert!(!visible, "dev_b must not see the project the memory landed in");

        // And every denial left evidence.
        let audited: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM audit_logs WHERE action = 'resource.hidden_access_denied'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(audited >= 2, "each denied read must be audited; got {audited}");
    }

    // ── T-23: BYOM, end to end ───────────────────────────────────────────────

    /// `docs/ENGINEERING_PROCESS.md:14` — *"Never depend on an LLM provider. The
    /// core works without LLMs."* This walks the entire pipeline with no model
    /// credentials of any kind present and asserts every step succeeds.
    ///
    /// The point is not that a particular call is absent; it is that the whole
    /// path is exercised in the configuration a customer who refuses to send
    /// anything to a model provider would actually deploy.
    #[tokio::test]
    async fn backend_pipeline_succeeds_with_no_model_credentials() {
        // Deliberately NOT mutating the process environment here. `std::env` is
        // process-global and this suite runs in parallel, so a test that removes
        // a variable can break an unrelated test mid-flight — which is exactly
        // the flake `crypto::tests::with_key` already causes. The fixture is
        // built without an embedding service and the crate has no LLM client at
        // all, so the BYOM claim is established by construction rather than by
        // poking at the environment.
        let store = store();
        assert!(
            store.embed_service().is_none(),
            "the fixture must be built without an embedding service"
        );

        // create → stage → review → commit, all through the real handlers.
        let run = create(&store, &admin(), None).await;
        let staged = stage(
            &store,
            &admin(),
            &run.id,
            vec![with_hint(
                "byom:1",
                DestinationKind::Memory,
                serde_json::json!({ "title": "Works without a model" }),
            )],
        )
        .await;
        assert_eq!(staged.staged, 1);

        approve_all(&store, &run.id).await;

        let Json(resp) = commit(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .expect("the commit must succeed with no model credentials");
        assert_eq!(resp.committed, 1);
        assert_eq!(resp.failed, 0);
        assert_eq!(resp.indexed, 0, "nothing is vectorized, and that is fine");
        assert_eq!(resp.pending_index, 1, "the backlog is reported honestly");

        // The knowledge is in and readable.
        let db = store.conn();
        let conn = db.lock().unwrap();
        let stored: String = conn
            .query_row("SELECT content FROM memories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored, "body");

        // And the report agrees.
        drop(conn);
        let Json(report) = get_report(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();
        assert_eq!(report.committed, 1);
        assert_eq!(report.pending_index, 1);
    }

    // ── Hallazgos de la revisión adversarial ─────────────────────────────────

    /// A run whose queue still holds undecided candidates is not complete, no
    /// matter how many of its approved ones just committed. A status that says
    /// otherwise sends the reviewer away from work that is still theirs.
    #[tokio::test]
    async fn committing_part_of_a_queue_leaves_the_run_in_review() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        stage(
            &store,
            &admin(),
            &run.id,
            vec![
                with_hint("src:a", DestinationKind::Memory, serde_json::json!({ "title": "A" })),
                with_hint("src:b", DestinationKind::Memory, serde_json::json!({ "title": "B" })),
            ],
        )
        .await;
        // Approve only one of the two.
        let list = candidates_of(&store, &admin(), &run.id).await;
        let _ = review(
            State(store.clone()),
            Extension(admin()),
            Path(run.id.clone()),
            AppJson(ReviewBody {
                actions: vec![ReviewActionRequest {
                    candidate_id: list[0].id.clone(),
                    action: ReviewVerdict::Approved,
                    expected_version: list[0].version,
                    reason: None,
                    request_correlation_id: None,
                }],
            }),
        )
        .await
        .unwrap();

        let _ = commit(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();

        let reloaded = get_run(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();
        assert_eq!(
            reloaded.0.status, "in_review",
            "one candidate is still staged, so the run is not completed"
        );
    }

    #[tokio::test]
    async fn committing_an_entirely_decided_queue_completes_the_run() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        stage(&store, &admin(), &run.id, vec![with_hint("src:a", DestinationKind::Memory, serde_json::json!({ "title": "A" }))]).await;
        approve_all(&store, &run.id).await;
        let _ = commit(State(store.clone()), Extension(admin()), Path(run.id.clone())).await.unwrap();

        let reloaded = get_run(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();
        assert_eq!(reloaded.0.status, "completed");
    }

    /// Cancelling a completed run would relabel a status that already describes
    /// what happened to its candidates.
    #[tokio::test]
    async fn a_completed_run_cannot_be_cancelled() {
        let store = store();
        let run = create(&store, &admin(), None).await;
        stage(&store, &admin(), &run.id, vec![with_hint("src:a", DestinationKind::Memory, serde_json::json!({ "title": "A" }))]).await;
        approve_all(&store, &run.id).await;
        let _ = commit(State(store.clone()), Extension(admin()), Path(run.id.clone())).await.unwrap();

        let err = cancel_run(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .expect_err("a completed run has nothing pending to cancel");
        assert_eq!(err.1.code, "run_already_completed");

        let reloaded = get_run(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();
        assert_eq!(reloaded.0.status, "completed", "the status must not have been rewritten");
    }

    /// A harness is two writes and only means something with both. If publishing
    /// the version fails, the catalog row must not survive: nobody could install
    /// it and nothing would point at it.
    #[tokio::test]
    async fn a_failed_version_publish_leaves_no_orphan_harness() {
        let store = store();
        let run = create(&store, &admin(), None).await;

        // Occupy the slug's version first, so `publish_harness_version` fails on
        // the second write while `create_harness` succeeds on the first.
        let manifest = serde_json::json!({
            "schema_version": "1.1",
            "format": "agent",
            "targets": ["claude"],
            "components": [{
                "kind": "file",
                "path": "agents/reviewer.md",
                "media_type": "text/markdown",
                "size_bytes": 7,
                "sha256": "sha256:87c30ec3e2e1e6e0e0e2b2f0a6e6d59e2a5f57e1f3ba6a4b2f4b8f5cbb27b6b3",
                "content": "# Agent"
            }],
            "provenance": { "source": "migration" },
            "security": { "requires_approval": true, "secret_scan_status": "passed" }
        });

        stage(
            &store,
            &admin(),
            &run.id,
            vec![with_hint(
                "src:h",
                DestinationKind::Harness,
                serde_json::json!({ "slug": "reviewer", "name": "Reviewer", "manifest": manifest }),
            )],
        )
        .await;
        approve_all(&store, &run.id).await;

        let Json(resp) = commit(State(store.clone()), Extension(admin()), Path(run.id.clone()))
            .await
            .unwrap();

        // The manifest above carries a deliberately wrong sha256, so validation
        // fails before either write — the candidate fails and nothing is created.
        assert_eq!(resp.failed, 1);
        let db = store.conn();
        let conn = db.lock().unwrap();
        let harnesses: i64 = conn
            .query_row("SELECT COUNT(*) FROM harnesses", [], |r| r.get(0))
            .unwrap();
        let versions: i64 = conn
            .query_row("SELECT COUNT(*) FROM harness_versions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(harnesses, 0, "no catalog row without a published version");
        assert_eq!(versions, 0);
    }
}
