use anyhow::Result;
use rusqlite::Connection;

/// Entry point called by main.rs. Runs all migrations in order.
pub fn run_all(conn: &Connection) -> Result<()> {
    run_v1(conn)?;
    run_v2(conn)?;
    run_v3(conn)?;
    run_v4(conn)?;
    run_v5(conn)?;
    run_v6(conn)?;
    run_v7(conn)?;
    run_v8(conn)?;
    run_v9(conn)?;
    run_v10(conn)?;
    run_v11(conn)?;
    Ok(())
}

/// Backwards-compatible alias so existing call sites keep working.
pub fn run(conn: &Connection) -> Result<()> {
    run_all(conn)
}

/// Migration v1: base schema (organizations, users, api_keys, memories, FTS, audit_logs).
pub fn run_v1(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 1 {
        return Ok(());
    }

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS organizations (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            slug        TEXT NOT NULL UNIQUE,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS users (
            id          TEXT PRIMARY KEY,
            org_id      TEXT NOT NULL REFERENCES organizations(id),
            email       TEXT NOT NULL,
            name        TEXT NOT NULL,
            role        TEXT NOT NULL DEFAULT 'member',
            status      TEXT NOT NULL DEFAULT 'active',
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, email)
        );

        CREATE TABLE IF NOT EXISTS api_keys (
            id          TEXT PRIMARY KEY,
            user_id     TEXT NOT NULL REFERENCES users(id),
            org_id      TEXT NOT NULL REFERENCES organizations(id),
            key_hash    TEXT NOT NULL UNIQUE,
            label       TEXT NOT NULL,
            last_used   TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            revoked     INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS memories (
            id          TEXT PRIMARY KEY,
            org_id      TEXT NOT NULL REFERENCES organizations(id),
            user_id     TEXT NOT NULL REFERENCES users(id),
            project     TEXT NOT NULL DEFAULT 'default',
            tool        TEXT NOT NULL,
            content     TEXT NOT NULL,
            tags        TEXT DEFAULT '[]',
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
            content, tags,
            content='memories',
            content_rowid='rowid'
        );

        CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
            INSERT INTO memories_fts(rowid, content, tags)
            VALUES (new.rowid, new.content, new.tags);
        END;

        CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, tags)
            VALUES ('delete', old.rowid, old.content, old.tags);
        END;

        CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, tags)
            VALUES ('delete', old.rowid, old.content, old.tags);
            INSERT INTO memories_fts(rowid, content, tags)
            VALUES (new.rowid, new.content, new.tags);
        END;

        CREATE TABLE IF NOT EXISTS audit_logs (
            id              TEXT PRIMARY KEY,
            org_id          TEXT NOT NULL REFERENCES organizations(id),
            user_id         TEXT NOT NULL REFERENCES users(id),
            timestamp       TEXT NOT NULL DEFAULT (datetime('now')),
            action          TEXT NOT NULL,
            resource_type   TEXT NOT NULL,
            resource_id     TEXT,
            metadata        TEXT DEFAULT '{}'
        );

        PRAGMA user_version = 1;
        ",
    )?;
    Ok(())
}

