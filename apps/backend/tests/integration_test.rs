use nexusmind::auth::api_keys;
use nexusmind::db::{connection, migrations, queries};
use nexusmind::models::types::{Memory, Role, StoreMemoryRequest, UserRole};
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

/// Minimal helper that reproduces the old `store_memory` API via `upsert_memory`.
/// `upsert_memory` with `topic_key: None` always INSERTs, so behavior is identical.
fn legacy_store(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    project: &str,
    tool: &str,
    content: &str,
    tags: &[String],
) -> Memory {
    // Implicit project creation is disabled in upsert_memory; ensure the project exists
    // first (test scaffolding — production requires an admin to create it).
    if project != "default" {
        queries::get_or_create_project(conn, org_id, project).unwrap();
    }
    let req = StoreMemoryRequest {
        project: Some(project.to_string()),
        tool: tool.to_string(),
        content: content.to_string(),
        tags: Some(tags.to_vec()),
        title: None,
        memory_type: None,
        scope: None,
        topic_key: None,
        session_id: None,
    };
    queries::upsert_memory(conn, org_id, user_id, &req).unwrap()
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
    assert_eq!(ctx.role, UserRole::Standard(Role::Admin));
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
    assert_eq!(ctx.role, UserRole::Standard(Role::Member));
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
    legacy_store(&conn, &org1.id, &user1.id, "proj", "claude-code", "org1 secret content", &tags);

    // Org2 must see nothing.
    let org2_memories = queries::list_memories(&conn, &org2_id, None, None, None, None, None, None, 10, 0, false, None, None, None).unwrap();
    assert!(org2_memories.is_empty(), "org2 must not see org1 memories");

    // Org1 must see its own memory.
    let org1_memories = queries::list_memories(&conn, &org1.id, None, None, None, None, None, None, 10, 0, false, None, None, None).unwrap();
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
    legacy_store(&conn, &org1.id, &user1.id, "proj", "cursor", "authentication oauth flow", &[]);

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

    let entries = queries::list_audit(&conn, &org.id, None, None, None, None, None, None, 50, 0).unwrap();
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
        project: None,
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
        project: None,
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
        project: None,
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

    // Verify user_version is the current max (56 after the knowledge migration)
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version, 56);
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

    // Now run remaining migrations (backfill must include the pre-existing row)
    migrations::run_v2(&conn).unwrap();
    migrations::run_v3(&conn).unwrap();
    migrations::run_v4(&conn).unwrap();
    migrations::run_v5(&conn).unwrap();
    migrations::run_v6(&conn).unwrap();
    // Run remaining migrations so all columns (including archived_at from v17) exist
    migrations::run_all(&conn).unwrap();

    // The pre-existing row must be searchable
    let results = queries::search_memories(&conn, "org1", "authentication", 10).unwrap();
    assert_eq!(results.len(), 1, "pre-existing rows must be indexed after FTS backfill");
    assert!(results[0].content.contains("authentication"));
}

#[test]
fn custom_roles_and_assignment() {
    let conn = setup();
    let (org, _user, _) = queries::bootstrap(&conn, "Acme Corp", "acme", "admin@acme.com", "Admin").unwrap();

    // 1. Check template roles exist
    let roles = queries::list_roles(&conn, &org.id).unwrap();
    assert!(roles.iter().any(|r| r.name == "security-officer" && r.is_template));
    assert!(roles.iter().any(|r| r.name == "dev-senior" && r.is_template));

    // 2. Create custom role
    let custom = queries::create_role(
        &conn,
        &org.id,
        "custom-editor",
        "Custom Editor",
        &["memory:read".to_string(), "memory:write".to_string()],
        Some("Allows editing memories"),
    ).unwrap();
    assert_eq!(custom.name, "custom-editor");
    assert!(!custom.is_template);

    // 3. Resolve permissions
    let perms = queries::get_role_permissions(&conn, &org.id, "custom-editor").unwrap();
    assert_eq!(perms.len(), 2);
    assert!(perms.contains(&"memory:read".to_string()));
    assert!(perms.contains(&"memory:write".to_string()));

    // 4. Update user role to custom-editor
    let (invited_user, _) = queries::invite_user(&conn, &org.id, "dev@acme.com", "Dev", "member").unwrap();
    let updated = queries::update_user_role(&conn, &org.id, &invited_user.id, "custom-editor").unwrap();
    assert!(updated);

    // Fetch user to confirm role
    let users = queries::list_users(&conn, &org.id).unwrap();
    let updated_user = users.iter().find(|u| u.id == invited_user.id).unwrap();
    assert_eq!(updated_user.role, "custom-editor");
}

