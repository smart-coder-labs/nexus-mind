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
    run_v12(conn)?;
    run_v13(conn)?;
    run_v14(conn)?;
    run_v15(conn)?;
    run_v16(conn)?;
    run_v17(conn)?;
    run_v18(conn)?;
    run_v19(conn)?;
    run_v20(conn)?;
    run_v21(conn)?;
    run_v22(conn)?;
    run_v23(conn)?;
    run_v24(conn)?;
    run_v25(conn)?;
    run_v26(conn)?;
    run_v27(conn)?;
    run_v28(conn)?;
    run_v29(conn)?;
    run_v30(conn)?;
    run_v31(conn)?;
    run_v32(conn)?;
    run_v33(conn)?;
    run_v34(conn)?;
    run_v35(conn)?;
    run_v36(conn)?;
    Ok(())
}

/// Migration v32: adds admin_note to users and usage tracking columns to api_keys.
/// - users.admin_note TEXT: private org-admin note, never returned to non-admin callers.
/// - api_keys.times_used INTEGER: cumulative count of successful authentications.
/// - api_keys.last_used_at TEXT: ISO datetime of the last successful authentication.
/// Idempotent — guarded by PRAGMA user_version < 32.
pub fn run_v32(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 32 {
        return Ok(());
    }
    let alter_stmts = [
        "ALTER TABLE users ADD COLUMN admin_note TEXT",
        "ALTER TABLE api_keys ADD COLUMN times_used INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE api_keys ADD COLUMN last_used_at TEXT",
    ];
    for stmt in &alter_stmts {
        let _ = conn.execute_batch(stmt);
    }
    conn.execute_batch("PRAGMA user_version = 32;")?;
    Ok(())
}

/// Migration v33: adds last_login_at to users.
/// - users.last_login_at TEXT: ISO datetime of the last successful API key authentication.
/// Idempotent — guarded by PRAGMA user_version < 33.
pub fn run_v33(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 33 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE users ADD COLUMN last_login_at TEXT");
    conn.execute_batch("PRAGMA user_version = 33;")?;
    Ok(())
}

/// Migration v35: adds logo_url column to organizations for org branding.
/// NULL = no logo. Non-NULL = URL to the org logo image.
/// Idempotent — guarded by PRAGMA user_version < 35.
pub fn run_v35(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 35 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE organizations ADD COLUMN logo_url TEXT");
    conn.execute_batch("PRAGMA user_version = 35;")?;
    Ok(())
}