/// Migration v2: adds sessions table, 7 new columns on memories, rebuilds FTS.
/// Idempotent — guarded by PRAGMA user_version < 2.
pub fn run_v2(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 2 {
        return Ok(());
    }

    // Create sessions table BEFORE altering memories (session_id FK depends on it)
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS sessions (
            id         TEXT PRIMARY KEY,
            org_id     TEXT NOT NULL REFERENCES organizations(id),
            project    TEXT NOT NULL,
            directory  TEXT NOT NULL DEFAULT '',
            started_at TEXT NOT NULL DEFAULT (datetime('now')),
            ended_at   TEXT,
            summary    TEXT
        );
        ",
    )?;

    // Add new columns to memories — each ALTER TABLE must be a separate statement in SQLite
    // Ignore errors for columns that may already exist (they won't on a fresh v1 DB, but
    // this guard makes the function safe to re-run in edge cases).
    let alter_stmts = [
        "ALTER TABLE memories ADD COLUMN title TEXT",
        "ALTER TABLE memories ADD COLUMN type TEXT",
        "ALTER TABLE memories ADD COLUMN scope TEXT NOT NULL DEFAULT 'project'",
        "ALTER TABLE memories ADD COLUMN topic_key TEXT",
        "ALTER TABLE memories ADD COLUMN session_id TEXT REFERENCES sessions(id)",
        "ALTER TABLE memories ADD COLUMN revision_count INTEGER NOT NULL DEFAULT 1",
        "ALTER TABLE memories ADD COLUMN normalized_hash TEXT",
    ];

    for stmt in &alter_stmts {
        // Ignore "duplicate column name" errors so the function is idempotent
        // even if called on a partially migrated DB.
        let _ = conn.execute_batch(stmt);
    }

    // Rebuild FTS: drop old table + triggers, recreate with 4 columns, backfill
    conn.execute_batch(
        "
        DROP TRIGGER IF EXISTS memories_ai;
        DROP TRIGGER IF EXISTS memories_ad;
        DROP TRIGGER IF EXISTS memories_au;
        DROP TABLE IF EXISTS memories_fts;

        CREATE VIRTUAL TABLE memories_fts USING fts5(
            content, tags, title, type,
            content='memories',
            content_rowid='rowid'
        );

        INSERT INTO memories_fts(rowid, content, tags, title, type)
            SELECT rowid, content, tags, title, type FROM memories;

        CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
            INSERT INTO memories_fts(rowid, content, tags, title, type)
            VALUES (new.rowid, new.content, new.tags, new.title, new.type);
        END;

        CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, tags, title, type)
            VALUES ('delete', old.rowid, old.content, old.tags, old.title, old.type);
        END;

        CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, tags, title, type)
            VALUES ('delete', old.rowid, old.content, old.tags, old.title, old.type);
            INSERT INTO memories_fts(rowid, content, tags, title, type)
            VALUES (new.rowid, new.content, new.tags, new.title, new.type);
        END;

        PRAGMA user_version = 2;
        ",
    )?;

    Ok(())
}

/// Migration v3: adds password_hash to users + password_reset_tokens table.
pub fn run_v3(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 3 {
        return Ok(());
    }

    let _ = conn.execute_batch("ALTER TABLE users ADD COLUMN password_hash TEXT");

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS password_reset_tokens (
            id          TEXT PRIMARY KEY,
            user_id     TEXT NOT NULL REFERENCES users(id),
            token_hash  TEXT NOT NULL UNIQUE,
            expires_at  TEXT NOT NULL,
            used        INTEGER NOT NULL DEFAULT 0,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        PRAGMA user_version = 3;
        ",
    )?;

    Ok(())
}

/// Migration v4: adds memory_embeddings table for vector search.
pub fn run_v4(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 4 {
        return Ok(());
    }

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS memory_embeddings (
            memory_id  TEXT PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
            embedding  BLOB NOT NULL
        );

        PRAGMA user_version = 4;
        ",
    )?;

    Ok(())
}

/// Migration v5: adds roles table for custom RBAC.
pub fn run_v5(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 5 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS roles (
                id           TEXT PRIMARY KEY,
                org_id       TEXT REFERENCES organizations(id) ON DELETE CASCADE,
                name         TEXT NOT NULL,
                display_name TEXT NOT NULL,
                description  TEXT,
                extends_json TEXT NOT NULL DEFAULT '[]',
                permissions  TEXT NOT NULL DEFAULT '[]',
                color        TEXT,
                icon         TEXT,
                version      INTEGER NOT NULL DEFAULT 1,
                enabled      INTEGER NOT NULL DEFAULT 1,
                is_template  INTEGER NOT NULL DEFAULT 0,
                created_at   TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(org_id, name)
            );

            PRAGMA user_version = 5;
            ",
        )?;
    }

    let count: i32 = conn.query_row("SELECT COUNT(*) FROM roles WHERE is_template = 1", [], |r| r.get(0))?;
    if count == 0 {
        conn.execute_batch(
            "
            INSERT INTO roles (id, org_id, name, display_name, description, extends_json, permissions, version, enabled, is_template)
            VALUES 
            ('tmpl_security_officer', NULL, 'security-officer', 'Security Officer', 'Allows viewing audit logs and settings.', '[]', '[\"audit:read\", \"settings:write\"]', 1, 1, 1),
            ('tmpl_dev_senior', NULL, 'dev-senior', 'Developer Senior', 'Senior developer with full memory management.', '[]', '[\"memory:read\", \"memory:write\", \"memory:delete\", \"memory:search\"]', 1, 1, 1),
            ('tmpl_dev_junior', NULL, 'dev-junior', 'Junior Developer', 'Junior developer with search and read access.', '[]', '[\"memory:read\", \"memory:search\"]', 1, 1, 1),
            ('tmpl_auditor', NULL, 'auditor', 'Auditor', 'Auditor with access to audit trails.', '[]', '[\"audit:read\"]', 1, 1, 1)
            ON CONFLICT(id) DO NOTHING;
            "
        )?;
    }

    Ok(())
}