#[test]
fn project_role_overrides_integration() {
    let conn = setup();
    let (org, _admin, _) = queries::bootstrap(&conn, "Acme Corp", "acme", "admin@acme.com", "Admin").unwrap();

    // 1. Create a project
    let p_id = queries::get_or_create_project(&conn, &org.id, "payments").unwrap();
    assert!(!p_id.is_empty());

    // Verify it is in list_projects (alongside the auto-created default "nexus-mind" project).
    let projects = queries::list_projects(&conn, &org.id).unwrap();
    assert!(projects.iter().any(|p| p.name == "payments"), "payments project must be listed");
    assert!(projects.iter().any(|p| p.name == queries::DEFAULT_PROJECT_NAME), "default project must exist");

    // 2. Invite a viewer user
    let (dev, dev_key) = queries::invite_user(&conn, &org.id, "dev@acme.com", "Dev", "viewer").unwrap();
    let dev_hash = api_keys::hash_key(&dev_key);
    let dev_ctx = queries::validate_api_key(&conn, &dev_hash).unwrap().unwrap();
    assert_eq!(dev_ctx.role, UserRole::Standard(Role::Viewer));

    // 3. Dev attempts to store memory in "payments" project -> should fail permissions check
    assert!(nexusmind::api::helpers::require_permission(&conn, &dev_ctx, Some("payments"), "memory:write").is_err());

    // 4. Override Dev's role in project "payments" to "dev-senior"
    queries::upsert_project_member(&conn, &p_id, &dev.id, "dev-senior").unwrap();

    // Verify member list — admin was seeded when the project was created,
    // so there is at least one additional member alongside dev.
    let members = queries::list_project_members(&conn, &org.id, &p_id).unwrap();
    let dev_member = members.iter().find(|m| m.user_id == dev.id).expect("dev should be a member");
    assert_eq!(dev_member.role, "dev-senior");

    // 5. Dev attempts to store memory in "payments" project -> should now SUCCEED permissions check
    assert!(nexusmind::api::helpers::require_permission(&conn, &dev_ctx, Some("payments"), "memory:write").is_ok());

    // But still fails in "other-project"
    assert!(nexusmind::api::helpers::require_permission(&conn, &dev_ctx, Some("other-project"), "memory:write").is_err());

    // 6. Detach / Remove override
    let deleted = queries::delete_project_member(&conn, &p_id, &dev.id).unwrap();
    assert!(deleted);
    let members_after = queries::list_project_members(&conn, &org.id, &p_id).unwrap();
    assert!(!members_after.iter().any(|m| m.user_id == dev.id), "dev should no longer be a member");

    // Dev fails permissions check again
    assert!(nexusmind::api::helpers::require_permission(&conn, &dev_ctx, Some("payments"), "memory:write").is_err());
}

/// v32 — update_user_note: patch sets note, list_users returns it.
#[test]
fn update_user_note_persists_and_list_users_returns_it() {
    let conn = setup();
    let (org, admin, _) = queries::bootstrap(&conn, "Acme Corp", "acme", "admin@acme.com", "Admin").unwrap();

    // Invite a regular user
    let (member, _) = queries::invite_user(&conn, &org.id, "vip@acme.com", "VIP User", "member").unwrap();

    // Initially no note
    let users = queries::list_users(&conn, &org.id).unwrap();
    let u = users.iter().find(|u| u.id == member.id).unwrap();
    assert!(u.admin_note.is_none(), "admin_note must be NULL initially");

    // Set note
    let found = queries::update_user_admin_note(&conn, &org.id, &member.id, Some("VIP user")).unwrap();
    assert!(found, "update_user_admin_note must return true for existing user");

    // List again — note must appear
    let users_after = queries::list_users(&conn, &org.id).unwrap();
    let u2 = users_after.iter().find(|u| u.id == member.id).unwrap();
    assert_eq!(u2.admin_note.as_deref(), Some("VIP user"), "admin_note must be returned by list_users");

    // Clear note by passing None
    let found2 = queries::update_user_admin_note(&conn, &org.id, &member.id, None).unwrap();
    assert!(found2);
    let users_cleared = queries::list_users(&conn, &org.id).unwrap();
    let u3 = users_cleared.iter().find(|u| u.id == member.id).unwrap();
    assert!(u3.admin_note.is_none(), "admin_note must be NULL after clearing");

    // Admin user is present too (not the member we just changed)
    assert!(users_after.iter().any(|u| u.id == admin.id));
}