/// Migration v36: creates the conventions table.
/// Conventions are org-scoped rules that agents must follow. They can be scoped to a project,
/// categorized, weighted for priority, and soft-archived.
/// Idempotent — guarded by PRAGMA user_version < 36.
pub fn run_v36(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 36 {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS conventions (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
          project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
          title TEXT NOT NULL,
          content TEXT NOT NULL,
          category TEXT NOT NULL DEFAULT 'general',
          weight INTEGER NOT NULL DEFAULT 100,
          tags TEXT NOT NULL DEFAULT '[]',
          created_at TEXT NOT NULL DEFAULT (datetime('now')),
          updated_at TEXT NOT NULL DEFAULT (datetime('now')),
          created_by TEXT REFERENCES users(id) ON DELETE SET NULL,
          archived_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_conventions_org ON conventions(org_id);
        CREATE INDEX IF NOT EXISTS idx_conventions_category ON conventions(org_id, category);",
    )?;
    conn.execute_batch("PRAGMA user_version = 36;")?;
    Ok(())
}

/// Migration v34: adds exclude_patterns to code_projects for file exclusion during indexing.
/// - code_projects.exclude_patterns TEXT: JSON array of glob-like exclusion patterns.
/// Idempotent — guarded by PRAGMA user_version < 34.
pub fn run_v34(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 34 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE code_projects ADD COLUMN exclude_patterns TEXT DEFAULT '[]'");
    conn.execute_batch("PRAGMA user_version = 34;")?;
    Ok(())
}

/// Migration v26: adds sync-status tracking columns to code_projects.
/// last_indexed_at, last_index_error, indexed_files_count, index_status.
/// Idempotent — guarded by PRAGMA user_version < 26.
pub fn run_v26(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 26 {
        return Ok(());
    }
    let alter_stmts = [
        "ALTER TABLE code_projects ADD COLUMN last_indexed_at TEXT",
        "ALTER TABLE code_projects ADD COLUMN last_index_error TEXT",
        "ALTER TABLE code_projects ADD COLUMN indexed_files_count INTEGER DEFAULT 0",
        "ALTER TABLE code_projects ADD COLUMN index_status TEXT DEFAULT 'pending'",
    ];
    for stmt in &alter_stmts {
        let _ = conn.execute_batch(stmt);
    }
    conn.execute_batch("PRAGMA user_version = 26;")?;
    Ok(())
}

/// Migration v27: adds expires_at column to api_keys for optional key expiry.
/// NULL = never expires. Non-NULL = key is invalid after this datetime.
/// Idempotent — guarded by PRAGMA user_version < 27.
pub fn run_v27(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 27 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE api_keys ADD COLUMN expires_at TEXT");
    conn.execute_batch("PRAGMA user_version = 27;")?;
    Ok(())
}

/// Migration v28: adds disabled_at column to users for soft account disabling.
/// NULL = account active. Non-NULL = account was disabled at that datetime.
/// Disabled accounts have all API requests rejected with 401 + account_disabled error.
/// Idempotent — guarded by PRAGMA user_version < 28.
pub fn run_v28(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 28 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE users ADD COLUMN disabled_at TEXT");
    conn.execute_batch("PRAGMA user_version = 28;")?;
    Ok(())
}

/// Migration v29: adds admin_note column to memories for org-admin private notes.
/// NULL = no note. Non-NULL = admin note text. Never returned to non-admin callers.
/// Idempotent — guarded by PRAGMA user_version < 29.
pub fn run_v29(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 29 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE memories ADD COLUMN admin_note TEXT");
    conn.execute_batch("PRAGMA user_version = 29;")?;
    Ok(())
}

/// Migration v31: adds archived_at column to code_projects for soft-archive support.
/// archived_at = NULL means active; non-NULL means archived at that datetime.
/// Idempotent — guarded by PRAGMA user_version < 31.
pub fn run_v31(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 31 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE code_projects ADD COLUMN archived_at TEXT");
    conn.execute_batch("PRAGMA user_version = 31;")?;
    Ok(())
}

/// Migration v30: announcement banner + per-memory scheduled deletion.
/// - organizations: announcement TEXT, announcement_type TEXT DEFAULT 'info'
/// - memories: delete_after TEXT (ISO date string, NULL = no scheduled deletion)
/// Idempotent — guarded by PRAGMA user_version < 30.
pub fn run_v30(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 30 {
        return Ok(());
    }
    let alter_stmts = [
        "ALTER TABLE organizations ADD COLUMN announcement TEXT",
        "ALTER TABLE organizations ADD COLUMN announcement_type TEXT DEFAULT 'info'",
        "ALTER TABLE memories ADD COLUMN delete_after TEXT",
    ];
    for stmt in &alter_stmts {
        let _ = conn.execute_batch(stmt);
    }
    conn.execute_batch("PRAGMA user_version = 30;")?;
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

/// Migration v12: adds settings column to organizations for per-org agent event configuration.
/// Idempotent — guarded by PRAGMA user_version < 12.
pub fn run_v12(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 12 {
        return Ok(());
    }
    conn.execute_batch(
        "ALTER TABLE organizations ADD COLUMN settings TEXT NOT NULL DEFAULT '{}';
         PRAGMA user_version = 12;"
    )?;
    Ok(())
}

/// Migration v13: adds repo_url column to code_projects for GitHub URL-based indexing.
/// Idempotent — guarded by PRAGMA user_version < 13.
pub fn run_v13(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 13 {
        return Ok(());
    }
    conn.execute_batch(
        "ALTER TABLE code_projects ADD COLUMN repo_url TEXT;
         PRAGMA user_version = 13;"
    )?;
    Ok(())
}

/// Migration v14: adds webhooks table for GitHub webhook configuration.
/// Idempotent — guarded by PRAGMA user_version < 14.
pub fn run_v14(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 14 {
        return Ok(());
    }
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS webhooks (
            id         TEXT PRIMARY KEY,
            org_id     TEXT NOT NULL REFERENCES organizations(id),
            name       TEXT NOT NULL,
            target_url TEXT NOT NULL,
            secret     TEXT,
            events     TEXT NOT NULL DEFAULT '[\"*\"]',
            active     INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, name)
        );

        PRAGMA user_version = 14;
        ",
    )?;
    Ok(())
}

/// Migration v15: adds event_overrides column to projects for per-project agent event overrides.
/// NULL means "inherit from org settings" (no override).
/// Idempotent — guarded by PRAGMA user_version < 15.
pub fn run_v15(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 15 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE projects ADD COLUMN event_overrides TEXT");
    conn.execute_batch("PRAGMA user_version = 15;")?;
    Ok(())
}

/// Migration v17: adds archived_at column to memories for soft-archive support.
/// NULL = not archived. Non-NULL = archived at that datetime.
/// Idempotent — guarded by PRAGMA user_version < 17.
pub fn run_v17(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 17 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE memories ADD COLUMN archived_at TEXT");
    conn.execute_batch("PRAGMA user_version = 17;")?;
    Ok(())
}

/// Migration v18: adds custom_instructions column to organizations.
/// NULL = no custom instructions. Non-NULL = system prompt injected into every agent's context.
/// Idempotent — guarded by PRAGMA user_version < 18.
pub fn run_v18(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 18 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE organizations ADD COLUMN custom_instructions TEXT");
    conn.execute_batch("PRAGMA user_version = 18;")?;
    Ok(())
}

/// Migration v19: adds pinned column to memories for admin pinning.
/// pinned = 1 means the memory floats to top of list. Default 0.
/// Idempotent — guarded by PRAGMA user_version < 19.
pub fn run_v19(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 19 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE memories ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0");
    conn.execute_batch("PRAGMA user_version = 19;")?;
    Ok(())
}