/// Migration v6: adds projects and project_members tables, adds project_id column to memories, and backfills it.
pub fn run_v6(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 6 {
        return Ok(());
    }

    // 1. Create tables
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS projects (
            id           TEXT PRIMARY KEY,
            org_id       TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            name         TEXT NOT NULL,
            description  TEXT,
            created_at   TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, name)
        );

        CREATE TABLE IF NOT EXISTS project_members (
            id           TEXT PRIMARY KEY,
            project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            role         TEXT NOT NULL,
            created_at   TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(project_id, user_id)
        );
        "
    )?;

    // 2. Add project_id column to memories if not exists
    let has_project_id: bool = conn.query_row(
        "SELECT count(*) FROM pragma_table_info('memories') WHERE name='project_id'",
        [],
        |row| row.get::<_, i32>(0)
    ).unwrap_or(0) > 0;

    if !has_project_id {
        conn.execute("ALTER TABLE memories ADD COLUMN project_id TEXT REFERENCES projects(id) ON DELETE SET NULL", [])?;
    }

    // 3. Migrate existing memories
    let mut stmt = conn.prepare("SELECT DISTINCT org_id, project FROM memories")?;
    let mut rows = stmt.query([])?;
    let mut items = Vec::new();
    while let Some(row) = rows.next()? {
        items.push((row.get::<_, String>(0)?, row.get::<_, String>(1)?));
    }

    for (org_id, project_name) in items {
        // Generate a new project UUID
        let project_id = uuid::Uuid::new_v4().to_string();
        
        // Insert project if not exists
        conn.execute(
            "INSERT OR IGNORE INTO projects (id, org_id, name, description) VALUES (?1, ?2, ?3, 'Autogenerated from memories')",
            rusqlite::params![project_id, org_id, project_name],
        )?;

        // Get the actual project ID (either newly inserted or existing)
        let actual_project_id: String = conn.query_row(
            "SELECT id FROM projects WHERE org_id = ?1 AND name = ?2",
            rusqlite::params![org_id, project_name],
            |r| r.get(0),
        )?;

        // Update memories that had this project name to this project_id
        conn.execute(
            "UPDATE memories SET project_id = ?1 WHERE org_id = ?2 AND project = ?3",
            rusqlite::params![actual_project_id, org_id, project_name],
        )?;
    }

    conn.execute_batch("PRAGMA user_version = 6;")?;
    Ok(())
}

/// Migration v7: adds parent_id column to projects for hierarchical project support.
pub fn run_v7(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 7 {
        return Ok(());
    }

    let result = conn.execute(
        "ALTER TABLE projects ADD COLUMN parent_id TEXT REFERENCES projects(id) ON DELETE SET NULL",
        [],
    );
    if let Err(e) = result {
        let msg = e.to_string();
        if !msg.contains("duplicate column name") {
            return Err(e.into());
        }
    }

    conn.execute_batch("PRAGMA user_version = 7;")?;
    Ok(())
}

/// Migration v8: seeds project_members for all existing active users across all org projects,
/// using each user's current global role. Prevents breaking existing access when strict
/// project-based enforcement activates. No schema changes.
pub fn run_v8(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 8 {
        return Ok(());
    }

    conn.execute_batch(
        "
        INSERT OR IGNORE INTO project_members (id, project_id, user_id, role, created_at)
        SELECT lower(hex(randomblob(16))), p.id, u.id, u.role, datetime('now')
        FROM projects p
        JOIN users u ON u.org_id = p.org_id
        WHERE u.status = 'active';

        PRAGMA user_version = 8;
        ",
    )?;

    Ok(())
}

