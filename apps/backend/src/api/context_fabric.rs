use crate::{
    api::helpers::require_permission,
    context_fabric::{compile, AssembleRequest, AssembleResponse},
    models::types::{ApiError, AuthContext},
    store::sqlite::SqliteStore,
};
use axum::{extract::State, http::StatusCode, Extension, Json};
use std::collections::{HashMap, HashSet};

const VERIFIED_MEMORY_PROVENANCE: &str = "memory-search";

fn verification_error(code: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(ApiError {
            error: "Evidence could not be verified".into(),
            code: code.into(),
        }),
    )
}

fn verify_memory_evidence(
    conn: &rusqlite::Connection,
    auth: &AuthContext,
    request: AssembleRequest,
) -> Result<AssembleRequest, (StatusCode, Json<ApiError>)> {
    if request.source_cap == 0 {
        return Err(verification_error("invalid_source_cap"));
    }

    if request
        .candidates
        .iter()
        .any(|candidate| candidate.source != "memory")
    {
        return Err(verification_error("unsupported_unverified_source"));
    }

    let ids: Vec<String> = request
        .candidates
        .iter()
        .map(|candidate| candidate.locator.id.clone())
        .collect();
    let unique_ids: HashSet<&str> = ids.iter().map(String::as_str).collect();
    if unique_ids.len() != ids.len()
        || request.candidates.iter().any(|candidate| {
            candidate.unit_id.is_empty()
                || candidate.locator.id.is_empty()
                || candidate.unit_id != candidate.locator.id
                || candidate.locator.source != "memory"
        })
    {
        return Err(verification_error("invalid_memory_locator"));
    }

    // The visibility query is the authorization boundary. Missing rows are deliberately
    // indistinguishable from rows outside the caller's project/tenant scope.
    let viewer = if auth.role.is_super_user() {
        None
    } else {
        Some(auth.user_id.as_str())
    };
    let memories =
        crate::db::queries::get_memories_by_ids_visible(conn, &auth.org_id, &ids, viewer)
            .map_err(|_| verification_error("evidence_verification_failed"))?;
    if memories.len() != unique_ids.len() {
        return Err(verification_error("evidence_not_found"));
    }
    let memories: HashMap<String, crate::models::types::Memory> = memories
        .into_iter()
        .map(|memory| (memory.id.clone(), memory))
        .collect();

    for candidate in &request.candidates {
        let Some(memory) = memories.get(&candidate.locator.id) else {
            return Err(verification_error("evidence_not_found"));
        };
        if candidate.content != memory.content || candidate.provenance != VERIFIED_MEMORY_PROVENANCE
        {
            return Err(verification_error("evidence_integrity_mismatch"));
        }
    }

    Ok(request)
}

/// Read-only Compiler v0 boundary. Retrieval and authorization remain backend-owned.
pub async fn assemble(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Json(request): Json<AssembleRequest>,
) -> Result<Json<AssembleResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Database lock error".into(),
                code: "internal_error".into(),
            }),
        )
    })?;
    require_permission(&conn, &auth, None, "memory:read")?;
    let request = verify_memory_evidence(&conn, &auth, request)?;
    Ok(Json(compile(&request)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{connection::connect, migrations, queries},
        models::types::StoreMemoryRequest,
    };

    fn setup() -> (rusqlite::Connection, AuthContext, String) {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        let (org, user, _) =
            queries::bootstrap(&conn, "Acme", "acme", "a@acme.com", "Admin").unwrap();
        let memory = queries::upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &StoreMemoryRequest {
                project: None,
                tool: "claude".into(),
                content: "verified memory".into(),
                tags: None,
                title: None,
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: None,
            },
        )
        .unwrap();
        (
            conn,
            AuthContext {
                org_id: org.id,
                user_id: user.id,
                role: "member".parse().unwrap(),
            },
            memory.id,
        )
    }

    fn request(id: &str, content: &str, provenance: &str, source_cap: usize) -> AssembleRequest {
        AssembleRequest {
            contract_version: crate::context_fabric::CONTRACT_VERSION.into(),
            tokenizer: "whitespace-v0".into(),
            token_budget: 20,
            source_cap,
            excluded_sources: vec![],
            required_sources: vec![],
            generation: crate::context_fabric::GenerationRef {
                id: "g1".into(),
                version: 1,
            },
            candidates: vec![crate::context_fabric::CandidateEvidence {
                unit_id: id.into(),
                source: "memory".into(),
                content: content.into(),
                locator: crate::context_fabric::Locator {
                    source: "memory".into(),
                    id: id.into(),
                    reference: None,
                },
                provenance: provenance.into(),
                generation: crate::context_fabric::GenerationRef {
                    id: "g1".into(),
                    version: 1,
                },
                fresh: true,
                required: false,
            }],
        }
    }

    #[test]
    fn authorized_memory_is_verifiable() {
        let (conn, auth, id) = setup();
        let verified = verify_memory_evidence(
            &conn,
            &auth,
            request(&id, "verified memory", VERIFIED_MEMORY_PROVENANCE, 1),
        )
        .unwrap();
        assert_eq!(verified.candidates[0].locator.id, id);
    }

    #[test]
    fn manipulated_content_and_unknown_source_are_rejected_without_details() {
        let (conn, auth, id) = setup();
        let error = verify_memory_evidence(
            &conn,
            &auth,
            request(&id, "tampered", VERIFIED_MEMORY_PROVENANCE, 1),
        )
        .unwrap_err();
        assert_eq!(error.1 .0.code, "evidence_integrity_mismatch");
        let error = verify_memory_evidence(&conn, &auth, {
            let mut req = request(&id, "verified memory", VERIFIED_MEMORY_PROVENANCE, 1);
            req.candidates[0].source = "code".into();
            req
        })
        .unwrap_err();
        assert_eq!(error.1 .0.code, "unsupported_unverified_source");
    }

    #[test]
    fn zero_source_cap_is_rejected_deterministically() {
        let (conn, auth, id) = setup();
        let error = verify_memory_evidence(
            &conn,
            &auth,
            request(&id, "verified memory", VERIFIED_MEMORY_PROVENANCE, 0),
        )
        .unwrap_err();
        assert_eq!(error.1 .0.code, "invalid_source_cap");
    }
}
