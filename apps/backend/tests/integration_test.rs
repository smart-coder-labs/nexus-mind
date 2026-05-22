use nexusmind::auth::api_keys;
use nexusmind::db::{connection, migrations, queries};
use rusqlite::Connection;
use uuid::Uuid;

fn setup() -> Connection {
    let conn = connection::connect(":memory:").unwrap();
    migrations::run(&conn).unwrap();
    conn
}

/// Insert a second org directly (bootstrap only allows one org per DB).
fn insert_org(conn: &Connection, name: &str, slug: &str) -> String {
    let org_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO organizations (id, name, slug) VALUES (?1, ?2, ?3)",
        rusqlite::params![org_id, name, slug],
    )
    .unwrap();
    org_id
}

/// Insert a user directly, returns (user_id, raw_key).
fn insert_user(conn: &Connection, org_id: &str, email: &str, role: &str) -> (String, String) {
    let user_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO users (id, org_id, email, name, role, status, created_at)
         VALUES (?1, ?2, ?3, 'Test User', ?4, 'active', datetime('now'))",
        rusqlite::params![user_id, org_id, email, role],
    )
    .unwrap();

    let key_id = Uuid::new_v4().to_string();
    let (raw_key, key_hash) = api_keys::generate();
    conn.execute(
        "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
         VALUES (?1, ?2, ?3, ?4, 'default', datetime('now'))",
        rusqlite::params![key_id, user_id, org_id, key_hash],
    )
    .unwrap();

    (user_id, raw_key)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn bootstrap_creates_org_and_returns_key() {
    let conn = setup();
    let (org, user, raw_key) = queries::bootstrap(&conn, "Acme Corp", "acme", "admin@acme.com", "Admin User").unwrap();

    assert_eq!(org.name, "Acme Corp");
    assert_eq!(org.slug, "acme");
    assert_eq!(user.email, "admin@acme.com");
    assert_eq!(user.role, "admin");
    assert!(raw_key.starts_with("nm_"));

    let hash = api_keys::hash_key(&raw_key);
    let ctx = queries::validate_api_key(&conn, &hash).unwrap();
    assert!(ctx.is_some(), "returned key must be immediately valid");

    let ctx = ctx.unwrap();
    assert_eq!(ctx.org_id, org.id);
    assert_eq!(ctx.user_id, user.id);
    assert_eq!(ctx.role, "admin");
}

#[test]
fn invite_user_key_works_immediately() {
    let conn = setup();
    let (org, _admin, _) = queries::bootstrap(&conn, "Acme Corp", "acme", "admin@acme.com", "Admin").unwrap();

    let (member, raw_key) = queries::invite_user(&conn, &org.id, "dev@acme.com", "Dev User", "member").unwrap();
    assert_eq!(member.role, "member");

    let hash = api_keys::hash_key(&raw_key);
    let ctx = queries::validate_api_key(&conn, &hash).unwrap();
    assert!(ctx.is_some(), "invited user key must be immediately valid");

    let ctx = ctx.unwrap();
    assert_eq!(ctx.org_id, org.id, "org_id must match");
    assert_eq!(ctx.user_id, member.id, "user_id must match");
    assert_eq!(ctx.role, "member");
}

#[test]
fn store_memory_org_isolation() {
    let conn = setup();

    // Org 1 via bootstrap.
    let (org1, user1, _) = queries::bootstrap(&conn, "Org One", "org1", "admin@org1.com", "Admin1").unwrap();

    // Org 2 inserted directly.
    let org2_id = insert_org(&conn, "Org Two", "org2");
    let (user2_id, _) = insert_user(&conn, &org2_id, "admin@org2.com", "admin");

    // Store a memory in org1 only.
    let tags: Vec<String> = vec!["test".to_string()];
    queries::store_memory(&conn, &org1.id, &user1.id, "proj", "claude-code", "org1 secret content", &tags).unwrap();

    // Org2 must see nothing.
    let org2_memories = queries::list_memories(&conn, &org2_id, None, None, None, None, None, 10, 0).unwrap();
    assert!(org2_memories.is_empty(), "org2 must not see org1 memories");

    // Org1 must see its own memory.
    let org1_memories = queries::list_memories(&conn, &org1.id, None, None, None, None, None, 10, 0).unwrap();
    assert_eq!(org1_memories.len(), 1);
    assert_eq!(org1_memories[0].content, "org1 secret content");

    let _ = user2_id; // suppress unused warning
}