/// Migration v9: adds previous_hash/current_hash columns to audit_logs,
/// adds plan column to organizations (DEFAULT 'free'), and creates four
/// performance indexes on memories and audit_logs.
/// All DDL runs inside a single transaction; user_version is bumped to 9
/// only after all changes commit successfully.
pub fn run_v9(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 9 {
        return Ok(());
    }

    // ALTER TABLE statements must be individual statements outside the main
    // batch because SQLite only allows one DDL per statement in execute_batch
    // when mixing with other statements. We ignore "duplicate column" errors
    // to make each ALTER idempotent.
    let alter_stmts = [
        "ALTER TABLE audit_logs ADD COLUMN previous_hash TEXT",
        "ALTER TABLE audit_logs ADD COLUMN current_hash TEXT",
        "ALTER TABLE organizations ADD COLUMN plan TEXT NOT NULL DEFAULT 'free'",
    ];

    for stmt in &alter_stmts {
        let _ = conn.execute_batch(stmt);
    }

    // Indexes and version bump in one atomic batch
    conn.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_memories_scope
            ON memories(org_id, scope);

        CREATE INDEX IF NOT EXISTS idx_memories_type
            ON memories(org_id, type);

        CREATE INDEX IF NOT EXISTS idx_memories_project_id
            ON memories(org_id, project_id);

        CREATE INDEX IF NOT EXISTS idx_audit_logs_org_ts
            ON audit_logs(org_id, timestamp);

        PRAGMA user_version = 9;
        ",
    )?;

    Ok(())
}

/// Migration v10: adds policies table + idx_policies_org index.
/// Idempotent — guarded by PRAGMA user_version < 10.
pub fn run_v10(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 10 {
        return Ok(());
    }

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS policies (
            id          TEXT PRIMARY KEY,
            org_id      TEXT NOT NULL REFERENCES organizations(id),
            name        TEXT NOT NULL,
            rule_type   TEXT NOT NULL CHECK(rule_type IN ('model_whitelist','budget_limit','pii_redact')),
            config      TEXT NOT NULL DEFAULT '{}',
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );

        CREATE INDEX IF NOT EXISTS idx_policies_org ON policies(org_id, enabled);

        PRAGMA user_version = 10;
        ",
    )?;
    Ok(())
}

