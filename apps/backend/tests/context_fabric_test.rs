use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use clap::Parser;
use nexusmind::db::{connection::connect, migrations, queries};
use nexusmind::{
    api::router,
    config::Config,
    context_fabric::{
        AssembleRequest, CandidateEvidence, GenerationRef, Locator, CONTRACT_VERSION,
    },
};
use tower::util::ServiceExt;

#[test]
fn policy_scope_is_applied_before_embedding_and_fts_candidate_limits() {
    let conn = connect(":memory:").unwrap();
    migrations::run(&conn).unwrap();
    let (org, admin, _) = queries::bootstrap(&conn, "Acme", "acme", "a@acme.com", "Admin").unwrap();
    let (member, _) =
        queries::invite_user(&conn, &org.id, "m@acme.com", "Member", "member").unwrap();
    let _secret = queries::create_project(&conn, &org.id, "secret", None, None).unwrap();
    let shared = queries::create_project(&conn, &org.id, "shared", None, None).unwrap();
    queries::upsert_project_member(&conn, &shared.id, &member.id, "member").unwrap();

    let secret_memory = queries::upsert_memory(
        &conn,
        &org.id,
        &admin.id,
        &nexusmind::models::types::StoreMemoryRequest {
            project: Some("secret".into()),
            tool: "claude".into(),
            content: "policy-first zebra".into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: None,
            session_id: None,
        },
    )
    .unwrap();
    let shared_memory = queries::upsert_memory(
        &conn,
        &org.id,
        &admin.id,
        &nexusmind::models::types::StoreMemoryRequest {
            project: Some("shared".into()),
            tool: "claude".into(),
            content: "policy-first zebra".into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: None,
            session_id: None,
        },
    )
    .unwrap();
    queries::store_embedding(&conn, &secret_memory.id, &[1, 2, 3, 4]).unwrap();
    queries::store_embedding(&conn, &shared_memory.id, &[5, 6, 7, 8]).unwrap();

    let embeddings =
        queries::get_embeddings_for_org_visible(&conn, &org.id, Some(&member.id)).unwrap();
    assert_eq!(embeddings.len(), 1);
    assert_eq!(embeddings[0].0, shared_memory.id);
    let fts = queries::search_memories_visible(&conn, &org.id, "policy-first", 1, Some(&member.id))
        .unwrap();
    assert_eq!(fts.len(), 1);
    assert_eq!(fts[0].id, shared_memory.id);
}

fn http_setup() -> (axum::Router, String, String, String, String) {
    let conn = connect(":memory:").unwrap();
    migrations::run(&conn).unwrap();
    let (org, admin, _) =
        queries::bootstrap(&conn, "HTTP Org", "http-org", "admin@http.test", "Admin").unwrap();
    let (member, member_key) =
        queries::invite_user(&conn, &org.id, "member@http.test", "Member", "member").unwrap();
    let memory = queries::upsert_memory(
        &conn,
        &org.id,
        &admin.id,
        &nexusmind::models::types::StoreMemoryRequest {
            project: None,
            tool: "claude".into(),
            content: "backend verified content".into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: None,
            session_id: None,
        },
    )
    .unwrap();
    let secret_project = queries::create_project(&conn, &org.id, "secret", None, None).unwrap();
    let secret_memory = queries::upsert_memory(
        &conn,
        &org.id,
        &admin.id,
        &nexusmind::models::types::StoreMemoryRequest {
            project: Some(secret_project.name),
            tool: "claude".into(),
            content: "secret backend content".into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: None,
            session_id: None,
        },
    )
    .unwrap();
    let app = router::build(conn, Config::parse_from(["context-fabric-test"]));
    (app, member_key, memory.id, secret_memory.id, member.id)
}

fn assemble_request(
    id: &str,
    content: &str,
    source: &str,
    provenance: &str,
    source_cap: usize,
) -> AssembleRequest {
    AssembleRequest {
        contract_version: CONTRACT_VERSION.into(),
        tokenizer: "whitespace-v0".into(),
        token_budget: 20,
        source_cap,
        excluded_sources: vec![],
        required_sources: vec![],
        generation: GenerationRef {
            id: "g1".into(),
            version: 1,
        },
        candidates: vec![CandidateEvidence {
            unit_id: id.into(),
            source: source.into(),
            content: content.into(),
            locator: Locator {
                source: source.into(),
                id: id.into(),
                reference: None,
            },
            provenance: provenance.into(),
            generation: GenerationRef {
                id: "g1".into(),
                version: 1,
            },
            fresh: true,
            required: false,
        }],
    }
}

async fn post_assemble(
    app: axum::Router,
    key: &str,
    request: AssembleRequest,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/context/assemble")
                .header("Authorization", format!("Bearer {key}"))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn http_assemble_accepts_backend_verified_memory() {
    let (app, key, id, _, _) = http_setup();
    let (status, body) = post_assemble(
        app,
        &key,
        assemble_request(
            &id,
            "backend verified content",
            "memory",
            "memory-search",
            1,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["abstained"], false);
    assert_eq!(body["units"][0]["content"], "backend verified content");
}

#[tokio::test]
async fn http_assemble_rejects_tampered_content_and_unverified_source_without_details() {
    let (app, key, id, _, _) = http_setup();
    let (status, body) = post_assemble(
        app,
        &key,
        assemble_request(&id, "tampered secret", "memory", "memory-search", 1),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "evidence_integrity_mismatch");
    assert!(!body.to_string().contains(&id));

    let (app, key, id, _, _) = http_setup();
    let (status, body) = post_assemble(
        app,
        &key,
        assemble_request(&id, "backend verified content", "code", "code-search", 1),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "unsupported_unverified_source");
}

#[tokio::test]
async fn http_assemble_rejects_invisible_memory_and_invalid_source_cap() {
    let (app, key, _visible_id, invisible_id, _member_id) = http_setup();
    let (status, body) = post_assemble(
        app,
        &key,
        assemble_request(
            &invisible_id,
            "secret backend content",
            "memory",
            "memory-search",
            1,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "evidence_not_found");
    assert!(!body.to_string().contains(&invisible_id));

    let (app, key, id, _, _) = http_setup();
    let (status, body) = post_assemble(
        app,
        &key,
        assemble_request(
            &id,
            "backend verified content",
            "memory",
            "memory-search",
            0,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "invalid_source_cap");
}