#[test]
fn search_memory_org_isolation() {
    let conn = setup();

    // Org 1 via bootstrap.
    let (org1, user1, _) = queries::bootstrap(&conn, "Alpha", "alpha", "admin@alpha.com", "Admin").unwrap();

    // Org 2 inserted directly.
    let org2_id = insert_org(&conn, "Beta", "beta");
    let (_user2_id, _) = insert_user(&conn, &org2_id, "admin@beta.com", "admin");

    // Store "authentication oauth" in org1 only.
    queries::store_memory(&conn, &org1.id, &user1.id, "proj", "cursor", "authentication oauth flow", &[]).unwrap();

    // Org2 searching must get nothing.
    let org2_results = queries::search_memories(&conn, &org2_id, "authentication", 10).unwrap();
    assert!(org2_results.is_empty(), "org2 search must not return org1 memories");

    // Org1 searching must find it.
    let org1_results = queries::search_memories(&conn, &org1.id, "authentication", 10).unwrap();
    assert_eq!(org1_results.len(), 1);
    assert!(org1_results[0].content.contains("authentication"));
}

#[test]
fn audit_log_captures_events() {
    let conn = setup();
    let (org, user, _) = queries::bootstrap(&conn, "Acme Corp", "acme", "admin@acme.com", "Admin").unwrap();

    queries::log_audit(&conn, &org.id, &user.id, "store", "memory", None, serde_json::json!({})).unwrap();

    let entries = queries::list_audit(&conn, &org.id, None, None, None, None, None, 50, 0).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].org_id, org.id);
    assert_eq!(entries[0].action, "store");
    assert_eq!(entries[0].resource_type, "memory");
}

#[test]
fn suspend_user_revokes_key() {
    let conn = setup();
    let (org, _admin, _) = queries::bootstrap(&conn, "Acme Corp", "acme", "admin@acme.com", "Admin").unwrap();

    let (member, raw_key) = queries::invite_user(&conn, &org.id, "dev@acme.com", "Dev", "member").unwrap();
    let hash = api_keys::hash_key(&raw_key);

    // Key is valid before suspension.
    let ctx = queries::validate_api_key(&conn, &hash).unwrap();
    assert!(ctx.is_some(), "key must be valid before suspension");

    // Suspend the user.
    let suspended = queries::suspend_user(&conn, &org.id, &member.id).unwrap();
    assert!(suspended, "suspend_user must return true");

    // Key must be invalid after suspension.
    let ctx = queries::validate_api_key(&conn, &hash).unwrap();
    assert!(ctx.is_none(), "key must be revoked after suspension");
}

#[test]
fn rotate_key_invalidates_old_key() {
    let conn = setup();
    let (org, admin, old_raw_key) = queries::bootstrap(&conn, "Acme Corp", "acme", "admin@acme.com", "Admin").unwrap();
    let old_hash = api_keys::hash_key(&old_raw_key);

    // Old key is valid before rotation.
    assert!(
        queries::validate_api_key(&conn, &old_hash).unwrap().is_some(),
        "old key must be valid before rotation"
    );

    let new_raw_key = queries::rotate_key(&conn, &org.id, &admin.id).unwrap();
    let new_hash = api_keys::hash_key(&new_raw_key);

    // Old key must be invalid.
    let old_ctx = queries::validate_api_key(&conn, &old_hash).unwrap();
    assert!(old_ctx.is_none(), "old key must be revoked after rotation");

    // New key must be valid.
    let new_ctx = queries::validate_api_key(&conn, &new_hash).unwrap();
    assert!(new_ctx.is_some(), "new key must be valid after rotation");
    assert_eq!(new_ctx.unwrap().user_id, admin.id);
}

// ── Schema v2 integration tests ───────────────────────────────────────────────

/// 4.1 — Legacy request (only content + tool) succeeds, scope defaults to "project", type is null.
#[test]
fn legacy_request_succeeds_with_defaults() {
    let conn = setup();
    let (org, user, _) = queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

    let req = nexusmind::models::types::StoreMemoryRequest {
        project: None,
        tool: "claude".into(),
        content: "legacy content".into(),
        tags: None,
        title: None,
        memory_type: None,
        scope: None,
        topic_key: None,
        session_id: None,
    };
    let mem = queries::upsert_memory(&conn, &org.id, &user.id, &req).unwrap();

    assert_eq!(mem.scope, "project", "legacy request must default scope to 'project'");
    assert!(mem.memory_type.is_none(), "legacy request must have null type");
    assert_eq!(mem.revision_count, 1);
    assert!(mem.normalized_hash.is_some(), "hash must be computed even for legacy requests");
}

