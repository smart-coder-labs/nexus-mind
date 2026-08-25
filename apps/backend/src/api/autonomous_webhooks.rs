use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::{db::queries, models::types::ApiError, store::sqlite::SqliteStore};

fn reject(status: StatusCode, code: &str) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            error: code.replace('_', " "),
            code: code.into(),
        }),
    )
}

fn verify_signature(secret: &str, body: &[u8], header: &str) -> bool {
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
    let action = payload.get("action").and_then(|value| value.as_str());
    match (event, action) {
        ("issues", Some("opened" | "reopened" | "labeled")) => payload
            .pointer("/issue/number")
            .and_then(|value| value.as_i64())
            .map(|number| ("github_issue", number, None)),
        ("pull_request", Some("opened" | "synchronize" | "ready_for_review"))
            if payload
                .pointer("/pull_request/head/repo/fork")
                .and_then(|value| value.as_bool())
                != Some(true) =>
        {
            payload
                .pointer("/pull_request/number")
                .and_then(|value| value.as_i64())
                .map(|number| {
                    (
                        "github_pr",
                        number,
                        payload
                            .pointer("/pull_request/head/sha")
                            .and_then(|value| value.as_str()),
                    )
                })
        }
        _ => None,
    }
}

pub async fn github_webhook(
    State(store): State<SqliteStore>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if body.len() > 2 * 1024 * 1024 {
        return Err(reject(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"));
    }
    let delivery = headers
        .get("x-github-delivery")
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| reject(StatusCode::BAD_REQUEST, "missing_delivery_id"))?;
    let event = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| reject(StatusCode::BAD_REQUEST, "missing_event"))?;
    if !matches!(
        event,
        "issues"
            | "pull_request"
            | "pull_request_review"
            | "installation"
            | "installation_repositories"
    ) {
        return Ok(StatusCode::ACCEPTED);
    }
    let payload: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|_| reject(StatusCode::BAD_REQUEST, "invalid_json"))?;
    let installation_id = payload
        .pointer("/installation/id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| reject(StatusCode::BAD_REQUEST, "missing_installation"))?;
    let (org_id, connector_id, secret_raw) = {
        let db = store.conn();
        let conn = db
            .lock()
            .map_err(|_| reject(StatusCode::INTERNAL_SERVER_ERROR, "database_lock"))?;
        queries::find_github_app_webhook_connector(&conn, installation_id)
            .map_err(|_| reject(StatusCode::INTERNAL_SERVER_ERROR, "database_error"))?
            .ok_or_else(|| reject(StatusCode::NOT_FOUND, "installation_not_found"))?
    };
    let secrets: serde_json::Value = serde_json::from_str(&secret_raw)
        .map_err(|_| reject(StatusCode::SERVICE_UNAVAILABLE, "github_secret_invalid"))?;
    let webhook_secret = secrets
        .get("webhook_secret")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            reject(
                StatusCode::SERVICE_UNAVAILABLE,
                "github_webhook_secret_missing",
            )
        })?;
    let signature = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| reject(StatusCode::UNAUTHORIZED, "missing_signature"))?;
    if !verify_signature(webhook_secret, &body, signature) {
        return Err(reject(StatusCode::UNAUTHORIZED, "invalid_signature"));
    }
    let action = payload.get("action").and_then(|v| v.as_str());
    let repository = payload
        .pointer("/repository/full_name")
        .and_then(|v| v.as_str());
    let hash = hex::encode(Sha256::digest(&body));
    let db = store.conn();
    let conn = db
        .lock()
        .map_err(|_| reject(StatusCode::INTERNAL_SERVER_ERROR, "database_lock"))?;
    let fresh = queries::record_github_webhook_delivery(
        &conn,
        &org_id,
        &connector_id,
        delivery,
        event,
        action,
        repository,
        &hash,
    )
    .map_err(|error| {
        if error.to_string() == "github_delivery_payload_mismatch" {
            reject(StatusCode::CONFLICT, "delivery_payload_mismatch")
        } else {
            reject(StatusCode::INTERNAL_SERVER_ERROR, "database_error")
        }
    })?;
    if fresh {
        if let (Some(repository), Some((kind, number, head_sha))) =
            (repository, github_trigger(event, &payload))
        {
            queries::enqueue_github_webhook_agents(
                &conn, &org_id, delivery, repository, kind, number, head_sha, &hash,
            )
            .map_err(|_| reject(StatusCode::INTERNAL_SERVER_ERROR, "trigger_enqueue_failed"))?;
        }
    }
    Ok(StatusCode::ACCEPTED)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let issue = serde_json::json!({"action":"labeled","issue":{"number":7}});
        assert_eq!(
            github_trigger("issues", &issue),
            Some(("github_issue", 7, None))
        );
        let edited = serde_json::json!({"action":"edited","issue":{"number":7}});
        assert!(github_trigger("issues", &edited).is_none());
        let fork = serde_json::json!({"action":"opened","pull_request":{"number":9,"head":{"sha":"abc","repo":{"fork":true}}}});
        assert!(github_trigger("pull_request", &fork).is_none());
        let pull = serde_json::json!({"action":"synchronize","pull_request":{"number":9,"head":{"sha":"abc","repo":{"fork":false}}}});
        assert_eq!(
            github_trigger("pull_request", &pull),
            Some(("github_pr", 9, Some("abc")))
        );
    }
}