/// v34 — exclude_patterns: create project, PATCH exclude_patterns, GET project and verify patterns returned.
#[test]
fn code_project_exclude_patterns_roundtrip() {
    let conn = setup();

    // Bootstrap org
    let (org, _admin, _key) = queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

    // Create code project
    let project_id = queries::upsert_code_project(&conn, &org.id, "myapp", "/ws/myapp").unwrap();

    // Initially exclude_patterns must be empty
    let project = queries::get_code_project_by_id(&conn, &org.id, project_id)
        .unwrap()
        .expect("project must exist");
    assert!(project.exclude_patterns.is_empty(), "exclude_patterns must start empty");

    // PATCH exclude_patterns
    let patterns = vec!["*.lock".to_string(), "node_modules/*".to_string()];
    let updated = queries::update_code_project_exclude_patterns(&conn, &org.id, project_id, &patterns).unwrap();
    assert!(updated, "update must return true for existing project");

    // GET project and verify patterns
    let project_after = queries::get_code_project_by_id(&conn, &org.id, project_id)
        .unwrap()
        .expect("project must still exist");
    assert_eq!(project_after.exclude_patterns, patterns, "exclude_patterns must be persisted and returned");

    // Also verify via list
    let projects = queries::list_code_projects(&conn, &org.id).unwrap();
    let listed = projects.iter().find(|p| p.name == "myapp").expect("myapp must appear in list");
    assert_eq!(listed.exclude_patterns, patterns, "list_code_projects must include exclude_patterns");
}

/// Regression: `delete_code_project` must delete by the numeric `id` column, not by
/// matching the `name` column. Deleting one project must leave a sibling project
/// (with an unrelated name) untouched.
#[test]
fn delete_code_project_by_id_only_removes_target() {
    let conn = setup();
    let (org, _admin, _key) = queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

    let id_alpha = queries::upsert_code_project(&conn, &org.id, "alpha", "/ws/alpha").unwrap();
    let id_beta = queries::upsert_code_project(&conn, &org.id, "beta", "/ws/beta").unwrap();

    let deleted = queries::delete_code_project(&conn, &org.id, id_alpha).unwrap();
    assert!(deleted, "delete_code_project must return true for an existing project id");

    let remaining = queries::list_code_projects(&conn, &org.id).unwrap();
    assert!(
        remaining.iter().all(|p| p.id != id_alpha.to_string()),
        "deleted project (alpha) must be gone: {remaining:?}"
    );
    assert!(
        remaining.iter().any(|p| p.id == id_beta.to_string() && p.name == "beta"),
        "sibling project (beta) must be untouched: {remaining:?}"
    );
}

/// Regression (case-sensitive gotcha): reproduces the exact mechanism of the old
/// name-based bug. If a *different* project happens to be named the same as the
/// target project's numeric id (a plausible collision, e.g. an org with a project
/// literally named "7"), the old `WHERE name = ?` query would delete that unrelated
/// decoy project instead of the one the caller actually asked for by id.
#[test]
fn delete_code_project_id_based_deletion_avoids_name_collision_bug() {
    let conn = setup();
    let (org, _admin, _key) = queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

    let id_target = queries::upsert_code_project(&conn, &org.id, "target-app", "/ws/target").unwrap();
    // Decoy project whose NAME equals the target project's id as a string — this is
    // exactly what the old buggy query (`WHERE name = ?`) would have matched when
    // called with the target's id.
    let decoy_name = id_target.to_string();
    let id_decoy = queries::upsert_code_project(&conn, &org.id, &decoy_name, "/ws/decoy").unwrap();

    let deleted = queries::delete_code_project(&conn, &org.id, id_target).unwrap();
    assert!(deleted, "delete_code_project must return true for the target project id");

    let remaining = queries::list_code_projects(&conn, &org.id).unwrap();
    assert!(
        remaining.iter().all(|p| p.id != id_target.to_string()),
        "target project must be deleted: {remaining:?}"
    );
    assert!(
        remaining.iter().any(|p| p.id == id_decoy.to_string()),
        "decoy project (name == target's id) must survive — the old bug would have deleted it instead: {remaining:?}"
    );
}