/// 4.2 — Full v2 request: all new fields present, all persisted and returned.
#[test]
fn full_v2_request_persists_all_fields() {
    let conn = setup();
    let (org, user, _) = queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

    let req = nexusmind::models::types::StoreMemoryRequest {
        project: Some("nexusmind".into()),
        tool: "claude".into(),
        content: "v2 content".into(),
        tags: Some(vec!["rust".into()]),
        title: Some("V2 Memory".into()),
        memory_type: Some("architecture".into()),
        scope: Some("personal".into()),
        topic_key: Some("arch/v2-test".into()),
        session_id: None,
    };
    let mem = queries::upsert_memory(&conn, &org.id, &user.id, &req).unwrap();

    assert_eq!(mem.title.as_deref(), Some("V2 Memory"));
    assert_eq!(mem.memory_type.as_deref(), Some("architecture"));
    assert_eq!(mem.scope, "personal");
    assert_eq!(mem.topic_key.as_deref(), Some("arch/v2-test"));
    assert_eq!(mem.revision_count, 1);
    assert!(mem.normalized_hash.is_some());
}

/// 4.3 — Upsert on topic_key: second store returns updated content, revision_count = 2.
#[test]
fn upsert_on_topic_key_increments_revision() {
    let conn = setup();
    let (org, user, _) = queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

    let req1 = nexusmind::models::types::StoreMemoryRequest {
        project: Some("proj".into()),
        tool: "claude".into(),
        content: "first version".into(),
        tags: None,
        title: None,
        memory_type: None,
        scope: None,
        topic_key: Some("topic/test".into()),
        session_id: None,
    };
    let mem1 = queries::upsert_memory(&conn, &org.id, &user.id, &req1).unwrap();
    assert_eq!(mem1.revision_count, 1);

    let req2 = nexusmind::models::types::StoreMemoryRequest {
        project: Some("proj".into()),
        tool: "claude".into(),
        content: "second version".into(),
        tags: None,
        title: None,
        memory_type: None,
        scope: None,
        topic_key: Some("topic/test".into()),
        session_id: None,
    };
    let mem2 = queries::upsert_memory(&conn, &org.id, &user.id, &req2).unwrap();
    assert_eq!(mem2.revision_count, 2, "second store must increment revision_count to 2");
    assert_eq!(mem2.content, "second version");
    assert_eq!(mem2.id, mem1.id, "upsert must reuse the same row");
}

/// 4.4 — Migration idempotency: run migrations twice, no error, schema unchanged.
#[test]
fn migration_idempotency() {
    let conn = connection::connect(":memory:").unwrap();
    migrations::run_all(&conn).unwrap();
    // Run again — must not fail
    let result = migrations::run_all(&conn);
    assert!(result.is_ok(), "run_all must be idempotent: {:?}", result.err());

    // Verify user_version stays at 3
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version, 3);
}

/// 4.5 — FTS backfill: pre-existing rows are searchable after migration v2.
#[test]
fn fts_backfill_after_migration() {
    // Start with a v1-only DB
    let conn = connection::connect(":memory:").unwrap();
    migrations::run_v1(&conn).unwrap();

    // Insert a memory before v2 migration runs
    conn.execute(
        "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO users (id, org_id, email, name, role) VALUES ('u1', 'org1', 'a@b.com', 'A', 'admin')",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO memories (id, org_id, user_id, tool, content) VALUES ('m1', 'org1', 'u1', 'claude', 'pre-migration authentication content')",
        [],
    ).unwrap();

    // Now run v2 migration (backfill must include the pre-existing row)
    migrations::run_v2(&conn).unwrap();

    // The pre-existing row must be searchable
    let results = queries::search_memories(&conn, "org1", "authentication", 10).unwrap();
    assert_eq!(results.len(), 1, "pre-existing rows must be indexed after FTS backfill");
    assert!(results[0].content.contains("authentication"));
}