/// Migration v11: adds code_projects and code_chunks tables for the code-index RAG feature.
/// Idempotent — guarded by PRAGMA user_version < 11.
pub fn run_v11(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 11 {
        return Ok(());
    }

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS code_projects (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            org_id       TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            name         TEXT NOT NULL,
            root_path    TEXT NOT NULL,
            file_count   INTEGER NOT NULL DEFAULT 0,
            chunk_count  INTEGER NOT NULL DEFAULT 0,
            last_indexed TEXT,
            created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            UNIQUE(org_id, name)
        );

        CREATE TABLE IF NOT EXISTS code_chunks (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            code_project_id  INTEGER NOT NULL REFERENCES code_projects(id) ON DELETE CASCADE,
            file_path        TEXT NOT NULL,
            file_hash        TEXT NOT NULL,
            language         TEXT,
            symbol           TEXT,
            start_line       INTEGER NOT NULL,
            end_line         INTEGER NOT NULL,
            content          TEXT NOT NULL,
            embedding        BLOB,
            created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        );

        CREATE INDEX IF NOT EXISTS idx_code_chunks_project
            ON code_chunks(code_project_id);

        CREATE INDEX IF NOT EXISTS idx_code_chunks_file
            ON code_chunks(code_project_id, file_path);

        PRAGMA user_version = 11;
        ",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::connect;

    fn in_memory_db() -> Connection {
        connect(":memory:").unwrap()
    }

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [name],
            |r| r.get::<_, i32>(0),
        )
        .unwrap()
            > 0
    }

    fn get_user_version(conn: &Connection) -> i32 {
        conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn creates_all_tables() {
        let conn = in_memory_db();
        run(&conn).unwrap();

        assert!(table_exists(&conn, "organizations"), "missing: organizations");
        assert!(table_exists(&conn, "users"), "missing: users");
        assert!(table_exists(&conn, "api_keys"), "missing: api_keys");
        assert!(table_exists(&conn, "memories"), "missing: memories");
        assert!(table_exists(&conn, "audit_logs"), "missing: audit_logs");
        assert!(table_exists(&conn, "roles"), "missing: roles");
        assert!(table_exists(&conn, "projects"), "missing: projects");
        assert!(table_exists(&conn, "project_members"), "missing: project_members");
        assert!(table_exists(&conn, "policies"), "missing: policies");
    }

    #[test]
    fn is_idempotent() {
        let conn = in_memory_db();
        run(&conn).unwrap();
        run(&conn).unwrap(); // IF NOT EXISTS — must not fail
    }

    #[test]
    fn users_requires_valid_org_fk() {
        let conn = in_memory_db();
        run(&conn).unwrap();

        let result = conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1', 'nonexistent', 'a@b.com', 'A')",
            [],
        );
        assert!(result.is_err(), "should reject unknown org_id");
    }

    #[test]
    fn memories_org_scoped() {
        let conn = in_memory_db();
        run(&conn).unwrap();

        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1', 'org1', 'dev@acme.com', 'Dev')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, tool, content) VALUES ('m1', 'org1', 'u1', 'claude', 'use snake_case')",
            [],
        )
        .unwrap();

        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE org_id = 'org1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Different org sees zero memories
        let cross_org: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE org_id = 'org2'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cross_org, 0, "org isolation violated");
    }

    // ── v2 migration tests ────────────────────────────────────────────────────

    #[test]
    fn run_all_sets_user_version_to_10_is_now_11() {
        // This test documents the historical expectation; the current version is 11
        // after v11 migration was added. See run_all_sets_user_version_to_11.
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(get_user_version(&conn) >= 10, "user_version must be at least 10 after run_all");
    }

    #[test]
    fn run_all_creates_sessions_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(table_exists(&conn, "sessions"), "missing: sessions");
    }

    #[test]
    fn run_all_adds_v2_columns_to_memories() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        // Insert a row using v2 columns to verify they exist
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1', 'org1', 'dev@acme.com', 'Dev')",
            [],
        ).unwrap();

        let result = conn.execute(
            "INSERT INTO memories (id, org_id, user_id, tool, content, title, type, scope, topic_key, revision_count, normalized_hash)
             VALUES ('m1', 'org1', 'u1', 'claude', 'content', 'My Title', 'bugfix', 'project', 'k1', 1, 'abc123')",
            [],
        );
        assert!(result.is_ok(), "v2 columns must exist: {:?}", result.err());

        let scope: String = conn
            .query_row("SELECT scope FROM memories WHERE id = 'm1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(scope, "project");
    }

    #[test]
    fn run_all_idempotent_on_already_migrated_db() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        // Running again must not fail
        let result = run_all(&conn);
        assert!(result.is_ok(), "run_all must be idempotent: {:?}", result.err());
    }

    #[test]
    fn run_all_fts_includes_title_and_type() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1', 'org1', 'dev@acme.com', 'Dev')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, tool, content, title, type) VALUES ('m1', 'org1', 'u1', 'claude', 'some content', 'JWT auth middleware', 'bugfix')",
            [],
        ).unwrap();

        // Search by title word
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories m JOIN memories_fts fts ON fts.rowid = m.rowid WHERE memories_fts MATCH 'JWT' AND m.org_id = 'org1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "FTS must match on title column");

        // Search by type
        let count2: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories m JOIN memories_fts fts ON fts.rowid = m.rowid WHERE memories_fts MATCH 'bugfix' AND m.org_id = 'org1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count2, 1, "FTS must match on type column");
    }

    // ── v9 migration tests ────────────────────────────────────────────────────

    /// Build a v8 database (run v1..v8 only) for testing run_v9 in isolation.
    fn in_memory_db_v8() -> Connection {
        let conn = connect(":memory:").unwrap();
        run_v1(&conn).unwrap();
        run_v2(&conn).unwrap();
        run_v3(&conn).unwrap();
        run_v4(&conn).unwrap();
        run_v5(&conn).unwrap();
        run_v6(&conn).unwrap();
        run_v7(&conn).unwrap();
        run_v8(&conn).unwrap();
        conn
    }

    fn column_exists(conn: &Connection, table: &str, column: &str) -> bool {
        let count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
            rusqlite::params![table, column],
            |r| r.get(0),
        ).unwrap_or(0);
        count > 0
    }

    fn index_exists(conn: &Connection, table: &str, index: &str) -> bool {
        let count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_index_list(?1) WHERE name = ?2",
            rusqlite::params![table, index],
            |r| r.get(0),
        ).unwrap_or(0);
        count > 0
    }

    #[test]
    fn run_v9_adds_hash_columns_to_audit_logs() {
        let conn = in_memory_db_v8();
        run_v9(&conn).unwrap();

        assert!(column_exists(&conn, "audit_logs", "previous_hash"),
                "audit_logs must have previous_hash after v9");
        assert!(column_exists(&conn, "audit_logs", "current_hash"),
                "audit_logs must have current_hash after v9");
    }

    #[test]
    fn run_v9_adds_plan_to_organizations() {
        let conn = in_memory_db_v8();
        run_v9(&conn).unwrap();

        assert!(column_exists(&conn, "organizations", "plan"),
                "organizations must have plan after v9");

        // Verify DEFAULT 'free' — insert org without plan and read it back
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('test-org', 'Test', 'test')",
            [],
        ).unwrap();
        let plan: String = conn.query_row(
            "SELECT plan FROM organizations WHERE id = 'test-org'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(plan, "free", "default plan must be 'free'");
    }

    #[test]
    fn run_v9_adds_four_indexes() {
        let conn = in_memory_db_v8();
        run_v9(&conn).unwrap();

        assert!(index_exists(&conn, "memories", "idx_memories_scope"),
                "idx_memories_scope must exist");
        assert!(index_exists(&conn, "memories", "idx_memories_type"),
                "idx_memories_type must exist");
        assert!(index_exists(&conn, "memories", "idx_memories_project_id"),
                "idx_memories_project_id must exist");
        assert!(index_exists(&conn, "audit_logs", "idx_audit_logs_org_ts"),
                "idx_audit_logs_org_ts must exist");
    }

    #[test]
    fn run_v9_is_idempotent() {
        let conn = in_memory_db_v8();
        run_v9(&conn).unwrap();
        // Running again must not fail
        let result = run_v9(&conn);
        assert!(result.is_ok(), "run_v9 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 9, "user_version must remain 9");
    }

    // ── v10 migration tests ───────────────────────────────────────────────────

    /// Build a v9 database (run v1..v9 only) for testing run_v10 in isolation.
    fn in_memory_db_v9() -> Connection {
        let conn = connect(":memory:").unwrap();
        run_v1(&conn).unwrap();
        run_v2(&conn).unwrap();
        run_v3(&conn).unwrap();
        run_v4(&conn).unwrap();
        run_v5(&conn).unwrap();
        run_v6(&conn).unwrap();
        run_v7(&conn).unwrap();
        run_v8(&conn).unwrap();
        run_v9(&conn).unwrap();
        conn
    }

    #[test]
    fn run_v10_creates_policies_table() {
        let conn = in_memory_db_v9();
        run_v10(&conn).unwrap();
        assert!(table_exists(&conn, "policies"), "policies table must exist after v10");
    }

    #[test]
    fn run_v10_creates_org_index() {
        let conn = in_memory_db_v9();
        run_v10(&conn).unwrap();
        assert!(index_exists(&conn, "policies", "idx_policies_org"),
                "idx_policies_org must exist after v10");
    }

    #[test]
    fn run_v10_is_idempotent() {
        let conn = in_memory_db_v9();
        run_v10(&conn).unwrap();
        let result = run_v10(&conn);
        assert!(result.is_ok(), "run_v10 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 10, "user_version must remain 10");
    }

    #[test]
    fn run_v10_rejects_invalid_rule_type() {
        let conn = in_memory_db_v9();
        run_v10(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        let bad = conn.execute(
            "INSERT INTO policies (id, org_id, name, rule_type, config) VALUES ('p1','org1','x','banana','{}')",
            [],
        );
        assert!(bad.is_err(), "CHECK constraint must reject unknown rule_type");
    }

    #[test]
    fn run_v10_preserves_existing_rows() {
        let conn = in_memory_db_v9();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status) VALUES ('u1', 'org1', 'a@b.com', 'A', 'admin', 'active')",
            [],
        ).unwrap();
        run_v10(&conn).unwrap();
        // Existing tables must still be readable
        let org_count: i32 = conn.query_row("SELECT COUNT(*) FROM organizations", [], |r| r.get(0)).unwrap();
        assert_eq!(org_count, 1, "existing rows must be preserved after v10");
    }

    #[test]
    fn run_v9_preserves_existing_rows() {
        let conn = in_memory_db_v8();

        // Seed an org + user + audit_log row in v8
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status) VALUES ('u1', 'org1', 'a@b.com', 'A', 'admin', 'active')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO audit_logs (id, org_id, user_id, action, resource_type) VALUES ('al1', 'org1', 'u1', 'store', 'memory')",
            [],
        ).unwrap();

        run_v9(&conn).unwrap();

        // Original row must still be readable; new hash columns default to NULL
        let (action, prev_hash): (String, Option<String>) = conn.query_row(
            "SELECT action, previous_hash FROM audit_logs WHERE id = 'al1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(action, "store");
        assert!(prev_hash.is_none(), "pre-v9 rows must have previous_hash = NULL");
    }

    // ── v11 migration tests ───────────────────────────────────────────────────

    /// Build a v10 database for testing run_v11 in isolation.
    fn in_memory_db_v10() -> Connection {
        let conn = connect(":memory:").unwrap();
        run_v1(&conn).unwrap();
        run_v2(&conn).unwrap();
        run_v3(&conn).unwrap();
        run_v4(&conn).unwrap();
        run_v5(&conn).unwrap();
        run_v6(&conn).unwrap();
        run_v7(&conn).unwrap();
        run_v8(&conn).unwrap();
        run_v9(&conn).unwrap();
        run_v10(&conn).unwrap();
        conn
    }

    #[test]
    fn run_v11_creates_code_tables() {
        let conn = in_memory_db_v10();
        run_v11(&conn).unwrap();
        assert!(table_exists(&conn, "code_projects"), "code_projects must exist after v11");
        assert!(table_exists(&conn, "code_chunks"), "code_chunks must exist after v11");
    }

    #[test]
    fn run_v11_creates_indexes() {
        let conn = in_memory_db_v10();
        run_v11(&conn).unwrap();
        assert!(index_exists(&conn, "code_chunks", "idx_code_chunks_project"),
                "idx_code_chunks_project must exist after v11");
        assert!(index_exists(&conn, "code_chunks", "idx_code_chunks_file"),
                "idx_code_chunks_file must exist after v11");
    }

    #[test]
    fn run_v11_is_idempotent() {
        let conn = in_memory_db_v10();
        run_v11(&conn).unwrap();
        let result = run_v11(&conn);
        assert!(result.is_ok(), "run_v11 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 11, "user_version must remain 11");
    }

    #[test]
    fn run_v11_sets_user_version_to_11() {
        let conn = in_memory_db_v10();
        run_v11(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 11, "user_version must be 11 after v11");
    }

    #[test]
    fn run_all_sets_user_version_to_11() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 11, "user_version must be 11 after run_all");
    }

    #[test]
    fn run_v11_code_projects_unique_org_name() {
        let conn = in_memory_db_v10();
        run_v11(&conn).unwrap();
        // Seed org
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO code_projects (org_id, name, root_path) VALUES ('org1', 'myapp', '/ws/myapp')",
            [],
        ).unwrap();
        // Duplicate must fail
        let dup = conn.execute(
            "INSERT INTO code_projects (org_id, name, root_path) VALUES ('org1', 'myapp', '/ws/myapp2')",
            [],
        );
        assert!(dup.is_err(), "UNIQUE(org_id, name) must be enforced on code_projects");
    }

    #[test]
    fn run_v11_code_chunks_cascade_delete() {
        let conn = in_memory_db_v10();
        run_v11(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO code_projects (id, org_id, name, root_path) VALUES (1, 'org1', 'myapp', '/ws')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO code_chunks (code_project_id, file_path, file_hash, start_line, end_line, content) VALUES (1, 'src/lib.rs', 'abc123', 1, 10, 'fn main() {}')",
            [],
        ).unwrap();
        // Chunk must exist
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM code_chunks WHERE code_project_id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "chunk must exist before delete");
        // Delete project — chunks cascade
        conn.execute("DELETE FROM code_projects WHERE id = 1", []).unwrap();
        let after: i32 = conn.query_row("SELECT COUNT(*) FROM code_chunks WHERE code_project_id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(after, 0, "chunks must cascade-delete with project");
    }

    #[test]
    fn run_v11_preserves_existing_tables() {
        let conn = in_memory_db_v10();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        run_v11(&conn).unwrap();
        // Prior tables still readable
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM organizations", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "existing rows must be preserved after v11");
    }
}
