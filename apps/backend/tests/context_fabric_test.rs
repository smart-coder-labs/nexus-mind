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
        AssembleRequest, AssembleResponse, CandidateEvidence, Claim, CompileDiagnostics,
        EvidenceReference, GenerateRequest, GenerationRef, Locator, VerifyRequest, CONTRACT_VERSION,
        DETERMINISTIC_PROVIDER,
    },
    models::types::ContextFabricMetadata,
    store::{sqlite::SqliteStore, MemoryStore},
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
            context_fabric_metadata: None,
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
            context_fabric_metadata: None,
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
            context_fabric_metadata: None,
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
            context_fabric_metadata: None,
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
        profile_id: None,
        profile_version: None,
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
            captured_at_unix: None,
            content_hash: None,
            snapshot: None,
            source_generation: None,
            tenant_scope: None,
            acl_generation: None,
            policy_generation: None,
        }],
        references: vec![],
        freshness_window_secs: None,
    }
}

fn references_request(references: Vec<EvidenceReference>) -> AssembleRequest {
    AssembleRequest {
        contract_version: CONTRACT_VERSION.into(),
        tokenizer: "whitespace-v0".into(),
        token_budget: 40,
        source_cap: 5,
        excluded_sources: vec![],
        required_sources: vec![],
        generation: GenerationRef { id: "g1".into(), version: 1 },
        profile_id: None,
        profile_version: None,
        candidates: vec![],
        references,
        freshness_window_secs: None,
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

async fn post_json(app: axum::Router, key: &str, uri: &str, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
    let response = app.oneshot(
        Request::builder().method("POST").uri(uri)
            .header("Authorization", format!("Bearer {key}"))
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap(),
    ).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn compiled(unit: CandidateEvidence, abstained: bool) -> AssembleResponse {
    AssembleResponse {
        contract_version: CONTRACT_VERSION.into(), abstained, units: if abstained { vec![] } else { vec![unit] },
        diagnostics: CompileDiagnostics { reason_codes: vec![], candidate_count: 1, selected_count: 1, omitted_sources: vec![], coverage: vec!["memory".into()] },
    }
}

fn generate_request(assembled: AssembleResponse, provider: &str, budget: usize) -> GenerateRequest {
    GenerateRequest { contract_version: CONTRACT_VERSION.into(), profile_id: "lab-profile".into(), profile_version: 1,
        generation: GenerationRef { id: "g1".into(), version: 1 }, model: "lab-model".into(), provider: provider.into(),
        output_token_budget: budget, assembled }
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
async fn http_assemble_resolves_visible_code_chunk_without_client_content() {
    let conn = connect(":memory:").unwrap();
    migrations::run(&conn).unwrap();
    let (org, _admin, _) = queries::bootstrap(&conn, "Code Org", "code-org", "a@code.test", "Admin").unwrap();
    let (member, member_key) = queries::invite_user(&conn, &org.id, "m@code.test", "Member", "member").unwrap();
    let canonical = queries::create_project(&conn, &org.id, "shared", None, None).unwrap();
    queries::upsert_project_member(&conn, &canonical.id, &member.id, "member").unwrap();
    let project_id = queries::upsert_code_project(&conn, &org.id, "shared", "/repo").unwrap();
    queries::set_code_project_success(&conn, project_id, 1, "2026-08-02T00:00:00Z").unwrap();
    let chunk_id = queries::insert_code_chunk(&conn, project_id, "src/lib.rs", "sha-code", Some("rust"), Some("visible"), 1, 3, "fn visible() {}", None).unwrap();
    let app = router::build(conn, Config::parse_from(["context-fabric-test"]));
    let request = references_request(vec![EvidenceReference {
        source: "code".into(),
        locator: Locator { source: "code".into(), id: chunk_id.to_string(), reference: Some("visible".into()) },
        expected_hash: Some("sha-code".into()),
        expected_generation: None,
    }]);
    let (status, body) = post_assemble(app, &member_key, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["units"][0]["content"], "fn visible() {}");
    assert_eq!(body["units"][0]["provenance"], "code-knowledge-graph");
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

#[tokio::test]
async fn generation_is_versioned_local_only_and_budgeted() {
    let (app, key, id, _, _) = http_setup();
    let request = assemble_request(&id, "backend verified content", "memory", "memory-search", 1);
    let assembled = compiled(request.candidates[0].clone(), false);
    let (status, body) = post_json(app, &key, "/v1/context/generate", serde_json::to_value(generate_request(assembled.clone(), "unknown-provider", 20)).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["abstained"], true);
    assert!(body["metadata"]["reason_codes"].as_array().unwrap().iter().any(|r| r == "provider_unavailable"));

    let (app, key, id, _, _) = http_setup();
    let budget_request = assemble_request(&id, "backend verified content", "memory", "memory-search", 1);
    let (status, body) = post_json(app, &key, "/v1/context/generate", serde_json::to_value(generate_request(compiled(budget_request.candidates[0].clone(), false), DETERMINISTIC_PROVIDER, 1)).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["abstained"], true);
    assert!(body["metadata"]["reason_codes"].as_array().unwrap().iter().any(|r| r == "budget_exceeded"));
}

#[tokio::test]
async fn generation_rejects_uncompiled_context_and_verify_fails_closed() {
    let (app, key, id, _, _) = http_setup();
    let request = assemble_request(&id, "backend verified content", "memory", "memory-search", 1);
    let (status, body) = post_json(app, &key, "/v1/context/generate", serde_json::to_value(generate_request(compiled(request.candidates[0].clone(), true), DETERMINISTIC_PROVIDER, 20)).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["abstained"], true);
    assert!(body["metadata"]["reason_codes"].as_array().unwrap().iter().any(|r| r == "context_not_compiled"));

    let (verify_app, verify_key, _, secret_id, _) = http_setup();
    let mut unauthorized = assemble_request(&secret_id, "secret backend content", "memory", "memory-search", 1).candidates[0].clone();
    unauthorized.generation = GenerationRef { id: "g1".into(), version: 1 };
    let verify = VerifyRequest { contract_version: CONTRACT_VERSION.into(), profile_id: "lab-profile".into(), profile_version: 1,
        generation: GenerationRef { id: "g1".into(), version: 1 }, model: "lab-model".into(), provider: DETERMINISTIC_PROVIDER.into(),
        assembled: compiled(unauthorized.clone(), false), output: Some(unauthorized.content.clone()),
        claims: vec![Claim { id: "c1".into(), text: unauthorized.content.clone(), unit_id: unauthorized.unit_id.clone(), locator: unauthorized.locator.clone() }] };
    let (status, body) = post_json(verify_app, &verify_key, "/v1/context/verify", serde_json::to_value(verify).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "unauthorized");
}

#[tokio::test]
async fn verify_reports_supported_and_unsupported_claims_without_evidence_leak() {
    let (app, key, id, _, _) = http_setup();
    let candidate = assemble_request(&id, "backend verified content", "memory", "memory-search", 1).candidates[0].clone();
    let base = VerifyRequest { contract_version: CONTRACT_VERSION.into(), profile_id: "lab-profile".into(), profile_version: 1,
        generation: GenerationRef { id: "g1".into(), version: 1 }, model: "lab-model".into(), provider: DETERMINISTIC_PROVIDER.into(),
        assembled: compiled(candidate.clone(), false), output: Some(candidate.content.clone()), claims: vec![Claim {
            id: "supported".into(), text: "verified".into(), unit_id: candidate.unit_id.clone(), locator: candidate.locator.clone(),
        }] };
    let (status, body) = post_json(app, &key, "/v1/context/verify", serde_json::to_value(base).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "verified");
    assert_eq!(body["claims"][0]["status"], "verified");

    let (app, key, id, _, _) = http_setup();
    let candidate = assemble_request(&id, "backend verified content", "memory", "memory-search", 1).candidates[0].clone();
    let mut unsupported = VerifyRequest { contract_version: CONTRACT_VERSION.into(), profile_id: "lab-profile".into(), profile_version: 1,
        generation: GenerationRef { id: "g1".into(), version: 1 }, model: "lab-model".into(), provider: DETERMINISTIC_PROVIDER.into(),
        assembled: compiled(candidate.clone(), false), output: Some(candidate.content.clone()), claims: vec![Claim {
            id: "unsupported".into(), text: "not in memory".into(), unit_id: candidate.unit_id.clone(), locator: candidate.locator.clone(),
        }] };
    let (status, body) = post_json(app, &key, "/v1/context/verify", serde_json::to_value(&mut unsupported).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "unsupported");
}

#[test]
fn memory_write_gate_is_atomic_and_expired_rows_are_not_retrievable() {
    let conn = connect(":memory:").unwrap();
    migrations::run(&conn).unwrap();
    let (org, user, _) = queries::bootstrap(&conn, "Gate Org", "gate-org", "gate@test", "Admin").unwrap();
    let store = SqliteStore::new(conn);
    let invalid = nexusmind::models::types::StoreMemoryRequest {
        project: None, tool: "test".into(), content: " ".into(), tags: None, title: None,
        memory_type: None, scope: Some("invalid".into()), topic_key: None, session_id: None, context_fabric_metadata: None,
    };
    assert!(store.store(&org.id, &user.id, &invalid).is_err());
    let db = store.conn();
    let conn = db.lock().unwrap();
    assert_eq!(conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get::<_, i64>(0)).unwrap(), 0);

    let memory = queries::upsert_memory(&conn, &org.id, &user.id, &nexusmind::models::types::StoreMemoryRequest {
        project: None, tool: "test".into(), content: "expires soon".into(), tags: None, title: None,
        memory_type: None, scope: None, topic_key: None, session_id: None, context_fabric_metadata: None,
    }).unwrap();
    conn.execute("UPDATE memories SET delete_after = datetime('now', '-1 day') WHERE id = ?1", [&memory.id]).unwrap();
    assert!(queries::list_memories(&conn, &org.id, None, None, None, None, None, None, 10, 0, false, None, None, None).unwrap().is_empty());
}

#[test]
fn provenance_roundtrip_is_allow_listed_untrusted_and_tenant_scoped() {
    let conn = connect(":memory:").unwrap();
    migrations::run(&conn).unwrap();
    migrations::apply_context_fabric(&conn).unwrap();
    migrations::apply_context_fabric_provenance(&conn).unwrap();
    let (org, user, _) = queries::bootstrap(&conn, "Provenance Org", "provenance-org", "p@test", "Admin").unwrap();
    let metadata = ContextFabricMetadata {
        schema_version: 3,
        source_type: "memory-search".into(),
        source_id: "source-1".into(),
        source_version: Some("7".into()),
        profile: Some("baseline".into()),
        generation: Some("generation-1:2".into()),
        snapshot: Some("sha256:snapshot".into()),
        freshness: Some("fresh".into()),
        observed_at: Some("2026-08-02T00:00:00Z".into()),
        trust: Some("backend-evidence".into()),
        locator: Some("memory:source-1".into()),
        sensitivity: "internal".into(),
        trusted: true,
        verified: true,
    };
    let memory = queries::upsert_memory(&conn, &org.id, &user.id, &nexusmind::models::types::StoreMemoryRequest {
        project: None, tool: "claude".into(), content: "provenance content".into(), tags: None,
        title: None, memory_type: None, scope: None, topic_key: None, session_id: None,
        context_fabric_metadata: Some(metadata),
    }).unwrap();
    assert_eq!(memory.context_fabric_metadata.as_ref().unwrap().source_id, "source-1");
    assert!(!memory.context_fabric_metadata.as_ref().unwrap().trusted);
    assert!(!memory.context_fabric_metadata.as_ref().unwrap().verified);
    let raw: String = conn.query_row("SELECT context_fabric_metadata FROM memories WHERE id = ?1", [&memory.id], |row| row.get(0)).unwrap();
    assert!(!raw.contains("provenance content"));
    assert!(queries::get_memory_by_id_for_org(&conn, "wrong-org", &memory.id).unwrap().is_none());

    let invalid = ContextFabricMetadata {
        schema_version: 2, source_type: "memory-search".into(), source_id: "source-1".into(),
        source_version: None, profile: None, generation: None, snapshot: None, freshness: None,
        observed_at: None, trust: None, locator: None, sensitivity: "internal".into(),
        trusted: false, verified: false,
    };
    let before: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0)).unwrap();
    let result = queries::upsert_memory(&conn, &org.id, &user.id, &nexusmind::models::types::StoreMemoryRequest {
        project: None, tool: "claude".into(), content: "must not persist".into(), tags: None,
        title: None, memory_type: None, scope: None, topic_key: None, session_id: None,
        context_fabric_metadata: Some(invalid),
    });
    assert!(result.is_err());
    let after: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0)).unwrap();
    assert_eq!(before, after);
}