#[test]
fn update_org_settings_persists_announcement() {
    use nexusmind::models::types::OrgSettings;
    let conn = setup();
    let (org, _, _) = queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

    // Set announcement via update_org_settings (min_password_length is NOT NULL DEFAULT 8)
    let input = OrgSettings {
        min_password_length: Some(8),
        announcement: Some("Maintenance tonight".to_string()),
        announcement_type: Some("warning".to_string()),
        ..Default::default()
    };
    let result = queries::update_org_settings(&conn, &org.id, &input).unwrap();
    assert_eq!(result.announcement.as_deref(), Some("Maintenance tonight"), "announcement must be persisted");
    assert_eq!(result.announcement_type.as_deref(), Some("warning"), "announcement_type must be persisted");

    // Verify GET reflects the saved announcement
    let fetched = queries::get_org_settings(&conn, &org.id).unwrap();
    assert_eq!(fetched.announcement.as_deref(), Some("Maintenance tonight"), "GET must return persisted announcement");

    // Clear announcement by passing empty string (min_password_length must remain non-null)
    let clear_input = OrgSettings {
        min_password_length: Some(8),
        announcement: Some(String::new()),
        ..Default::default()
    };
    let cleared = queries::update_org_settings(&conn, &org.id, &clear_input).unwrap();
    assert!(cleared.announcement.is_none(), "empty string must clear announcement to NULL");
}

// ── over-enrolled projects diagnostic tests ───────────────────────────────────

/// When every active user in the org is a member of a project, that project must
/// appear in the over-enrolled list.
#[test]
fn over_enrolled_project_appears_when_all_users_are_members() {
    let conn = setup();
    let (org, admin, _) = queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

    // Invite one more user so we have 2 active users total.
    let (member, _) = queries::invite_user(&conn, &org.id, "dev@acme.com", "Dev", "member").unwrap();

    // Use create_project (no auto-enroll), then manually add both users.
    let project = queries::create_project(&conn, &org.id, "all-hands", None, None).unwrap();
    queries::upsert_project_member(&conn, &project.id, &admin.id, "admin").unwrap();
    queries::upsert_project_member(&conn, &project.id, &member.id, "member").unwrap();

    let results = queries::list_over_enrolled_projects(&conn, &org.id).unwrap();
    let found = results.iter().find(|p| p.project_name == "all-hands");
    assert!(found.is_some(), "all-hands project must appear in over-enrolled list");
    let entry = found.unwrap();
    assert_eq!(entry.member_count, 2, "member_count must be 2");
    assert_eq!(entry.active_user_count, 2, "active_user_count must be 2");
}

/// A project with only a subset of users enrolled must NOT appear in the list.
#[test]
fn partial_enrollment_not_over_enrolled() {
    let conn = setup();
    let (org, admin, _) = queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

    // Two extra users so there are 3 active users total.
    let (member1, _) = queries::invite_user(&conn, &org.id, "dev1@acme.com", "Dev1", "member").unwrap();
    let (_member2, _) = queries::invite_user(&conn, &org.id, "dev2@acme.com", "Dev2", "member").unwrap();

    // Use create_project (no auto-enroll), then manually add only 2 of 3 users.
    let project = queries::create_project(&conn, &org.id, "partial-project", None, None).unwrap();
    queries::upsert_project_member(&conn, &project.id, &admin.id, "admin").unwrap();
    queries::upsert_project_member(&conn, &project.id, &member1.id, "member").unwrap();

    let results = queries::list_over_enrolled_projects(&conn, &org.id).unwrap();
    let found = results.iter().find(|p| p.project_name == "partial-project");
    assert!(
        found.is_none(),
        "partial-project must NOT appear in over-enrolled list (only 2/3 users enrolled)"
    );
}

/// Suspended users are excluded from active_user_count, so a project with all
/// active (non-suspended) users enrolled is still flagged even when a suspended
/// user is not a member.
#[test]
fn suspended_users_excluded_from_active_count() {
    let conn = setup();
    let (org, admin, _) = queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

    let (active_member, _) = queries::invite_user(&conn, &org.id, "active@acme.com", "Active", "member").unwrap();
    let (suspended_member, _) = queries::invite_user(&conn, &org.id, "suspended@acme.com", "Suspended", "member").unwrap();

    // Suspend one user — they must not count toward active_user_count.
    queries::suspend_user(&conn, &org.id, &suspended_member.id).unwrap();

    // Use create_project (no auto-enroll), then manually enroll only the two active users.
    let project = queries::create_project(&conn, &org.id, "active-only", None, None).unwrap();
    queries::upsert_project_member(&conn, &project.id, &admin.id, "admin").unwrap();
    queries::upsert_project_member(&conn, &project.id, &active_member.id, "member").unwrap();

    let results = queries::list_over_enrolled_projects(&conn, &org.id).unwrap();
    let found = results.iter().find(|p| p.project_name == "active-only");
    assert!(
        found.is_some(),
        "active-only project must appear — all 2 active users are enrolled (suspended user is excluded)"
    );
    let entry = found.unwrap();
    assert_eq!(entry.active_user_count, 2, "active_user_count must exclude the suspended user");
}