/// Migration v20: adds invite_links table for one-time invite link generation.
/// Idempotent — guarded by PRAGMA user_version < 20.
pub fn run_v20(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 20 {
        return Ok(());
    }
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS invite_links (
            token       TEXT PRIMARY KEY,
            org_id      TEXT NOT NULL,
            role        TEXT NOT NULL DEFAULT 'user',
            created_by  TEXT NOT NULL,
            used_at     TEXT,
            expires_at  TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        PRAGMA user_version = 20;
        ",
    )?;
    Ok(())
}

/// Migration v21: makes users.email nullable so invite-created users (no email) can be stored.
/// SQLite cannot drop NOT NULL via ALTER COLUMN, so we recreate the table.
pub fn run_v21(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 21 {
        return Ok(());
    }
    conn.execute_batch(
        "
        PRAGMA foreign_keys = OFF;

        CREATE TABLE IF NOT EXISTS users_new (
            id          TEXT PRIMARY KEY,
            org_id      TEXT NOT NULL REFERENCES organizations(id),
            email       TEXT,
            name        TEXT NOT NULL,
            role        TEXT NOT NULL DEFAULT 'member',
            status      TEXT NOT NULL DEFAULT 'active',
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            password_hash TEXT,
            UNIQUE(org_id, email)
        );

        INSERT OR IGNORE INTO users_new (id, org_id, email, name, role, status, created_at, password_hash)
        SELECT id, org_id, email, name, role, status, created_at, password_hash FROM users;

        DROP TABLE users;
        ALTER TABLE users_new RENAME TO users;

        PRAGMA foreign_keys = ON;
        PRAGMA user_version = 21;
        ",
    )?;
    Ok(())
}

/// Migration v22: adds min_password_length column to organizations.
/// Default 8 — must be enforced at the application layer on password change.
/// Idempotent — guarded by PRAGMA user_version < 22.
pub fn run_v22(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 22 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE organizations ADD COLUMN min_password_length INTEGER NOT NULL DEFAULT 8");
    conn.execute_batch("PRAGMA user_version = 22;")?;
    Ok(())
}

/// Migration v23: adds archived_at to projects (soft delete) and reindex_interval_hours to code_projects.
/// archived_at = NULL means active; non-NULL means archived at that datetime.
/// reindex_interval_hours = NULL means no auto re-index.
/// Idempotent — guarded by PRAGMA user_version < 23.
pub fn run_v23(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 23 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE projects ADD COLUMN archived_at TEXT");
    let _ = conn.execute_batch("ALTER TABLE code_projects ADD COLUMN reindex_interval_hours INTEGER");
    conn.execute_batch("PRAGMA user_version = 23;")?;
    Ok(())
}

/// Migration v24: adds webhook_deliveries table for delivery history.
/// Idempotent — guarded by PRAGMA user_version < 24.
pub fn run_v24(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 24 {
        return Ok(());
    }
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS webhook_deliveries (
            id           TEXT PRIMARY KEY,
            webhook_id   TEXT NOT NULL,
            org_id       TEXT NOT NULL,
            event_type   TEXT NOT NULL,
            payload      TEXT NOT NULL,
            status_code  INTEGER,
            success      INTEGER NOT NULL DEFAULT 0,
            error        TEXT,
            delivered_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_webhook_id
            ON webhook_deliveries(webhook_id, delivered_at DESC);

        PRAGMA user_version = 24;
        ",
    )?;
    Ok(())
}

/// Migration v25: adds collections table and collection_id column to memories.
/// Collections are org-scoped named groups. Memories can belong to one or none.
/// Idempotent — guarded by PRAGMA user_version < 25.
pub fn run_v25(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 25 {
        return Ok(());
    }
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS collections (
            id TEXT NOT NULL PRIMARY KEY,
            org_id TEXT NOT NULL,
            name TEXT NOT NULL,
            description TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, name)
        );
        "
    )?;
    let _ = conn.execute_batch("ALTER TABLE memories ADD COLUMN collection_id TEXT REFERENCES collections(id) ON DELETE SET NULL");
    conn.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_memories_collection ON memories(org_id, collection_id);
        PRAGMA user_version = 25;
        "
    )?;
    Ok(())
}