#[test]
fn archive_restore_lifecycle_revalidates_visibility_and_retention() {
    let conn = connect(":memory:").unwrap();
    migrations::run(&conn).unwrap();
    let (org, user, _) = queries::bootstrap(&conn, "Lifecycle Org", "lifecycle-org", "life@test", "Admin").unwrap();
    let req = nexusmind::models::types::StoreMemoryRequest {
        project: None, tool: "test".into(), content: "lifecycle content".into(), tags: None, title: None,
        memory_type: None, scope: None, topic_key: None, session_id: None, context_fabric_metadata: None,
    };
    let memory = queries::upsert_memory(&conn, &org.id, &user.id, &req).unwrap();
    assert!(queries::archive_memory(&conn, &org.id, &memory.id).unwrap());
    assert!(queries::list_memories(&conn, &org.id, None, None, None, None, None, None, 10, 0, false, None, None, None).unwrap().is_empty());
    assert!(queries::restore_memory(&conn, &org.id, &memory.id).unwrap());
    assert!(!queries::list_memories(&conn, &org.id, None, None, None, None, None, None, 10, 0, false, None, None, None).unwrap().is_empty());
    conn.execute("UPDATE memories SET archived_at = datetime('now'), delete_after = datetime('now', '-1 day') WHERE id = ?1", [&memory.id]).unwrap();
    assert!(!queries::restore_memory(&conn, &org.id, &memory.id).unwrap());
}