/// parent_id update — happy path, cycle rejection, cross-org rejection.
#[test]
fn update_project_parent_id_validation() {
    let conn = setup();
    let (org, _admin, _) = queries::bootstrap(&conn, "Acme Corp", "acme", "admin@acme.com", "Admin").unwrap();

    // Create three projects in org: A, B, C (A → B → C chain)
    let a = queries::create_project(&conn, &org.id, "proj-a", None, None).unwrap();
    let b = queries::create_project(&conn, &org.id, "proj-b", None, None).unwrap();
    let c = queries::create_project(&conn, &org.id, "proj-c", None, None).unwrap();

    // Happy path: set B's parent to A
    let updated = queries::update_project(&conn, &org.id, &b.id, Some(&a.id)).unwrap();
    assert!(updated, "setting parent should return true");

    // Happy path: set C's parent to B (creating chain A → B → C)
    let updated2 = queries::update_project(&conn, &org.id, &c.id, Some(&b.id)).unwrap();
    assert!(updated2);

    // Happy path: clear parent (set to None)
    let cleared = queries::update_project(&conn, &org.id, &c.id, None).unwrap();
    assert!(cleared);

    // Re-establish A → B → C chain for cycle tests
    queries::update_project(&conn, &org.id, &c.id, Some(&b.id)).unwrap();

    // Cycle: try to set A's parent to C (would create C → A → B → C cycle)
    let cycle_err = queries::update_project(&conn, &org.id, &a.id, Some(&c.id));
    assert!(cycle_err.is_err(), "cycle must be rejected");
    assert!(cycle_err.unwrap_err().to_string().contains("cycle_detected"));

    // Self-parenting: A cannot be its own parent
    let self_err = queries::update_project(&conn, &org.id, &a.id, Some(&a.id));
    assert!(self_err.is_err(), "self-parenting must be rejected");
    assert!(self_err.unwrap_err().to_string().contains("cycle_detected"));

    // Cross-org: set up second org and try to use its project as parent
    let org2_id = conn.query_row(
        "INSERT INTO organizations (id, name, slug) VALUES (?, 'Org2', 'org2') RETURNING id",
        [uuid::Uuid::new_v4().to_string()],
        |row| row.get::<_, String>(0),
    ).unwrap();
    let other_org_project = queries::create_project(&conn, &org2_id, "other-org-proj", None, None).unwrap();
    let cross_org_err = queries::update_project(&conn, &org.id, &a.id, Some(&other_org_project.id));
    assert!(cross_org_err.is_err(), "cross-org parent must be rejected");
    assert!(cross_org_err.unwrap_err().to_string().contains("not_found"));
}

/// A pre-existing cycle in the data (created by bypassing validation) must not
/// hang update_project — the ancestor walk must terminate.
#[test]
fn update_project_terminates_on_pre_existing_cycle() {
    let conn = setup();
    let (org, _admin, _) = queries::bootstrap(&conn, "Acme Corp", "acme", "admin@acme.com", "Admin").unwrap();

    let a = queries::create_project(&conn, &org.id, "cyc-a", None, None).unwrap();
    let b = queries::create_project(&conn, &org.id, "cyc-b", None, None).unwrap();
    let c = queries::create_project(&conn, &org.id, "cyc-c", None, None).unwrap();

    // Manually create a pre-existing cycle A → B → A via raw SQL, bypassing validation.
    conn.execute(
        "UPDATE projects SET parent_id = ?1 WHERE id = ?2",
        rusqlite::params![b.id, a.id],
    )
    .unwrap();
    conn.execute(
        "UPDATE projects SET parent_id = ?1 WHERE id = ?2",
        rusqlite::params![a.id, b.id],
    )
    .unwrap();

    // Pointing C at a member of the cyclic pair must terminate (Ok or Err — no hang).
    let result = queries::update_project(&conn, &org.id, &c.id, Some(&a.id));
    // The important assertion is that we got here at all; either outcome is acceptable.
    let _ = result;
}
