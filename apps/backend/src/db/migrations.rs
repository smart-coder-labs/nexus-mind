use anyhow::Result;
use rusqlite::Connection;

/// Entry point called by main.rs. Runs all migrations in order.
pub fn run_all(conn: &Connection) -> Result<()> {
    run_v1(conn)?;
    run_v2(conn)?;
    run_v3(conn)?;
    run_v4(conn)?;
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
    fn run_all_sets_user_version_to_4() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 4, "user_version must be 4 after run_all");
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
}