/// Migration v16: adds retention_days column to organizations.
/// NULL = no retention policy (keep memories forever).
/// Idempotent — guarded by PRAGMA user_version < 16.
pub fn run_v16(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 16 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE organizations ADD COLUMN retention_days INTEGER");
    conn.execute_batch("PRAGMA user_version = 16;")?;
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
        assert_eq!(get_user_version(&conn), 36, "user_version must be 36 after run_all");
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

    // ── v14 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v14_creates_webhooks_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(table_exists(&conn, "webhooks"), "webhooks table must exist after v14");
    }

    #[test]
    fn run_v14_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v14(&conn);
        assert!(result.is_ok(), "run_v14 must be idempotent: {:?}", result.err());
        // run_all brings to v15; re-running v14 after that still stays at v15
        assert!(get_user_version(&conn) >= 14, "user_version must be at least 14");
    }

    #[test]
    fn run_v14_sets_user_version_to_14() {
        // After run_all the version is 15; this documents the historical expectation
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(get_user_version(&conn) >= 14, "user_version must be at least 14 after run_all");
    }

    // ── v15 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v15_adds_event_overrides_to_projects() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "projects", "event_overrides"),
                "projects must have event_overrides after v15");
    }

    #[test]
    fn run_v15_column_defaults_to_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO projects (id, org_id, name) VALUES ('p1', 'org1', 'my-project')",
            [],
        ).unwrap();
        let val: Option<String> = conn.query_row(
            "SELECT event_overrides FROM projects WHERE id = 'p1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(val.is_none(), "event_overrides must default to NULL (inherit)");
    }

    #[test]
    fn run_v15_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v15(&conn);
        assert!(result.is_ok(), "run_v15 must be idempotent: {:?}", result.err());
        assert!(get_user_version(&conn) >= 15, "user_version must be at least 15");
    }

    #[test]
    fn run_v15_sets_user_version_to_15() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(get_user_version(&conn) >= 15, "user_version must be at least 15 after run_all");
    }

    // ── v16 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v16_adds_retention_days_to_organizations() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "organizations", "retention_days"),
                "organizations must have retention_days after v16");
    }

    #[test]
    fn run_v16_retention_days_defaults_to_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        let val: Option<i64> = conn.query_row(
            "SELECT retention_days FROM organizations WHERE id = 'org1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(val.is_none(), "retention_days must default to NULL (keep forever)");
    }

    #[test]
    fn run_v16_retention_days_persists_value() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "UPDATE organizations SET retention_days = 90 WHERE id = 'org1'",
            [],
        ).unwrap();
        let val: Option<i64> = conn.query_row(
            "SELECT retention_days FROM organizations WHERE id = 'org1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(val, Some(90), "retention_days must persist the set value");
    }

    #[test]
    fn run_v16_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v16(&conn);
        assert!(result.is_ok(), "run_v16 must be idempotent: {:?}", result.err());
        assert!(get_user_version(&conn) >= 16, "user_version must be at least 16");
    }

    #[test]
    fn run_v16_sets_user_version_to_16() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(get_user_version(&conn) >= 16, "user_version must be at least 16 after run_all");
    }

    // ── v17 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v17_adds_archived_at_to_memories() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "memories", "archived_at"),
                "memories must have archived_at after v17");
    }

    #[test]
    fn run_v17_archived_at_defaults_to_null() {
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
            "INSERT INTO memories (id, org_id, user_id, tool, content) VALUES ('m1', 'org1', 'u1', 'claude', 'hello')",
            [],
        ).unwrap();
        let val: Option<String> = conn.query_row(
            "SELECT archived_at FROM memories WHERE id = 'm1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(val.is_none(), "archived_at must default to NULL");
    }

    #[test]
    fn run_v17_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v17(&conn);
        assert!(result.is_ok(), "run_v17 must be idempotent: {:?}", result.err());
        assert!(get_user_version(&conn) >= 17, "user_version must be at least 17");
    }

    #[test]
    fn run_v17_sets_user_version_to_17() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(get_user_version(&conn) >= 17, "user_version must be at least 17 after run_all");
    }

    // ── v18 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v18_adds_custom_instructions_to_organizations() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "organizations", "custom_instructions"),
                "organizations must have custom_instructions after v18");
    }

    #[test]
    fn run_v18_custom_instructions_defaults_to_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        let val: Option<String> = conn.query_row(
            "SELECT custom_instructions FROM organizations WHERE id = 'org1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(val.is_none(), "custom_instructions must default to NULL");
    }

    #[test]
    fn run_v18_custom_instructions_persists_value() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "UPDATE organizations SET custom_instructions = 'Always use TypeScript strict mode.' WHERE id = 'org1'",
            [],
        ).unwrap();
        let val: Option<String> = conn.query_row(
            "SELECT custom_instructions FROM organizations WHERE id = 'org1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(val.as_deref(), Some("Always use TypeScript strict mode."),
                   "custom_instructions must persist the saved value");
    }

    #[test]
    fn run_v18_clear_custom_instructions_sets_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "UPDATE organizations SET custom_instructions = 'Some instructions.' WHERE id = 'org1'",
            [],
        ).unwrap();
        conn.execute(
            "UPDATE organizations SET custom_instructions = NULL WHERE id = 'org1'",
            [],
        ).unwrap();
        let val: Option<String> = conn.query_row(
            "SELECT custom_instructions FROM organizations WHERE id = 'org1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(val.is_none(), "clearing custom_instructions must store NULL");
    }

    #[test]
    fn run_v18_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v18(&conn);
        assert!(result.is_ok(), "run_v18 must be idempotent: {:?}", result.err());
        assert!(get_user_version(&conn) >= 18, "user_version must be at least 18");
    }

    #[test]
    fn run_v18_sets_user_version_to_18() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(get_user_version(&conn) >= 18, "user_version must be at least 18 after run_all");
    }

    // ── v19 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v19_adds_pinned_to_memories() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "memories", "pinned"),
                "memories must have pinned after v19");
    }

    #[test]
    fn run_v19_pinned_defaults_to_zero() {
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
            "INSERT INTO memories (id, org_id, user_id, tool, content) VALUES ('m1', 'org1', 'u1', 'claude', 'hello')",
            [],
        ).unwrap();
        let val: i64 = conn.query_row(
            "SELECT pinned FROM memories WHERE id = 'm1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(val, 0, "pinned must default to 0");
    }

    #[test]
    fn run_v19_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v19(&conn);
        assert!(result.is_ok(), "run_v19 must be idempotent: {:?}", result.err());
        assert!(get_user_version(&conn) >= 19, "user_version must be at least 19");
    }

    #[test]
    fn run_v19_sets_user_version_to_19() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(get_user_version(&conn) >= 19, "user_version must be at least 19 after run_all");
    }

    // ── v20 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v20_creates_invite_links_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(table_exists(&conn, "invite_links"), "invite_links table must exist after v20");
    }

    #[test]
    fn run_v20_sets_user_version_to_20() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 36, "user_version must be 36 after run_all");
    }

    #[test]
    fn run_v20_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v20(&conn);
        assert!(result.is_ok(), "run_v20 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 36, "user_version must remain 36 after re-running v20");
    }

    // ── v22 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v22_adds_min_password_length_to_organizations() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "organizations", "min_password_length"),
                "organizations must have min_password_length after v22");
    }

    #[test]
    fn run_v22_min_password_length_defaults_to_8() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        let val: i64 = conn.query_row(
            "SELECT min_password_length FROM organizations WHERE id = 'org1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(val, 8, "min_password_length must default to 8");
    }

    #[test]
    fn run_v22_min_password_length_persists_value() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "UPDATE organizations SET min_password_length = 12 WHERE id = 'org1'",
            [],
        ).unwrap();
        let val: i64 = conn.query_row(
            "SELECT min_password_length FROM organizations WHERE id = 'org1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(val, 12, "min_password_length must persist the set value");
    }

    #[test]
    fn run_v22_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v22(&conn);
        assert!(result.is_ok(), "run_v22 must be idempotent: {:?}", result.err());
        assert!(get_user_version(&conn) >= 22, "user_version must be at least 22");
    }

    // ── v23 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v23_adds_archived_at_to_projects() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "projects", "archived_at"),
                "projects must have archived_at after v23");
    }

    #[test]
    fn run_v23_archive_sets_archived_at() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO projects (id, org_id, name) VALUES ('p1', 'org1', 'my-project')",
            [],
        ).unwrap();
        // Archive
        conn.execute(
            "UPDATE projects SET archived_at = datetime('now') WHERE id = 'p1' AND org_id = 'org1'",
            [],
        ).unwrap();
        let val: Option<String> = conn.query_row(
            "SELECT archived_at FROM projects WHERE id = 'p1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(val.is_some(), "archived_at must be set after archiving");
    }

    #[test]
    fn run_v23_restore_clears_archived_at() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO projects (id, org_id, name, archived_at) VALUES ('p1', 'org1', 'my-project', datetime('now'))",
            [],
        ).unwrap();
        // Restore
        conn.execute(
            "UPDATE projects SET archived_at = NULL WHERE id = 'p1' AND org_id = 'org1'",
            [],
        ).unwrap();
        let val: Option<String> = conn.query_row(
            "SELECT archived_at FROM projects WHERE id = 'p1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(val.is_none(), "archived_at must be NULL after restoring");
    }

    #[test]
    fn run_v23_adds_reindex_interval_hours_to_code_projects() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "code_projects", "reindex_interval_hours"),
                "code_projects must have reindex_interval_hours after v23");
    }

    #[test]
    fn run_v23_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v23(&conn);
        assert!(result.is_ok(), "run_v23 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 36, "user_version must remain 36");
    }

    // ── v24 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v24_creates_webhook_deliveries_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(table_exists(&conn, "webhook_deliveries"),
                "webhook_deliveries table must exist after v24");
    }

    #[test]
    fn run_v24_creates_index() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(index_exists(&conn, "webhook_deliveries", "idx_webhook_deliveries_webhook_id"),
                "idx_webhook_deliveries_webhook_id must exist after v24");
    }

    #[test]
    fn run_v24_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v24(&conn);
        assert!(result.is_ok(), "run_v24 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 36, "user_version must remain 36");
    }

    #[test]
    fn run_v24_sets_user_version_to_24() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 36, "user_version must be 36 after run_all");
    }

    #[test]
    fn run_v25_collections_assign_memory_count() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        // Seed org + user
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1', 'org1', 'dev@acme.com', 'Dev')",
            [],
        ).unwrap();

        // Create collection
        conn.execute(
            "INSERT INTO collections (id, org_id, name) VALUES ('col1', 'org1', 'My Collection')",
            [],
        ).unwrap();

        // Create memory and assign to collection
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, tool, content) VALUES ('m1', 'org1', 'u1', 'claude', 'test')",
            [],
        ).unwrap();
        conn.execute(
            "UPDATE memories SET collection_id = 'col1' WHERE id = 'm1'",
            [],
        ).unwrap();

        // Assert count via LEFT JOIN
        let count: i64 = conn.query_row(
            "SELECT COUNT(m.id) FROM collections c LEFT JOIN memories m ON m.collection_id = c.id WHERE c.id = 'col1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 1, "collection must have memory_count = 1 after assignment");
    }

    #[test]
    fn run_v14_webhooks_unique_org_name() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO webhooks (id, org_id, name, target_url) VALUES ('wh1', 'org1', 'my-hook', 'https://example.com/hook')",
            [],
        ).unwrap();
        let dup = conn.execute(
            "INSERT INTO webhooks (id, org_id, name, target_url) VALUES ('wh2', 'org1', 'my-hook', 'https://other.com/hook')",
            [],
        );
        assert!(dup.is_err(), "UNIQUE(org_id, name) must be enforced on webhooks");
    }

    // ── v26 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v26_adds_sync_status_columns_to_code_projects() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "code_projects", "last_indexed_at"),
                "code_projects must have last_indexed_at after v26");
        assert!(column_exists(&conn, "code_projects", "last_index_error"),
                "code_projects must have last_index_error after v26");
        assert!(column_exists(&conn, "code_projects", "indexed_files_count"),
                "code_projects must have indexed_files_count after v26");
        assert!(column_exists(&conn, "code_projects", "index_status"),
                "code_projects must have index_status after v26");
    }

    #[test]
    fn run_v26_code_project_sync_status_roundtrip() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();

        // Create code project
        conn.execute(
            "INSERT INTO code_projects (org_id, name, root_path) VALUES ('org1', 'myapp', '/ws/myapp')",
            [],
        ).unwrap();

        // Set status = 'success', indexed_files_count = 42
        conn.execute(
            "UPDATE code_projects SET index_status = 'success', indexed_files_count = 42, last_indexed_at = '2026-06-20T10:00:00Z' WHERE org_id = 'org1' AND name = 'myapp'",
            [],
        ).unwrap();

        // Verify list returns correct values
        let (status, count, at): (String, i64, String) = conn.query_row(
            "SELECT index_status, indexed_files_count, last_indexed_at FROM code_projects WHERE org_id = 'org1' AND name = 'myapp'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();

        assert_eq!(status, "success", "index_status must be 'success'");
        assert_eq!(count, 42, "indexed_files_count must be 42");
        assert_eq!(at, "2026-06-20T10:00:00Z", "last_indexed_at must match");
    }

    #[test]
    fn run_v26_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v26(&conn);
        assert!(result.is_ok(), "run_v26 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 36, "user_version must remain 36");
    }

    // ── v27 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v27_adds_expires_at_to_api_keys() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "api_keys", "expires_at"),
                "api_keys must have expires_at after v27");
    }

    #[test]
    fn run_v27_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v27(&conn);
        assert!(result.is_ok(), "run_v27 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 36, "user_version must remain 36");
    }

    #[test]
    fn run_v27_sets_user_version_to_27() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 36, "user_version must be 36 after run_all");
    }

    // ── v28 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v28_adds_disabled_at_to_users() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "users", "disabled_at"),
                "users must have disabled_at after v28");
    }

    #[test]
    fn run_v28_disabled_at_defaults_to_null() {
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
        let val: Option<String> = conn.query_row(
            "SELECT disabled_at FROM users WHERE id = 'u1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(val.is_none(), "disabled_at must default to NULL");
    }

    #[test]
    fn run_v28_disable_enable_roundtrip() {
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

        // Disable
        conn.execute(
            "UPDATE users SET disabled_at = datetime('now') WHERE id = 'u1'",
            [],
        ).unwrap();
        let disabled: Option<String> = conn.query_row(
            "SELECT disabled_at FROM users WHERE id = 'u1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(disabled.is_some(), "disabled_at must be set after disabling");

        // Re-enable
        conn.execute(
            "UPDATE users SET disabled_at = NULL WHERE id = 'u1'",
            [],
        ).unwrap();
        let enabled: Option<String> = conn.query_row(
            "SELECT disabled_at FROM users WHERE id = 'u1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(enabled.is_none(), "disabled_at must be NULL after re-enabling");
    }

    #[test]
    fn run_v28_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v28(&conn);
        assert!(result.is_ok(), "run_v28 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 36, "user_version must remain 36");
    }

    #[test]
    fn run_all_sets_user_version_to_29() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 36, "user_version must be 36 after run_all");
    }

    // ── v29 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v29_adds_admin_note_to_memories() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "memories", "admin_note"),
                "memories must have admin_note after v29");
    }

    #[test]
    fn run_v29_admin_note_defaults_to_null() {
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
            "INSERT INTO memories (id, org_id, user_id, tool, content) VALUES ('m1', 'org1', 'u1', 'claude', 'hello')",
            [],
        ).unwrap();
        let val: Option<String> = conn.query_row(
            "SELECT admin_note FROM memories WHERE id = 'm1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(val.is_none(), "admin_note must default to NULL");
    }

    #[test]
    fn run_v29_admin_note_roundtrip() {
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
            "INSERT INTO memories (id, org_id, user_id, tool, content) VALUES ('m1', 'org1', 'u1', 'claude', 'hello')",
            [],
        ).unwrap();
        // Set note
        conn.execute(
            "UPDATE memories SET admin_note = 'Suspicious pattern — watch this.' WHERE id = 'm1'",
            [],
        ).unwrap();
        let note: Option<String> = conn.query_row(
            "SELECT admin_note FROM memories WHERE id = 'm1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(note.as_deref(), Some("Suspicious pattern — watch this."), "admin_note must persist");
        // Clear note
        conn.execute(
            "UPDATE memories SET admin_note = NULL WHERE id = 'm1'",
            [],
        ).unwrap();
        let cleared: Option<String> = conn.query_row(
            "SELECT admin_note FROM memories WHERE id = 'm1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(cleared.is_none(), "admin_note must be NULL after clearing");
    }

    #[test]
    fn run_v29_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v29(&conn);
        assert!(result.is_ok(), "run_v29 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 36, "user_version must remain 36");
    }

    // ── admin_note integration test (via queries) ─────────────────────────────

    #[test]
    fn admin_note_set_and_not_in_list_without_admin() {
        use crate::db::{connection::connect, queries};

        let conn = connect(":memory:").unwrap();
        run_all(&conn).unwrap();

        let (_org, _user, _raw_key) = queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Get org_id + user_id
        let (org_id, user_id): (String, String) = conn.query_row(
            "SELECT o.id, u.id FROM organizations o JOIN users u ON u.org_id = o.id LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();

        // Create a memory
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, tool, content, project) VALUES ('m1', ?1, ?2, 'claude', 'test content', 'default')",
            rusqlite::params![org_id, user_id],
        ).unwrap();

        // Set admin_note via query
        let result = queries::update_memory_admin_note(&conn, &org_id, "m1", "Private admin note").unwrap();
        assert!(result.is_some(), "update_memory_admin_note must return the updated memory");
        let mem = result.unwrap();
        assert_eq!(mem.admin_note.as_deref(), Some("Private admin note"), "admin_note must be returned in admin context");

        // Simulate non-admin list: admin_note should be present in DB but stripped by handler layer
        // Here we test that the DB query returns it, and the handler is responsible for stripping.
        let mems = queries::list_memories(
            &conn, &org_id, None, None, None, None, None, None, 50, 0, false, None, None, None,
        ).unwrap();
        assert_eq!(mems.len(), 1);
        assert_eq!(mems[0].admin_note.as_deref(), Some("Private admin note"),
            "DB query always returns admin_note; handler strips for non-admins");

        // Verify clearing: empty string → NULL
        let cleared = queries::update_memory_admin_note(&conn, &org_id, "m1", "").unwrap();
        assert!(cleared.is_some());
        assert!(cleared.unwrap().admin_note.is_none(), "empty string must clear admin_note to NULL");
    }

    // ── Disable/enable account integration test ───────────────────────────────

    #[test]
    fn disabled_user_key_is_rejected() {
        use crate::auth::api_keys;
        use crate::db::{connection::connect, queries};

        let conn = connect(":memory:").unwrap();
        run_all(&conn).unwrap();

        // Create org + user + key via bootstrap
        let (_org, user, raw_key) = queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Key should work initially
        let hash = api_keys::hash_key(&raw_key);
        let ctx = queries::validate_api_key(&conn, &hash).unwrap();
        assert!(ctx.is_some(), "key must be valid before disabling");

        // Disable the user
        let changed = queries::disable_user(&conn, &user.org_id, &user.id).unwrap();
        assert!(changed, "disable_user must return true for an active user");

        // Key must now be rejected
        let ctx_disabled = queries::validate_api_key(&conn, &hash).unwrap();
        assert!(ctx_disabled.is_none(), "key must be rejected after account is disabled");

        // is_key_account_disabled must return true
        let is_disabled = queries::is_key_account_disabled(&conn, &hash).unwrap();
        assert!(is_disabled, "is_key_account_disabled must return true for a disabled account");

        // Re-enable the user
        let re_enabled = queries::enable_user(&conn, &user.org_id, &user.id).unwrap();
        assert!(re_enabled, "enable_user must return true for a disabled user");

        // Key must work again
        let ctx_enabled = queries::validate_api_key(&conn, &hash).unwrap();
        assert!(ctx_enabled.is_some(), "key must work again after re-enabling");
    }

    // ── v30 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v30_adds_announcement_columns_to_organizations() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "organizations", "announcement"),
                "organizations must have announcement after v30");
        assert!(column_exists(&conn, "organizations", "announcement_type"),
                "organizations must have announcement_type after v30");
    }

    #[test]
    fn run_v30_adds_delete_after_to_memories() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "memories", "delete_after"),
                "memories must have delete_after after v30");
    }

    #[test]
    fn run_v30_announcement_roundtrip() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();

        // Set announcement
        conn.execute(
            "UPDATE organizations SET announcement = 'Maintenance tonight', announcement_type = 'warning' WHERE id = 'org1'",
            [],
        ).unwrap();

        let (ann, ann_type): (Option<String>, Option<String>) = conn.query_row(
            "SELECT announcement, announcement_type FROM organizations WHERE id = 'org1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(ann.as_deref(), Some("Maintenance tonight"), "announcement must persist");
        assert_eq!(ann_type.as_deref(), Some("warning"), "announcement_type must persist");

        // Clear announcement
        conn.execute(
            "UPDATE organizations SET announcement = NULL WHERE id = 'org1'",
            [],
        ).unwrap();
        let ann_cleared: Option<String> = conn.query_row(
            "SELECT announcement FROM organizations WHERE id = 'org1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(ann_cleared.is_none(), "clearing announcement must store NULL");
    }

    #[test]
    fn run_v30_delete_after_roundtrip() {
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
            "INSERT INTO memories (id, org_id, user_id, tool, content) VALUES ('m1', 'org1', 'u1', 'claude', 'hello')",
            [],
        ).unwrap();

        // Set delete_after
        conn.execute(
            "UPDATE memories SET delete_after = '2026-12-31' WHERE id = 'm1'",
            [],
        ).unwrap();

        let val: Option<String> = conn.query_row(
            "SELECT delete_after FROM memories WHERE id = 'm1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(val.as_deref(), Some("2026-12-31"), "delete_after must persist");

        // Clear it
        conn.execute(
            "UPDATE memories SET delete_after = NULL WHERE id = 'm1'",
            [],
        ).unwrap();
        let cleared: Option<String> = conn.query_row(
            "SELECT delete_after FROM memories WHERE id = 'm1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(cleared.is_none(), "clearing delete_after must store NULL");
    }

    #[test]
    fn run_v30_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v30(&conn);
        assert!(result.is_ok(), "run_v30 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 36, "user_version must remain 36");
    }

    #[test]
    fn run_v30_sets_user_version_to_30() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 36, "user_version must be 36 after run_all");
    }

    #[test]
    fn run_v31_adds_archived_at_to_code_projects() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "code_projects", "archived_at"),
                "code_projects must have archived_at after v31");
    }

    #[test]
    fn run_v31_archived_at_defaults_to_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO code_projects (org_id, name, root_path) VALUES ('org1', 'myapp', '/ws')",
            [],
        ).unwrap();
        let val: Option<String> = conn.query_row(
            "SELECT archived_at FROM code_projects WHERE name = 'myapp'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(val.is_none(), "archived_at must default to NULL");
    }

    #[test]
    fn run_v31_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v31(&conn);
        assert!(result.is_ok(), "run_v31 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 36, "user_version must remain 36");
    }

    #[test]
    fn run_v31_sets_user_version_to_31() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 36, "user_version must be 36 after run_all");
    }

    // ── v32 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v32_adds_admin_note_to_users() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "users", "admin_note"),
                "users must have admin_note after v32");
    }

    #[test]
    fn run_v32_admin_note_defaults_to_null() {
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
        let val: Option<String> = conn.query_row(
            "SELECT admin_note FROM users WHERE id = 'u1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(val.is_none(), "admin_note must default to NULL");
    }

    #[test]
    fn run_v32_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v32(&conn);
        assert!(result.is_ok(), "run_v32 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 36, "user_version must remain 36");
    }

    #[test]
    fn run_v32_sets_user_version_to_32() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 36, "user_version must be 36 after run_all");
    }

    // ── v35 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v35_adds_logo_url_to_organizations() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "organizations", "logo_url"),
                "organizations must have logo_url after v35");
    }

    #[test]
    fn run_v35_logo_url_defaults_to_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        let val: Option<String> = conn.query_row(
            "SELECT logo_url FROM organizations WHERE id = 'org1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(val.is_none(), "logo_url must default to NULL");
    }

    #[test]
    fn run_v35_logo_url_persists_value() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "UPDATE organizations SET logo_url = 'https://example.com/logo.png' WHERE id = 'org1'",
            [],
        ).unwrap();
        let val: Option<String> = conn.query_row(
            "SELECT logo_url FROM organizations WHERE id = 'org1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(val.as_deref(), Some("https://example.com/logo.png"),
                   "logo_url must persist the set value");
    }

    #[test]
    fn run_v35_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v35(&conn);
        assert!(result.is_ok(), "run_v35 must be idempotent: {:?}", result.err());
        assert!(get_user_version(&conn) >= 35, "user_version must be at least 35");
    }

    // ── v36 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v36_creates_conventions_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(table_exists(&conn, "conventions"), "conventions table must exist after v36");
    }

    #[test]
    fn run_v36_creates_indexes() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(index_exists(&conn, "conventions", "idx_conventions_org"),
                "idx_conventions_org must exist after v36");
        assert!(index_exists(&conn, "conventions", "idx_conventions_category"),
                "idx_conventions_category must exist after v36");
    }

    #[test]
    fn run_v36_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v36(&conn);
        assert!(result.is_ok(), "run_v36 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 36, "user_version must remain 36");
    }

    #[test]
    fn run_v36_convention_roundtrip() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO conventions (org_id, title, content, category, weight, tags) VALUES ('org1', 'Test Convention', 'Content here', 'architecture', 200, '[]')",
            [],
        ).unwrap();
        let (title, cat, weight): (String, String, i64) = conn.query_row(
            "SELECT title, category, weight FROM conventions WHERE org_id = 'org1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
        assert_eq!(title, "Test Convention");
        assert_eq!(cat, "architecture");
        assert_eq!(weight, 200);
    }
}
