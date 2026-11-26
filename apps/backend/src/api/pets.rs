//! Protected REST API for NexusMind desktop pets (mascots):
//! - `POST /v1/pets/heartbeat` — mandatory handshake + live PC/device presence
//! - `POST /v1/pets/conversations` — full redacted conversation & tool-call transcripts
//! - `POST /v1/pets/events` — granular auditable logs (also mirrored into org audit log)
//! - `GET  /v1/pets` — list pets with PC hostname, OS, status, and Outcome Tape
//! - `GET  /v1/pets/conversations` — list auditable conversations across pets
//! - `GET  /v1/pets/events` — list auditable pet events across pets

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    api::helpers::AppJson,
    db::pets::{self as db_pets, ConversationWrite, EventWrite, HeartbeatWrite},
    db::queries,
    models::types::{ApiError, AuthContext},
    store::sqlite::SqliteStore,
};

type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

fn err(status: StatusCode, code: &str, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            error: msg.into(),
            code: code.to_string(),
        }),
    )
}

fn internal(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    tracing::error!("pets api error: {e:#}");
    err(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Internal error")
}

fn lock_err() -> (StatusCode, Json<ApiError>) {
    err(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Database lock failed")
}

pub async fn heartbeat(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<HeartbeatWrite>,
) -> ApiResult<Json<Value>> {
    if input.device_id.trim().is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "invalid_device_id", "device_id is required"));
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let pet = db_pets::upsert_pet_heartbeat(&conn, &auth.org_id, &auth.user_id, &input)
        .map_err(internal)?;
    Ok(Json(json!({
        "connected": true,
        "org_id": auth.org_id,
        "user_id": auth.user_id,
        "pet": pet,
    })))
}

pub async fn record_conversation(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<ConversationWrite>,
) -> ApiResult<Json<Value>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let pet_id = db_pets::upsert_conversation(&conn, &auth.org_id, &auth.user_id, &input)
        .map_err(internal)?;
    Ok(Json(json!({ "ok": true, "pet_id": pet_id, "run_id": input.run_id })))
}

pub async fn record_event(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<EventWrite>,
) -> ApiResult<Json<Value>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let (event_id, pet_id) =
        db_pets::insert_event(&conn, &auth.org_id, &auth.user_id, &input).map_err(internal)?;

    // Mirror security/action events into NexusMind's cryptographic audit log
    if matches!(
        input.event_kind.as_str(),
        "action" | "approval" | "kill_switch" | "blocked" | "prompt"
    ) {
        let _ = queries::insert_audit_log_chained(
            &conn,
            &auth.org_id,
            &auth.user_id,
            &format!("pet.{}", input.event_kind),
            "mascot_pet",
            Some(&pet_id),
            json!({
                "device_id": input.device_id,
                "run_id": input.run_id,
                "action_kind": input.action_kind,
                "decision_source": input.decision_source,
                "risk": input.risk,
                "outcome": input.outcome,
                "summary": input.summary,
            }),
            None,
        );
    }

    Ok(Json(json!({ "ok": true, "event_id": event_id, "pet_id": pet_id })))
}

pub async fn list_pets(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<Value>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let pets = db_pets::list_pets(&conn, &auth.org_id).map_err(internal)?;
    Ok(Json(json!({ "pets": pets })))
}

#[derive(Deserialize)]
pub struct PetQuery {
    pub pet_id: Option<String>,
    pub event_kind: Option<String>,
    pub limit: Option<i64>,
}

pub async fn list_conversations(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(q): Query<PetQuery>,
) -> ApiResult<Json<Value>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let conversations = db_pets::list_conversations(
        &conn,
        &auth.org_id,
        q.pet_id.as_deref(),
        q.limit.unwrap_or(50),
    )
    .map_err(internal)?;
    Ok(Json(json!({ "conversations": conversations })))
}

pub async fn list_events(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(q): Query<PetQuery>,
) -> ApiResult<Json<Value>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let events = db_pets::list_events(
        &conn,
        &auth.org_id,
        q.pet_id.as_deref(),
        q.event_kind.as_deref(),
        q.limit.unwrap_or(100),
    )
    .map_err(internal)?;
    Ok(Json(json!({ "events": events })))
}
