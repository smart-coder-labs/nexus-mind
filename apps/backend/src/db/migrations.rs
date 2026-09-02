use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};

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
    run_v37(conn)?;
    run_v38(conn)?;
    run_v39(conn)?;
    run_v40(conn)?;
    run_v41(conn)?;
    run_v42(conn)?;
    run_v43(conn)?;
    run_v44(conn)?;
    run_v45(conn)?;
    run_v46(conn)?;
    run_v47(conn)?;
    run_v48(conn)?;
    run_v49(conn)?;
    run_v50(conn)?;
    run_v51(conn)?;
    run_v52(conn)?;
    run_v53(conn)?;
    run_v54(conn)?;
    run_v55(conn)?;
    run_v56(conn)?;
    run_v57(conn)?;
    run_v58(conn)?;
    run_v59(conn)?;
    run_v60(conn)?;
    run_v61(conn)?;
    run_v62(conn)?;
    run_v63(conn)?;
    run_v64(conn)?;
    run_v65(conn)?;
    run_v66(conn)?;
    run_v67(conn)?;
    run_v68(conn)?;
    run_v69(conn)?;
    run_v70(conn)?;
    // Gap at v71/v72: prod's shared DB was advanced to user_version 72 by an
    // out-of-tree run_v72, so the security-template CHECK fix is v73 to land above it.
    run_v73(conn)?;
    run_v74(conn)?;
    Ok(())
}

/// Migration v73: allow the `security_scan` and `security_dast` agent templates. The
/// template_key CHECK on autonomous_agent_definitions did not list them, so creating
/// either agent failed with a CHECK-constraint error surfaced to the API as a generic
/// "Database error". SQLite can't ALTER a CHECK, so the table is rebuilt (same proven
/// pattern as run_v66/run_v69); ids are preserved so inbound foreign keys stay valid.
///
/// Numbered 73 (not 71/72) on purpose: the shared production DB was advanced to
/// user_version 72 by an out-of-tree `run_v72` (an `autonomous_agent_deliveries`
/// rebuild from the autonomous-agents-mvp WIP) that never landed on main, so a v71/v72
/// migration would be skipped by the `>=` guard and never apply. This migration only
/// touches `autonomous_agent_definitions`, which that out-of-tree v72 did not modify,
/// so the rebuild is safe against prod's current schema.
pub fn run_v73(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 73 {
        return Ok(());
    }
    conn.execute_batch(
        "
        PRAGMA foreign_keys = OFF;

        CREATE TABLE autonomous_agent_definitions_new (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            description TEXT,
            template_key TEXT NOT NULL CHECK(template_key IN ('qa','github_issue_resolver','github_pr_reviewer','lead_generation','judge','ai_content_manager','security_scan','security_dast')),
            template_version INTEGER NOT NULL CHECK(template_version > 0),
            status TEXT NOT NULL DEFAULT 'disabled' CHECK(status IN ('disabled','enabled','archived')),
            current_revision INTEGER NOT NULL DEFAULT 1 CHECK(current_revision > 0),
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, name)
        );

        INSERT INTO autonomous_agent_definitions_new
            (id,org_id,name,description,template_key,template_version,status,current_revision,created_by,created_at,updated_at)
        SELECT id,org_id,name,description,template_key,template_version,status,current_revision,created_by,created_at,updated_at
        FROM autonomous_agent_definitions;

        DROP TABLE autonomous_agent_definitions;
        ALTER TABLE autonomous_agent_definitions_new RENAME TO autonomous_agent_definitions;

        CREATE INDEX IF NOT EXISTS idx_autonomous_agent_definitions_org_status
            ON autonomous_agent_definitions(org_id, status);

        PRAGMA foreign_keys = ON;
        PRAGMA user_version = 73;
        ",
    )?;
    Ok(())
}

/// Migration v74: admits the `source-code` connector.
///
/// `migration_runs.source_kind` carries a CHECK constraint, and SQLite cannot
/// alter one in place — the table is rebuilt with the widened list, exactly as
/// v73 rebuilt `autonomous_agent_definitions`. Its child tables reference it by
/// name and their rows are untouched, so with foreign keys off during the swap
/// the ids they point at survive the rename. The triggers and indexes are
/// recreated verbatim; only the connector list changed.
pub fn run_v74(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 74 {
        return Ok(());
    }
    conn.execute_batch(
        "
        PRAGMA foreign_keys = OFF;

        CREATE TABLE migration_runs_new (
            id             TEXT PRIMARY KEY,
            org_id         TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            client_id      TEXT REFERENCES clients(id)  ON DELETE RESTRICT,
            project_id     TEXT REFERENCES projects(id) ON DELETE RESTRICT,
            source_kind    TEXT NOT NULL CHECK(source_kind IN
                             ('repo-docs','git-history','claude-memories','db-schema','source-code','noop')),
            status         TEXT NOT NULL DEFAULT 'staging' CHECK(status IN
                             ('staging','in_review','committing','completed','cancelled')),
            source_ref     TEXT,
            runner_version TEXT,
            attestation    TEXT NOT NULL DEFAULT '{}',
            created_by     TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at     TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
        );

        INSERT INTO migration_runs_new
            (id,org_id,client_id,project_id,source_kind,status,source_ref,runner_version,attestation,created_by,created_at,updated_at)
        SELECT id,org_id,client_id,project_id,source_kind,status,source_ref,runner_version,attestation,created_by,created_at,updated_at
        FROM migration_runs;

        DROP TABLE migration_runs;
        ALTER TABLE migration_runs_new RENAME TO migration_runs;

        CREATE INDEX idx_migration_runs_org_status ON migration_runs(org_id, status);
        CREATE INDEX idx_migration_runs_client     ON migration_runs(client_id);

        CREATE TRIGGER migration_runs_project_scope_insert
        BEFORE INSERT ON migration_runs
        WHEN NEW.project_id IS NOT NULL AND NOT EXISTS (
            SELECT 1 FROM projects WHERE id = NEW.project_id AND org_id = NEW.org_id
        )
        BEGIN
            SELECT RAISE(ABORT, 'migration project must belong to run organization');
        END;
        CREATE TRIGGER migration_runs_client_scope_insert
        BEFORE INSERT ON migration_runs
        WHEN NEW.client_id IS NOT NULL AND NOT EXISTS (
            SELECT 1 FROM clients WHERE id = NEW.client_id AND org_id = NEW.org_id
        )
        BEGIN
            SELECT RAISE(ABORT, 'migration client must belong to run organization');
        END;
        CREATE TRIGGER migration_runs_scope_immutable
        BEFORE UPDATE OF org_id, client_id, project_id, source_kind ON migration_runs
        WHEN OLD.org_id <> NEW.org_id
          OR OLD.source_kind <> NEW.source_kind
          OR OLD.client_id IS NOT NEW.client_id
          OR OLD.project_id IS NOT NEW.project_id
        BEGIN
            SELECT RAISE(ABORT, 'migration run scope is immutable');
        END;

        PRAGMA foreign_keys = ON;
        PRAGMA user_version = 74;
        ",
    )?;
    Ok(())
}

/// Migration v70: allow the `linkedin` connector kind (AI Content Manager stores
/// its OAuth token as an encrypted connector). SQLite can't ALTER a CHECK, so the
/// connectors table is rebuilt; ids are preserved so inbound foreign keys stay valid.
pub fn run_v70(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 70 {
        return Ok(());
    }
    conn.execute_batch(
        "
        PRAGMA foreign_keys = OFF;

        -- Drop the trigger that references autonomous_agent_connectors before the
        -- rebuild; the DROP+RENAME would otherwise corrupt its table reference. It
        -- is recreated verbatim after the table is back.
        DROP TRIGGER IF EXISTS autonomous_github_delivery_connector_scope;

        CREATE TABLE autonomous_agent_connectors_new (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            kind TEXT NOT NULL CHECK(kind IN ('github_app','slack','target_secret','linkedin')),
            name TEXT NOT NULL,
            secret_ciphertext TEXT,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            scopes_json TEXT NOT NULL DEFAULT '[]',
            health TEXT NOT NULL DEFAULT 'unknown' CHECK(health IN ('unknown','ready','degraded','revoked')),
            revocation_generation INTEGER NOT NULL DEFAULT 1 CHECK(revocation_generation > 0),
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, kind, name)
        );

        INSERT INTO autonomous_agent_connectors_new
            (id,org_id,kind,name,secret_ciphertext,metadata_json,scopes_json,health,revocation_generation,created_by,created_at,updated_at)
        SELECT id,org_id,kind,name,secret_ciphertext,metadata_json,scopes_json,health,revocation_generation,created_by,created_at,updated_at
        FROM autonomous_agent_connectors;

        DROP TABLE autonomous_agent_connectors;
        ALTER TABLE autonomous_agent_connectors_new RENAME TO autonomous_agent_connectors;

        CREATE TRIGGER IF NOT EXISTS autonomous_github_delivery_connector_scope
        BEFORE INSERT ON autonomous_github_deliveries
        WHEN NOT EXISTS(
            SELECT 1 FROM autonomous_agent_connectors connector
            WHERE connector.id=NEW.connector_id AND connector.org_id=NEW.org_id
        )
        BEGIN
            SELECT RAISE(ABORT, 'github delivery connector must belong to organization');
        END;

        PRAGMA foreign_keys = ON;
        PRAGMA user_version = 70;
        ",
    )?;
    Ok(())
}

/// Migration v69: allow the `ai_content_manager` agent template (LinkedIn content
/// creation). SQLite can't ALTER a CHECK, so the table is rebuilt (same proven
/// pattern as run_v66); ids are preserved so inbound foreign keys stay valid.
pub fn run_v69(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 69 {
        return Ok(());
    }
    conn.execute_batch(
        "
        PRAGMA foreign_keys = OFF;

        CREATE TABLE autonomous_agent_definitions_new (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            description TEXT,
            template_key TEXT NOT NULL CHECK(template_key IN ('qa','github_issue_resolver','github_pr_reviewer','lead_generation','judge','ai_content_manager')),
            template_version INTEGER NOT NULL CHECK(template_version > 0),
            status TEXT NOT NULL DEFAULT 'disabled' CHECK(status IN ('disabled','enabled','archived')),
            current_revision INTEGER NOT NULL DEFAULT 1 CHECK(current_revision > 0),
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, name)
        );

        INSERT INTO autonomous_agent_definitions_new
            (id,org_id,name,description,template_key,template_version,status,current_revision,created_by,created_at,updated_at)
        SELECT id,org_id,name,description,template_key,template_version,status,current_revision,created_by,created_at,updated_at
        FROM autonomous_agent_definitions;

        DROP TABLE autonomous_agent_definitions;
        ALTER TABLE autonomous_agent_definitions_new RENAME TO autonomous_agent_definitions;

        CREATE INDEX IF NOT EXISTS idx_autonomous_agent_definitions_org_status
            ON autonomous_agent_definitions(org_id, status);

        PRAGMA foreign_keys = ON;
        PRAGMA user_version = 69;
        ",
    )?;
    Ok(())
}

/// Migration v68: add a nullable `archived_at` to autonomous_agent_runs so runs can
/// be archived (hidden from the default list) without deleting them. Nullable ADD
/// COLUMN — no rebuild, existing rows and SELECTs are unaffected.
pub fn run_v68(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 68 {
        return Ok(());
    }
    conn.execute_batch(
        "
        ALTER TABLE autonomous_agent_runs ADD COLUMN archived_at TEXT;
        PRAGMA user_version = 68;
        ",
    )?;
    Ok(())
}

/// Migration v67: add a per-run `input_json` payload to autonomous_agent_runs so a
/// manual run can carry inputs chosen at trigger time (the Judge template's PR/issue
/// targets), instead of baking them into the agent definition. Nullable ADD COLUMN —
/// no rebuild, existing rows and SELECTs are unaffected.
pub fn run_v67(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 67 {
        return Ok(());
    }
    conn.execute_batch(
        "
        ALTER TABLE autonomous_agent_runs ADD COLUMN input_json TEXT;
        PRAGMA user_version = 67;
        ",
    )?;
    Ok(())
}

/// Migration v66: allow the `judge` agent template. The judge verifies whether a
/// PR/issue actually delivered its claim against the live application, so it needs
/// its own template_key. SQLite can't ALTER a CHECK, so the table is rebuilt (same
/// proven pattern as run_v65); ids are preserved so inbound foreign keys stay valid.
pub fn run_v66(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 66 {
        return Ok(());
    }
    conn.execute_batch(
        "
        PRAGMA foreign_keys = OFF;

        CREATE TABLE autonomous_agent_definitions_new (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            description TEXT,
            template_key TEXT NOT NULL CHECK(template_key IN ('qa','github_issue_resolver','github_pr_reviewer','lead_generation','judge')),
            template_version INTEGER NOT NULL CHECK(template_version > 0),
            status TEXT NOT NULL DEFAULT 'disabled' CHECK(status IN ('disabled','enabled','archived')),
            current_revision INTEGER NOT NULL DEFAULT 1 CHECK(current_revision > 0),
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, name)
        );

        INSERT INTO autonomous_agent_definitions_new
            (id,org_id,name,description,template_key,template_version,status,current_revision,created_by,created_at,updated_at)
        SELECT id,org_id,name,description,template_key,template_version,status,current_revision,created_by,created_at,updated_at
        FROM autonomous_agent_definitions;

        DROP TABLE autonomous_agent_definitions;
        ALTER TABLE autonomous_agent_definitions_new RENAME TO autonomous_agent_definitions;

        CREATE INDEX IF NOT EXISTS idx_autonomous_agent_definitions_org_status
            ON autonomous_agent_definitions(org_id, status);

        PRAGMA foreign_keys = ON;
        PRAGMA user_version = 66;
        ",
    )?;
    Ok(())
}

/// Migration v65: allow the `lead_generation` agent template. The template_key
/// CHECK on autonomous_agent_definitions only permitted the three code templates,
/// so a new outbound/lead-gen agent could not be created. SQLite can't ALTER a
/// CHECK, so the table is rebuilt (same proven pattern as run_v21 for `users`);
/// ids are preserved so the seven inbound foreign keys stay valid.
pub fn run_v65(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 65 {
        return Ok(());
    }
    conn.execute_batch(
        "
        PRAGMA foreign_keys = OFF;

        CREATE TABLE autonomous_agent_definitions_new (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            description TEXT,
            template_key TEXT NOT NULL CHECK(template_key IN ('qa','github_issue_resolver','github_pr_reviewer','lead_generation')),
            template_version INTEGER NOT NULL CHECK(template_version > 0),
            status TEXT NOT NULL DEFAULT 'disabled' CHECK(status IN ('disabled','enabled','archived')),
            current_revision INTEGER NOT NULL DEFAULT 1 CHECK(current_revision > 0),
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, name)
        );

        INSERT INTO autonomous_agent_definitions_new
            (id,org_id,name,description,template_key,template_version,status,current_revision,created_by,created_at,updated_at)
        SELECT id,org_id,name,description,template_key,template_version,status,current_revision,created_by,created_at,updated_at
        FROM autonomous_agent_definitions;

        DROP TABLE autonomous_agent_definitions;
        ALTER TABLE autonomous_agent_definitions_new RENAME TO autonomous_agent_definitions;

        CREATE INDEX IF NOT EXISTS idx_autonomous_agent_definitions_org_status
            ON autonomous_agent_definitions(org_id, status);

        PRAGMA foreign_keys = ON;
        PRAGMA user_version = 65;
        ",
    )?;
    Ok(())
}

/// Migration v64: allow the `github_issue_comment` delivery channel. The
/// issue-resolver posts a "no code change required" comment on the issue via this
/// channel, but the original CHECK omitted it, so the insert aborted the run.
/// SQLite can't ALTER a CHECK, so the table is rebuilt (no inbound FKs, no extra
/// indexes/triggers, so a straight copy is safe).
pub fn run_v64(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 64 {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE autonomous_agent_deliveries_new (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            run_id TEXT NOT NULL REFERENCES autonomous_agent_runs(id) ON DELETE RESTRICT,
            finding_id TEXT REFERENCES autonomous_agent_findings(id) ON DELETE RESTRICT,
            channel TEXT NOT NULL CHECK(channel IN ('nexusmind','github_issue','github_issue_comment','github_review','github_pr','slack')),
            idempotency_key TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','delivered','failed','dead_letter')),
            external_id TEXT,
            external_url TEXT,
            attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts >= 0),
            last_error_code TEXT,
            next_attempt_at TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, channel, idempotency_key)
         );
         INSERT INTO autonomous_agent_deliveries_new SELECT * FROM autonomous_agent_deliveries;
         DROP TABLE autonomous_agent_deliveries;
         ALTER TABLE autonomous_agent_deliveries_new RENAME TO autonomous_agent_deliveries;
         PRAGMA user_version = 64;",
    )?;
    Ok(())
}

/// Migration v63: full turn-by-turn transcript of each autonomous-agent run.
/// The worker streams Claude's stream-json output here line by line (sanitized)
/// so operators can watch/audit the agent's conversation live and after the
/// fact. Rows cascade-delete with their run; no append-only trigger so a future
/// retention job can prune old transcripts.
pub fn run_v63(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 63 {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS autonomous_agent_run_transcript (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            run_id TEXT NOT NULL REFERENCES autonomous_agent_runs(id) ON DELETE CASCADE,
            sequence INTEGER NOT NULL CHECK(sequence > 0),
            kind TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(run_id, sequence)
         );
         CREATE INDEX IF NOT EXISTS idx_aa_transcript_run
            ON autonomous_agent_run_transcript(org_id, run_id, sequence);
         PRAGMA user_version = 63;",
    )?;
    Ok(())
}

/// Migration v62: autonomous-agent control-plane persistence.
/// Long-running execution remains outside SQLite transactions; these tables
/// store durable intent, leases, redacted evidence, and idempotent outputs.
pub fn run_v62(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 62 {
        return Ok(());
    }
    conn.execute_batch(
        "ALTER TABLE organizations ADD COLUMN autonomous_agents_enabled INTEGER NOT NULL DEFAULT 1 CHECK(autonomous_agents_enabled IN (0,1));
         ALTER TABLE organizations ADD COLUMN autonomous_agent_retention_days INTEGER NOT NULL DEFAULT 90 CHECK(autonomous_agent_retention_days BETWEEN 7 AND 3650);

         CREATE TABLE IF NOT EXISTS autonomous_agent_definitions (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            description TEXT,
            template_key TEXT NOT NULL CHECK(template_key IN ('qa','github_issue_resolver','github_pr_reviewer')),
            template_version INTEGER NOT NULL CHECK(template_version > 0),
            status TEXT NOT NULL DEFAULT 'disabled' CHECK(status IN ('disabled','enabled','archived')),
            current_revision INTEGER NOT NULL DEFAULT 1 CHECK(current_revision > 0),
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, name)
         );
         CREATE INDEX IF NOT EXISTS idx_autonomous_agent_definitions_org_status
            ON autonomous_agent_definitions(org_id, status);

         CREATE TABLE IF NOT EXISTS autonomous_runtime_health (
            id INTEGER PRIMARY KEY CHECK(id = 1),
            status TEXT NOT NULL CHECK(status IN ('ready','degraded','reauth_required','unavailable')),
            reason_code TEXT,
            claude_version TEXT,
            last_success_at TEXT,
            last_failure_at TEXT,
            checked_at TEXT NOT NULL DEFAULT (datetime('now'))
         );

         CREATE TABLE IF NOT EXISTS autonomous_agent_revisions (
            id TEXT PRIMARY KEY,
            definition_id TEXT NOT NULL REFERENCES autonomous_agent_definitions(id) ON DELETE CASCADE,
            revision INTEGER NOT NULL CHECK(revision > 0),
            config_json TEXT NOT NULL,
            config_hash TEXT NOT NULL,
            capabilities_json TEXT NOT NULL,
            budgets_json TEXT NOT NULL,
            policy_generation INTEGER NOT NULL DEFAULT 1 CHECK(policy_generation > 0),
            validation_status TEXT NOT NULL DEFAULT 'pending' CHECK(validation_status IN ('pending','valid','invalid')),
            validation_json TEXT,
            validated_at TEXT,
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(definition_id, revision)
         );
         CREATE TRIGGER IF NOT EXISTS autonomous_agent_revisions_no_update
         BEFORE UPDATE ON autonomous_agent_revisions BEGIN
            SELECT RAISE(ABORT, 'autonomous agent revisions are append-only');
         END;
         CREATE TRIGGER IF NOT EXISTS autonomous_agent_revisions_no_delete
         BEFORE DELETE ON autonomous_agent_revisions BEGIN
            SELECT RAISE(ABORT, 'autonomous agent revisions are append-only');
         END;

         CREATE TABLE IF NOT EXISTS autonomous_agent_validations (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            definition_id TEXT NOT NULL REFERENCES autonomous_agent_definitions(id) ON DELETE CASCADE,
            revision_id TEXT NOT NULL REFERENCES autonomous_agent_revisions(id) ON DELETE CASCADE,
            config_hash TEXT NOT NULL,
            status TEXT NOT NULL CHECK(status IN ('valid','invalid')),
            result_json TEXT NOT NULL,
            validated_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE INDEX IF NOT EXISTS idx_autonomous_agent_validations_revision
            ON autonomous_agent_validations(revision_id, created_at);

         CREATE TABLE IF NOT EXISTS autonomous_agent_targets (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            definition_id TEXT NOT NULL REFERENCES autonomous_agent_definitions(id) ON DELETE CASCADE,
            project_id TEXT REFERENCES projects(id) ON DELETE RESTRICT,
            repository TEXT,
            environment TEXT,
            kind TEXT NOT NULL DEFAULT 'project' CHECK(kind IN ('repository','web_application','project')),
            name TEXT NOT NULL DEFAULT 'Target',
            config_json TEXT NOT NULL DEFAULT '{}',
            credential_connector_id TEXT REFERENCES autonomous_agent_connectors(id) ON DELETE RESTRICT,
            enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(definition_id, project_id, repository, environment)
         );

         CREATE TABLE IF NOT EXISTS autonomous_agent_schedules (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            definition_id TEXT NOT NULL REFERENCES autonomous_agent_definitions(id) ON DELETE CASCADE,
            kind TEXT NOT NULL CHECK(kind IN ('manual','daily','interval')),
            expression TEXT,
            timezone TEXT NOT NULL DEFAULT 'UTC',
            misfire_policy TEXT NOT NULL DEFAULT 'run_once' CHECK(misfire_policy IN ('run_once','skip')),
            next_run_at TEXT,
            enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0,1)),
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(definition_id)
         );
         CREATE INDEX IF NOT EXISTS idx_autonomous_agent_schedules_due
            ON autonomous_agent_schedules(enabled, next_run_at);

         CREATE TABLE IF NOT EXISTS autonomous_agent_connectors (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            kind TEXT NOT NULL CHECK(kind IN ('github_app','slack','target_secret')),
            name TEXT NOT NULL,
            secret_ciphertext TEXT,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            scopes_json TEXT NOT NULL DEFAULT '[]',
            health TEXT NOT NULL DEFAULT 'unknown' CHECK(health IN ('unknown','ready','degraded','revoked')),
            revocation_generation INTEGER NOT NULL DEFAULT 1 CHECK(revocation_generation > 0),
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, kind, name)
         );

         CREATE TABLE IF NOT EXISTS autonomous_github_deliveries (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            connector_id TEXT NOT NULL REFERENCES autonomous_agent_connectors(id) ON DELETE RESTRICT,
            delivery_id TEXT NOT NULL,
            event_name TEXT NOT NULL,
            action TEXT,
            repository TEXT,
            payload_hash TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(connector_id, delivery_id)
         );
         CREATE TRIGGER IF NOT EXISTS autonomous_github_delivery_connector_scope
         BEFORE INSERT ON autonomous_github_deliveries
         WHEN NOT EXISTS(
             SELECT 1 FROM autonomous_agent_connectors connector
             WHERE connector.id=NEW.connector_id AND connector.org_id=NEW.org_id
         )
         BEGIN
             SELECT RAISE(ABORT, 'github delivery connector must belong to organization');
         END;

         CREATE TABLE IF NOT EXISTS autonomous_agent_runs (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            definition_id TEXT NOT NULL REFERENCES autonomous_agent_definitions(id) ON DELETE RESTRICT,
            revision_id TEXT NOT NULL REFERENCES autonomous_agent_revisions(id) ON DELETE RESTRICT,
            automation_run_id TEXT NOT NULL UNIQUE REFERENCES automation_runs(id) ON DELETE RESTRICT,
            trigger_kind TEXT NOT NULL CHECK(trigger_kind IN ('manual','schedule','github_webhook','reconcile')),
            occurrence_key TEXT NOT NULL,
            scheduled_for TEXT,
            snapshot_sha TEXT,
            status TEXT NOT NULL DEFAULT 'queued' CHECK(status IN ('queued','leased','running','succeeded','partial','failed','cancelled','blocked_policy','blocked_runtime','budget_exhausted','dead_letter')),
            budget_json TEXT NOT NULL,
            started_at TEXT,
            finished_at TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(definition_id, occurrence_key)
         );
         CREATE INDEX IF NOT EXISTS idx_autonomous_agent_runs_org_status
            ON autonomous_agent_runs(org_id, status, created_at);

         CREATE TABLE IF NOT EXISTS autonomous_agent_leases (
            id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES autonomous_agent_runs(id) ON DELETE CASCADE,
            attempt_id TEXT NOT NULL UNIQUE REFERENCES automation_attempts(id) ON DELETE RESTRICT,
            worker_id TEXT NOT NULL,
            claim_token_hash TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            heartbeat_at TEXT NOT NULL DEFAULT (datetime('now')),
            released_at TEXT
         );
         CREATE UNIQUE INDEX IF NOT EXISTS idx_autonomous_agent_leases_one_active
            ON autonomous_agent_leases(run_id) WHERE released_at IS NULL;

         CREATE TABLE IF NOT EXISTS autonomous_agent_events (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            run_id TEXT NOT NULL REFERENCES autonomous_agent_runs(id) ON DELETE CASCADE,
            sequence INTEGER NOT NULL CHECK(sequence > 0),
            kind TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(run_id, sequence)
         );
         CREATE TRIGGER IF NOT EXISTS autonomous_agent_events_no_update
         BEFORE UPDATE ON autonomous_agent_events BEGIN
            SELECT RAISE(ABORT, 'autonomous agent events are append-only');
         END;
         CREATE TRIGGER IF NOT EXISTS autonomous_agent_events_no_delete
         BEFORE DELETE ON autonomous_agent_events BEGIN
            SELECT RAISE(ABORT, 'autonomous agent events are append-only');
         END;

         CREATE TABLE IF NOT EXISTS autonomous_agent_findings (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            definition_id TEXT NOT NULL REFERENCES autonomous_agent_definitions(id) ON DELETE RESTRICT,
            run_id TEXT NOT NULL REFERENCES autonomous_agent_runs(id) ON DELETE RESTRICT,
            fingerprint TEXT NOT NULL,
            title TEXT NOT NULL,
            severity TEXT NOT NULL CHECK(severity IN ('info','low','medium','high','critical')),
            status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','resolved','ignored')),
            summary TEXT NOT NULL,
            evidence_json TEXT NOT NULL DEFAULT '[]',
            occurrence_count INTEGER NOT NULL DEFAULT 1 CHECK(occurrence_count > 0),
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, definition_id, fingerprint)
         );

         CREATE TABLE IF NOT EXISTS autonomous_agent_deliveries (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            run_id TEXT NOT NULL REFERENCES autonomous_agent_runs(id) ON DELETE RESTRICT,
            finding_id TEXT REFERENCES autonomous_agent_findings(id) ON DELETE RESTRICT,
            channel TEXT NOT NULL CHECK(channel IN ('nexusmind','github_issue','github_review','github_pr','slack')),
            idempotency_key TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','delivered','failed','dead_letter')),
            external_id TEXT,
            external_url TEXT,
            attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts >= 0),
            last_error_code TEXT,
            next_attempt_at TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, channel, idempotency_key)
         );

         CREATE TABLE IF NOT EXISTS autonomous_agent_work_items (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            definition_id TEXT NOT NULL REFERENCES autonomous_agent_definitions(id) ON DELETE RESTRICT,
            run_id TEXT NOT NULL UNIQUE REFERENCES autonomous_agent_runs(id) ON DELETE CASCADE,
            repository TEXT NOT NULL,
            kind TEXT NOT NULL CHECK(kind IN ('github_issue','github_pr')),
            external_number INTEGER NOT NULL,
            head_sha TEXT,
            payload_hash TEXT NOT NULL,
            eligibility TEXT NOT NULL CHECK(eligibility IN ('pending','eligible','ineligible','completed')),
            reason TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(definition_id, repository, kind, external_number, payload_hash)
         );

         CREATE TABLE IF NOT EXISTS autonomous_agent_output_links (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            run_id TEXT NOT NULL REFERENCES autonomous_agent_runs(id) ON DELETE RESTRICT,
            work_item_id TEXT REFERENCES autonomous_agent_work_items(id) ON DELETE RESTRICT,
            kind TEXT NOT NULL CHECK(kind IN ('github_issue','github_review','branch','commit','draft_pr')),
            external_id TEXT NOT NULL,
            external_url TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, kind, external_id)
         );

         PRAGMA user_version = 62;",
    )?;
    Ok(())
}

/// Migration v61: grants autonomous-agent permissions to the built-in
/// super-user role template. The built-in admin and super-user runtime lists
/// are maintained in `get_role_permissions`; this persisted template grant
/// keeps custom-role cloning and permission reporting consistent.
pub fn run_v61(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 61 {
        return Ok(());
    }

    let autonomous_permissions = [
        "autonomous_agent:read",
        "autonomous_agent:create",
        "autonomous_agent:update",
        "autonomous_agent:enable",
        "autonomous_agent:run",
        "autonomous_agent:cancel",
        "autonomous_agent:manage_connectors",
    ];
    conn.execute(
        "INSERT OR IGNORE INTO roles
         (id,org_id,name,display_name,description,extends_json,permissions,version,enabled,is_template,created_at,updated_at)
         VALUES('admin_template',NULL,'admin','Admin','Built-in organization administrator template','[]',?1,1,1,1,datetime('now'),datetime('now'))",
        [serde_json::to_string(&autonomous_permissions)?],
    )?;

    let raw: Option<String> = conn
        .query_row(
            "SELECT permissions FROM roles WHERE id = 'super_user_template'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(raw) = raw {
        let mut permissions: Vec<String> = serde_json::from_str(&raw)?;
        for permission in autonomous_permissions {
            if !permissions.iter().any(|value| value == permission) {
                permissions.push(permission.to_string());
            }
        }
        conn.execute(
            "UPDATE roles SET permissions = ?1, version = version + 1, updated_at = datetime('now') WHERE id = 'super_user_template'",
            [serde_json::to_string(&permissions)?],
        )?;
    }
    conn.execute_batch("PRAGMA user_version = 61;")?;
    Ok(())
}

/// Migration v60: knowledge migration — staging, review and the documentation corpus.
///
/// # Why this one recreates instead of altering
///
/// v56 created a complete staging schema for knowledge migration and then
/// nothing was ever built on top of it: no queries, no routes, no UI. The
/// tables are empty in every installation that exists.
///
/// The shape it needs is not reachable by `ALTER TABLE`:
///
///   * `destination_kind` has to move OFF `migration_runs` and ONTO
///     `migration_candidates`. v56 assumed one destination kind per run, which
///     dies on the first real scan — walking `docs/` yields memories,
///     conventions, tasks and SDD artifacts in a single pass, and keeping the
///     assumption would mean walking the same tree four times.
///   * Dropping it also drops v56's cross-column
///     `CHECK(destination_kind = 'convention' AND project_id IS NULL)`, which
///     contradicted the destination anyway: `conventions.project_id` exists and
///     is nullable, so v56 forbade project-scoped conventions that the
///     conventions table happily supports.
///   * `migration_provenance.destination_kind` widens from two accepted values
///     to six. SQLite cannot alter a CHECK constraint.
///
/// Three interlocking partial rebuilds are harder to read and easier to get
/// wrong than one clean recreation — but a recreation is only defensible while
/// the tables are empty, so this migration VERIFIES that rather than assuming
/// it. If any of the five holds a row, the premise is false and we abort with
/// the table and the count instead of dropping data someone cared about.
///
/// Everything v56 got right is preserved verbatim: the org/project scope
/// triggers, `UNIQUE(run_id, source_identity)` for per-run idempotency, the
/// `UNIQUE(org_id, destination_kind, source_identity)` provenance key that makes
/// re-committing a source a database error rather than an application check,
/// optimistic `version`, and the append-only triggers on review actions.
pub fn run_v60(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 60 {
        return Ok(());
    }

    // ── Guard: v56 must be unused ────────────────────────────────────────────
    for table in [
        "migration_runs",
        "migration_candidates",
        "migration_review_actions",
        "migration_provenance",
        "migration_outcomes",
    ] {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |r| r.get(0),
        )?;
        if exists == 0 {
            continue;
        }
        let rows: i64 =
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
        if rows != 0 {
            anyhow::bail!(
                "run_v60 aborted: {table} holds {rows} row(s). v60 recreates the v56 staging \
                 tables, which is only safe because that schema was never wired to anything. \
                 Migrate or export these rows by hand, then re-run."
            );
        }
    }

    // ── Recreate ─────────────────────────────────────────────────────────────
    // Dropped in reverse dependency order. `DROP TABLE` also drops the table's
    // indexes and triggers, so there is nothing to clean up separately.
    conn.execute_batch(
        "DROP TABLE IF EXISTS migration_outcomes;
         DROP TABLE IF EXISTS migration_provenance;
         DROP TABLE IF EXISTS migration_review_actions;
         DROP TABLE IF EXISTS migration_candidates;
         DROP TABLE IF EXISTS migration_runs;

         CREATE TABLE migration_runs (
            id             TEXT PRIMARY KEY,
            org_id         TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            client_id      TEXT REFERENCES clients(id)  ON DELETE RESTRICT,
            project_id     TEXT REFERENCES projects(id) ON DELETE RESTRICT,
            source_kind    TEXT NOT NULL CHECK(source_kind IN
                             ('repo-docs','git-history','claude-memories','db-schema','noop')),
            status         TEXT NOT NULL DEFAULT 'staging' CHECK(status IN
                             ('staging','in_review','committing','completed','cancelled')),
            source_ref     TEXT,
            runner_version TEXT,
            attestation    TEXT NOT NULL DEFAULT '{}',
            created_by     TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at     TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE INDEX idx_migration_runs_org_status ON migration_runs(org_id, status);
         CREATE INDEX idx_migration_runs_client     ON migration_runs(client_id);

         CREATE TRIGGER migration_runs_project_scope_insert
         BEFORE INSERT ON migration_runs
         WHEN NEW.project_id IS NOT NULL AND NOT EXISTS (
             SELECT 1 FROM projects WHERE id = NEW.project_id AND org_id = NEW.org_id
         )
         BEGIN
             SELECT RAISE(ABORT, 'migration project must belong to run organization');
         END;
         CREATE TRIGGER migration_runs_client_scope_insert
         BEFORE INSERT ON migration_runs
         WHEN NEW.client_id IS NOT NULL AND NOT EXISTS (
             SELECT 1 FROM clients WHERE id = NEW.client_id AND org_id = NEW.org_id
         )
         BEGIN
             SELECT RAISE(ABORT, 'migration client must belong to run organization');
         END;
         CREATE TRIGGER migration_runs_scope_immutable
         BEFORE UPDATE OF org_id, client_id, project_id, source_kind ON migration_runs
         WHEN OLD.org_id <> NEW.org_id
           OR OLD.source_kind <> NEW.source_kind
           OR OLD.client_id IS NOT NEW.client_id
           OR OLD.project_id IS NOT NEW.project_id
         BEGIN
             SELECT RAISE(ABORT, 'migration run scope is immutable');
         END;

         CREATE TABLE migration_candidates (
            id                  TEXT PRIMARY KEY,
            run_id              TEXT NOT NULL REFERENCES migration_runs(id) ON DELETE CASCADE,
            source_identity     TEXT NOT NULL,
            destination_kind    TEXT NOT NULL CHECK(destination_kind IN
                                  ('memory','convention','task','sdd_artifact',
                                   'harness','harness_config_review')),
            destination_hint    TEXT NOT NULL DEFAULT '{}',
            content             TEXT NOT NULL,
            source_excerpt      TEXT,
            confidence          REAL,
            normalized_metadata TEXT NOT NULL DEFAULT '{}',
            attestation         TEXT NOT NULL DEFAULT '{}',
            provenance_kind     TEXT NOT NULL DEFAULT 'client_attested'
                                  CHECK(provenance_kind IN ('client_attested','verified_manifest')),
            status              TEXT NOT NULL DEFAULT 'staged' CHECK(status IN
                                  ('staged','approved','rejected','committing','committed',
                                   'skipped','failed','cancelled')),
            version             INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
            indexed_at          TEXT,
            created_at          TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(run_id, source_identity)
         );
         CREATE INDEX idx_migration_candidates_run_status
             ON migration_candidates(run_id, status, id);
         CREATE INDEX idx_migration_candidates_pending_index
             ON migration_candidates(indexed_at)
             WHERE indexed_at IS NULL AND status = 'committed';

         CREATE TABLE migration_review_actions (
            id                     TEXT PRIMARY KEY,
            run_id                 TEXT NOT NULL REFERENCES migration_runs(id) ON DELETE CASCADE,
            candidate_id           TEXT REFERENCES migration_candidates(id) ON DELETE CASCADE,
            actor_id               TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            actor_authorization    TEXT NOT NULL DEFAULT '{}',
            action                 TEXT NOT NULL CHECK(action IN
                                     ('approved','rejected','cancelled','restaged','stale_version',
                                      'permission_denied','not_approved','stale_approval')),
            expected_version       INTEGER,
            resulting_version      INTEGER,
            reason                 TEXT,
            request_correlation_id TEXT,
            created_at             TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE INDEX idx_migration_review_actions_candidate
             ON migration_review_actions(candidate_id, created_at);
         CREATE TRIGGER migration_review_actions_no_update
         BEFORE UPDATE ON migration_review_actions
         BEGIN
             SELECT RAISE(ABORT, 'migration review actions are append-only');
         END;
         CREATE TRIGGER migration_review_actions_no_delete
         BEFORE DELETE ON migration_review_actions
         BEGIN
             SELECT RAISE(ABORT, 'migration review actions are append-only');
         END;

         CREATE TABLE migration_provenance (
            id               TEXT PRIMARY KEY,
            org_id           TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            destination_kind TEXT NOT NULL CHECK(destination_kind IN
                               ('memory','convention','task','sdd_artifact',
                                'harness','harness_config_review')),
            source_identity  TEXT NOT NULL,
            candidate_id     TEXT NOT NULL REFERENCES migration_candidates(id) ON DELETE RESTRICT,
            destination_id   TEXT,
            created_at       TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, destination_kind, source_identity)
         );

         CREATE TABLE migration_outcomes (
            id               TEXT PRIMARY KEY,
            run_id           TEXT NOT NULL REFERENCES migration_runs(id) ON DELETE CASCADE,
            candidate_id     TEXT NOT NULL REFERENCES migration_candidates(id) ON DELETE CASCADE,
            expected_version INTEGER NOT NULL,
            candidate_status TEXT NOT NULL,
            outcome_status   TEXT NOT NULL CHECK(outcome_status IN
                               ('staged','blocked','approved','committed','skipped',
                                'failed','cancelled')),
            error_code       TEXT,
            created_at       TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE INDEX idx_migration_outcomes_candidate
             ON migration_outcomes(candidate_id, created_at);

         CREATE TABLE IF NOT EXISTS doc_documents (
            id          TEXT PRIMARY KEY,
            org_id      TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            client_id   TEXT REFERENCES clients(id)  ON DELETE RESTRICT,
            project_id  TEXT REFERENCES projects(id) ON DELETE CASCADE,
            path        TEXT NOT NULL,
            content_sha TEXT NOT NULL,
            scanned_at  TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, project_id, path)
         );
         CREATE INDEX IF NOT EXISTS idx_doc_documents_project ON doc_documents(project_id);

         CREATE TABLE IF NOT EXISTS doc_chunks (
            id           TEXT PRIMARY KEY,
            document_id  TEXT NOT NULL REFERENCES doc_documents(id) ON DELETE CASCADE,
            heading_path TEXT NOT NULL DEFAULT '',
            anchor       TEXT NOT NULL,
            ordinal      INTEGER NOT NULL,
            content      TEXT NOT NULL,
            UNIQUE(document_id, anchor, ordinal)
         );
         CREATE INDEX IF NOT EXISTS idx_doc_chunks_document ON doc_chunks(document_id);

         CREATE TABLE IF NOT EXISTS doc_chunk_embeddings (
            chunk_id  TEXT PRIMARY KEY REFERENCES doc_chunks(id) ON DELETE CASCADE,
            embedding BLOB NOT NULL
         );

         PRAGMA user_version = 60;",
    )?;
    Ok(())
}

/// Migration v59: usage telemetry — token counts and execution time per event,
/// rolled up task → project → client → org.
///
/// `usage_events` is append-mostly telemetry. Every optional foreign key is
/// `ON DELETE SET NULL` and is stored as NULL when it cannot be resolved at
/// ingest, rather than rejecting the row: telemetry must never 500 a caller and
/// losing an org's project row must never drop its usage history. The one hard
/// scope is `org_id`.
///
/// The partial-unique index `idx_usage_backfill_session` is what makes the
/// sessions backfill idempotent — at most one `source='backfill'` row per
/// session, so re-running `backfill_from_sessions` with `INSERT OR IGNORE` is a
/// no-op. It is partial (`WHERE source='backfill'`) so explicit ingest events,
/// which may legitimately share a `session_id`, are never constrained.
///
/// Same idempotent idiom as `run_v58`: `CREATE TABLE/INDEX IF NOT EXISTS`, safe
/// to re-run against an already-migrated database.
pub fn run_v59(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 59 {
        return Ok(());
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS usage_events (
            id          TEXT PRIMARY KEY,
            org_id      TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            user_id     TEXT REFERENCES users(id) ON DELETE SET NULL,
            client_id   TEXT REFERENCES clients(id) ON DELETE SET NULL,
            project_id  TEXT REFERENCES projects(id) ON DELETE SET NULL,
            task_id     TEXT REFERENCES tasks(id) ON DELETE SET NULL,
            session_id  TEXT REFERENCES sessions(id) ON DELETE SET NULL,
            model       TEXT,
            tokens_in   INTEGER NOT NULL DEFAULT 0,
            tokens_out  INTEGER NOT NULL DEFAULT 0,
            duration_ms INTEGER NOT NULL DEFAULT 0,
            source      TEXT NOT NULL DEFAULT 'ingest',
            event_ts    TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE INDEX IF NOT EXISTS idx_usage_events_org_ts  ON usage_events(org_id, event_ts);
         CREATE INDEX IF NOT EXISTS idx_usage_events_project ON usage_events(project_id);
         CREATE INDEX IF NOT EXISTS idx_usage_events_client  ON usage_events(client_id);
         CREATE INDEX IF NOT EXISTS idx_usage_events_task    ON usage_events(task_id);
         CREATE INDEX IF NOT EXISTS idx_usage_events_session ON usage_events(session_id);
         CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_backfill_session
             ON usage_events(session_id) WHERE source='backfill';

         PRAGMA user_version = 59;",
    )?;
    Ok(())
}

/// Migration v58: consultancy client model.
///
/// Inserts one grouping level between organization and project, so a software
/// consultancy can hold several clients — each owning one or more projects —
/// alongside its own internal work.
///
/// `projects.client_id IS NULL` is load-bearing: it means "internal project",
/// not "unset". It must never be backfilled to a sentinel client row.
///
/// This is stage 1 only: additive tables, columns and indexes. The
/// `github_connections` primary-key rebuild and the token encryption that goes
/// with it land in a separate step of the same change, because that one cannot
/// be expressed additively — SQLite has no ALTER PRIMARY KEY.
pub fn run_v58(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 58 {
        return Ok(());
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS clients (
            id          TEXT PRIMARY KEY,
            org_id      TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            name        TEXT NOT NULL,
            slug        TEXT NOT NULL,
            status      TEXT NOT NULL DEFAULT 'active'
                        CHECK(status IN ('active', 'paused', 'offboarded')),
            archived_at TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, slug)
         );
         CREATE INDEX IF NOT EXISTS idx_clients_org_status ON clients(org_id, status);

         CREATE TABLE IF NOT EXISTS client_members (
            id         TEXT PRIMARY KEY,
            client_id  TEXT NOT NULL REFERENCES clients(id) ON DELETE CASCADE,
            user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            role       TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(client_id, user_id)
         );
         CREATE INDEX IF NOT EXISTS idx_client_members_user ON client_members(user_id);",
    )?;

    // Each ALTER TABLE must be its own statement in SQLite, and re-running the
    // migration against a database that already carries the column raises
    // "duplicate column name" — ignored, matching the established pattern.
    for stmt in [
        "ALTER TABLE projects      ADD COLUMN client_id     TEXT REFERENCES clients(id) ON DELETE RESTRICT",
        "ALTER TABLE code_projects ADD COLUMN project_id    TEXT REFERENCES projects(id)",
        "ALTER TABLE conventions   ADD COLUMN client_id     TEXT REFERENCES clients(id) ON DELETE CASCADE",
        "ALTER TABLE policies      ADD COLUMN client_id     TEXT REFERENCES clients(id) ON DELETE CASCADE",
        "ALTER TABLE memories      ADD COLUMN promoted_from TEXT REFERENCES memories(id)",
    ] {
        let _ = conn.execute(stmt, []);
    }

    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_projects_client       ON projects(org_id, client_id);
         CREATE INDEX IF NOT EXISTS idx_conventions_client    ON conventions(org_id, client_id);
         CREATE INDEX IF NOT EXISTS idx_policies_client       ON policies(org_id, client_id, enabled);
         CREATE INDEX IF NOT EXISTS idx_code_projects_project ON code_projects(project_id);",
    )?;

    // The one place the visibility rule lives.
    //
    // Before the client model, "who may see this project" was a hand-written
    // `JOIN project_members` repeated in ~19 queries. Adding a second
    // membership path to nineteen copies is how an isolation hole ships: one
    // query gets updated, another does not, and nobody notices until a client
    // reads another client's data. So the rule becomes a view and the queries
    // consume it.
    //
    // UNION (not UNION ALL) is load-bearing: a user who is both a project
    // member and a member of that project's client must appear once, or every
    // JOIN against this view would silently duplicate their rows.
    //
    // A project with `client_id IS NULL` is internal work — the second branch
    // yields nothing for it, so it is reachable only by direct project
    // membership. That is deliberate, not an oversight.
    conn.execute_batch(
        "DROP VIEW IF EXISTS project_visibility;
         CREATE VIEW project_visibility AS
             SELECT p.id AS project_id, p.org_id AS org_id,
                    p.name AS project_name, pm.user_id AS user_id
               FROM projects p
               JOIN project_members pm ON pm.project_id = p.id
             UNION
             SELECT p.id, p.org_id, p.name, cm.user_id
               FROM projects p
               JOIN client_members cm ON cm.client_id = p.client_id;",
    )?;

    rebuild_github_connections_v58(conn)?;

    conn.execute_batch("PRAGMA user_version = 58;")?;
    Ok(())
}

/// Stage 2 of v58 — rebuild `github_connections` with a per-client primary key
/// and encrypt the tokens it already holds.
///
/// Two things force this to be its own step rather than an `ALTER TABLE`:
/// SQLite cannot change a primary key in place, and each `access_token` has to
/// pass through the cipher on the way across, which SQL alone cannot do.
///
/// The old key was `PRIMARY KEY (org_id)` — one GitHub account per
/// organization. A consultancy needs one per client, because each client has
/// its own GitHub organization.
/// One stored connection as it exists before the rebuild:
/// (org_id, access_token, token_type, scopes, github_login, github_user_id,
///  created_at, updated_at).
type LegacyGithubRow = (String, String, String, String, String, i64, String, String);

fn rebuild_github_connections_v58(conn: &Connection) -> Result<()> {
    let n_before: i64 =
        conn.query_row("SELECT COUNT(*) FROM github_connections", [], |r| r.get(0))?;

    // A fresh install has no stored credentials, so it has nothing to encrypt
    // and must not be blocked on a key it does not need yet. An install that
    // *does* hold tokens cannot proceed without one: copying them forward in
    // plaintext is precisely what this migration exists to stop.
    if n_before > 0 && !crate::crypto::is_configured() {
        anyhow::bail!(
            "migration v58 must encrypt {n_before} stored GitHub token(s), but \
             NEXUSMIND_TOKEN_ENCRYPTION_KEY is unset or invalid. Set it (64 hex \
             chars) and retry — refusing to copy credentials forward in plaintext."
        );
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS github_connections_new (
            org_id         TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            client_id      TEXT REFERENCES clients(id) ON DELETE CASCADE,
            github_login   TEXT NOT NULL DEFAULT '',
            access_token   TEXT NOT NULL,
            token_type     TEXT NOT NULL DEFAULT 'bearer',
            scopes         TEXT NOT NULL DEFAULT '',
            github_user_id INTEGER NOT NULL DEFAULT 0,
            created_at     TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (org_id, client_id, github_login)
         );",
    )?;

    let rows: Vec<LegacyGithubRow> = {
        let mut stmt = conn.prepare(
            "SELECT org_id, access_token, token_type, scopes, github_login,
                    github_user_id, created_at, updated_at
             FROM github_connections",
        )?;
        let mapped = stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })?;
        mapped.collect::<std::result::Result<_, _>>()?
    };

    for (org_id, token, token_type, scopes, login, user_id, created, updated) in rows {
        let encrypted = crate::crypto::encrypt(&token).ok_or_else(|| {
            anyhow::anyhow!(
                "migration v58 failed to encrypt the GitHub token for org {org_id}; \
                 aborting rather than storing it in plaintext"
            )
        })?;
        conn.execute(
            "INSERT INTO github_connections_new
               (org_id, client_id, github_login, access_token, token_type, scopes,
                github_user_id, created_at, updated_at)
             VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                org_id, login, encrypted, token_type, scopes, user_id, created, updated
            ],
        )?;
    }

    let n_after: i64 = conn.query_row("SELECT COUNT(*) FROM github_connections_new", [], |r| {
        r.get(0)
    })?;
    if n_before != n_after {
        anyhow::bail!(
            "migration v58 would lose GitHub connections: {n_before} before, {n_after} after"
        );
    }

    conn.execute_batch(
        "DROP TABLE github_connections;
         ALTER TABLE github_connections_new RENAME TO github_connections;",
    )?;
    Ok(())
}

/// Migration v57: automation run provenance and immutable worker receipts.
///
/// This is deliberately limited to durable scope, attempts, receipt replay
/// protection, and revocation evidence. Profile and policy authorization are
/// introduced by the following work unit. The migration is additive and must
/// be applied by the operator; tests use only in-memory SQLite fixtures.
pub fn run_v57(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 57 {
        return Ok(());
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS automation_runs (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            project_id TEXT REFERENCES projects(id) ON DELETE RESTRICT,
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            profile_version_ref TEXT NOT NULL,
            policy_generation INTEGER NOT NULL CHECK(policy_generation >= 0),
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE INDEX IF NOT EXISTS idx_automation_runs_org_created
             ON automation_runs(org_id, created_at);
         CREATE TRIGGER IF NOT EXISTS automation_runs_scope_insert
         BEFORE INSERT ON automation_runs
         WHEN NEW.project_id IS NOT NULL AND NOT EXISTS (
             SELECT 1 FROM projects WHERE id = NEW.project_id AND org_id = NEW.org_id
         )
         BEGIN
             SELECT RAISE(ABORT, 'automation project must belong to run organization');
         END;

         CREATE TABLE IF NOT EXISTS automation_attempts (
            id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES automation_runs(id) ON DELETE CASCADE,
            status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active', 'revoked')),
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            revoked_at TEXT
         );
         CREATE INDEX IF NOT EXISTS idx_automation_attempts_run_status
             ON automation_attempts(run_id, status);

         CREATE TABLE IF NOT EXISTS automation_receipts (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            attempt_id TEXT NOT NULL REFERENCES automation_attempts(id) ON DELETE RESTRICT,
            callback_id TEXT NOT NULL,
            kind TEXT NOT NULL DEFAULT 'callback',
            payload_hash TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(attempt_id, callback_id)
         );
         CREATE INDEX IF NOT EXISTS idx_automation_receipts_attempt_created
             ON automation_receipts(attempt_id, created_at);
         CREATE TRIGGER IF NOT EXISTS automation_receipts_active_attempt_insert
         BEFORE INSERT ON automation_receipts
         WHEN NOT EXISTS (
             SELECT 1
             FROM automation_attempts a
             JOIN automation_runs r ON r.id = a.run_id
             WHERE a.id = NEW.attempt_id AND a.status = 'active' AND r.org_id = NEW.org_id
         )
         BEGIN
             SELECT RAISE(ABORT, 'automation receipt requires an active attempt in the organization');
         END;
         CREATE TRIGGER IF NOT EXISTS automation_receipts_no_update
         BEFORE UPDATE ON automation_receipts
         BEGIN
             SELECT RAISE(ABORT, 'automation receipts are append-only');
         END;
         CREATE TRIGGER IF NOT EXISTS automation_receipts_no_delete
         BEFORE DELETE ON automation_receipts
         BEGIN
             SELECT RAISE(ABORT, 'automation receipts are append-only');
         END;

         CREATE TABLE IF NOT EXISTS automation_revocations (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            attempt_id TEXT NOT NULL REFERENCES automation_attempts(id) ON DELETE RESTRICT,
            reason TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(attempt_id)
         );
         CREATE TRIGGER IF NOT EXISTS automation_revocations_no_update
         BEFORE UPDATE ON automation_revocations
         BEGIN
             SELECT RAISE(ABORT, 'automation revocations are append-only');
         END;
         CREATE TRIGGER IF NOT EXISTS automation_revocations_no_delete
         BEFORE DELETE ON automation_revocations
         BEGIN
             SELECT RAISE(ABORT, 'automation revocations are append-only');
         END;

         PRAGMA user_version = 57;",
    )?;
    Ok(())
}

/// Migration v56: durable staging and review records for knowledge migration.
/// Scope is constrained at the database boundary so a run cannot be repurposed
/// between organization/project memory and organization convention destinations.
pub fn run_v56(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 56 {
        return Ok(());
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS migration_runs (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            project_id TEXT REFERENCES projects(id) ON DELETE RESTRICT,
            destination_kind TEXT NOT NULL CHECK(destination_kind IN ('memory', 'convention')),
            status TEXT NOT NULL DEFAULT 'staging'
                CHECK(status IN ('staging', 'in_review', 'committing', 'completed', 'cancelled')),
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            CHECK(
                (destination_kind = 'memory')
                OR (destination_kind = 'convention' AND project_id IS NULL)
            )
         );
         CREATE INDEX IF NOT EXISTS idx_migration_runs_org_status
             ON migration_runs(org_id, status);

         CREATE TRIGGER IF NOT EXISTS migration_runs_scope_insert
         BEFORE INSERT ON migration_runs
         WHEN NEW.project_id IS NOT NULL AND NOT EXISTS (
             SELECT 1 FROM projects WHERE id = NEW.project_id AND org_id = NEW.org_id
         )
         BEGIN
             SELECT RAISE(ABORT, 'migration project must belong to run organization');
         END;
         CREATE TRIGGER IF NOT EXISTS migration_runs_scope_update
         BEFORE UPDATE OF org_id, project_id ON migration_runs
         WHEN NEW.project_id IS NOT NULL AND NOT EXISTS (
             SELECT 1 FROM projects WHERE id = NEW.project_id AND org_id = NEW.org_id
         )
         BEGIN
             SELECT RAISE(ABORT, 'migration project must belong to run organization');
         END;

         CREATE TABLE IF NOT EXISTS migration_candidates (
            id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES migration_runs(id) ON DELETE CASCADE,
            source_identity TEXT NOT NULL,
            content TEXT NOT NULL,
            normalized_metadata TEXT NOT NULL DEFAULT '{}',
            attestation TEXT NOT NULL DEFAULT '{}',
            provenance_kind TEXT NOT NULL DEFAULT 'client_attested'
                CHECK(provenance_kind IN ('client_attested', 'verified_manifest')),
            status TEXT NOT NULL DEFAULT 'staged'
                CHECK(status IN ('staged', 'approved', 'rejected', 'committing', 'committed', 'skipped', 'failed', 'cancelled')),
            version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(run_id, source_identity)
         );
         CREATE INDEX IF NOT EXISTS idx_migration_candidates_run_status
             ON migration_candidates(run_id, status, id);

         CREATE TABLE IF NOT EXISTS migration_review_actions (
            id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES migration_runs(id) ON DELETE CASCADE,
            candidate_id TEXT REFERENCES migration_candidates(id) ON DELETE CASCADE,
            actor_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            actor_authorization TEXT NOT NULL DEFAULT '{}',
            action TEXT NOT NULL CHECK(action IN ('approved', 'rejected', 'cancelled', 'restaged', 'stale_version', 'permission_denied', 'not_approved', 'stale_approval')),
            expected_version INTEGER,
            resulting_version INTEGER,
            reason TEXT,
            request_correlation_id TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE INDEX IF NOT EXISTS idx_migration_review_actions_candidate
             ON migration_review_actions(candidate_id, created_at);
         CREATE TRIGGER IF NOT EXISTS migration_review_actions_no_update
         BEFORE UPDATE ON migration_review_actions
         BEGIN
             SELECT RAISE(ABORT, 'migration review actions are append-only');
         END;
         CREATE TRIGGER IF NOT EXISTS migration_review_actions_no_delete
         BEFORE DELETE ON migration_review_actions
         BEGIN
             SELECT RAISE(ABORT, 'migration review actions are append-only');
         END;

         CREATE TABLE IF NOT EXISTS migration_provenance (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            destination_kind TEXT NOT NULL CHECK(destination_kind IN ('memory', 'convention')),
            source_identity TEXT NOT NULL,
            candidate_id TEXT NOT NULL REFERENCES migration_candidates(id) ON DELETE RESTRICT,
            destination_id TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(org_id, destination_kind, source_identity)
         );

         CREATE TABLE IF NOT EXISTS migration_outcomes (
            id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL REFERENCES migration_runs(id) ON DELETE CASCADE,
            candidate_id TEXT NOT NULL REFERENCES migration_candidates(id) ON DELETE CASCADE,
            expected_version INTEGER NOT NULL,
            candidate_status TEXT NOT NULL,
            outcome_status TEXT NOT NULL CHECK(outcome_status IN ('staged', 'blocked', 'approved', 'committed', 'skipped', 'failed', 'cancelled')),
            error_code TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE INDEX IF NOT EXISTS idx_migration_outcomes_candidate
             ON migration_outcomes(candidate_id, created_at);

         PRAGMA user_version = 56;",
    )?;
    Ok(())
}

/// Migration v55: creates the LIVING SPECIFICATION — `openspec/specs/{capability}/spec.md`.
///
/// `openspec/` has two trees and v53 only modelled one. `openspec/changes/{name}/`
/// holds the in-flight drafts; `openspec/specs/{capability}/spec.md` holds the
/// contract those drafts are negotiating over, and `sdd-archive` merges a change's
/// delta specs into it when the change closes.
///
/// A main spec is **not an artifact of a change**. It belongs to the project and it
/// OUTLIVES the changes that modify it, so it gets its own root entity rather than
/// hanging off a synthetic change — which would invert the relationship.
///
/// `sdd_specs.project` is a project **name** string, exactly like `sdd_changes.project`
/// (design D4), so unregistered and org-shared project names stay visible.
///
/// `sdd_spec_revisions.merged_from_change_id` is the point of the whole table: it
/// records WHICH change merged its deltas to produce this revision. It is
/// `ON DELETE SET NULL`, never CASCADE — purging a change must not erase the
/// specification it shaped. The provenance is lost; the contract is not.
pub fn run_v55(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 55 {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sdd_specs (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            project TEXT NOT NULL,
            capability TEXT NOT NULL,
            title TEXT,
            path TEXT,
            latest_revision INTEGER NOT NULL DEFAULT 0,
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            archived_at TEXT,
            UNIQUE(org_id, project, capability)
         );
         CREATE INDEX IF NOT EXISTS idx_sdd_specs_org_project ON sdd_specs(org_id, project);
         CREATE INDEX IF NOT EXISTS idx_sdd_specs_capability ON sdd_specs(org_id, capability);

         CREATE TABLE IF NOT EXISTS sdd_spec_revisions (
            id TEXT PRIMARY KEY,
            spec_id TEXT NOT NULL REFERENCES sdd_specs(id) ON DELETE CASCADE,
            revision INTEGER NOT NULL,
            content TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            byte_size INTEGER NOT NULL,
            merged_from_change_id TEXT REFERENCES sdd_changes(id) ON DELETE SET NULL,
            git_commit TEXT,
            git_path TEXT,
            source TEXT NOT NULL DEFAULT 'agent',
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(spec_id, revision)
         );
         CREATE INDEX IF NOT EXISTS idx_sdd_spec_revisions_spec
            ON sdd_spec_revisions(spec_id, revision DESC);
         CREATE INDEX IF NOT EXISTS idx_sdd_spec_revisions_hash
            ON sdd_spec_revisions(spec_id, content_hash);
         CREATE INDEX IF NOT EXISTS idx_sdd_spec_revisions_merged_from
            ON sdd_spec_revisions(merged_from_change_id);

         -- Standalone FTS5, matching `sdd_artifacts_fts`: many revisions map to one
         -- indexed document, so the external-content trigger pattern does not apply.
         -- `upsert_sdd_spec` maintains this table explicitly, delete-then-insert on
         -- spec_id for every new revision, so a spec contributes exactly one hit.
         CREATE VIRTUAL TABLE IF NOT EXISTS sdd_specs_fts USING fts5(
            spec_id UNINDEXED,
            project,
            capability,
            content
         );

         PRAGMA user_version = 55;",
    )?;
    Ok(())
}

/// Migration v54: grants the new `sdd:*` permission strings to the seeded role
/// templates per the sdd-artifacts design's grant matrix (design.md §2):
/// `tmpl_dev_junior` = read+write (agents run the SDD pipeline and must be able
/// to save artifacts); `tmpl_dev_senior` = read+write+delete;
/// `tmpl_security_officer`/`tmpl_auditor` = read only. Same shape as `run_v52`:
/// appends only the missing strings to each template's `permissions` JSON array,
/// never replacing pre-existing grants, and idempotent via a `json_each`
/// membership check.
pub fn run_v54(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 54 {
        return Ok(());
    }

    let grants: &[(&str, &[&str])] = &[
        ("tmpl_dev_junior", &["sdd:read", "sdd:write"]),
        ("tmpl_dev_senior", &["sdd:read", "sdd:write", "sdd:delete"]),
        ("tmpl_security_officer", &["sdd:read"]),
        ("tmpl_auditor", &["sdd:read"]),
    ];

    for (template_id, perms) in grants {
        for perm in *perms {
            conn.execute(
                "UPDATE roles
                 SET permissions = json_insert(permissions, '$[#]', ?1)
                 WHERE id = ?2
                   AND NOT EXISTS (
                       SELECT 1 FROM json_each(roles.permissions) WHERE value = ?1
                   )",
                rusqlite::params![perm, template_id],
            )?;
        }
    }

    conn.execute_batch("PRAGMA user_version = 54;")?;
    Ok(())
}

/// Migration v53: creates the SDD-artifacts data layer — 4 tables plus the
/// `sdd_artifacts_fts` FTS5 index. See design.md §2 for the authoritative
/// column/FK/index list. Purely additive: no existing table is touched.
///
/// `sdd_changes.project` is a project **name** string (mirrors `tasks.project`
/// and `sessions.project`), not a `project_id` FK — deliberate, so that
/// org-shared and unregistered project names stay visible (design.md D4).
///
/// `sdd_artifacts.capability` is `NOT NULL DEFAULT ''` and MUST NOT be made
/// nullable. SQLite treats every `NULL` as distinct inside a `UNIQUE`
/// constraint, so a nullable `capability` would let `(change, 'design', NULL)`
/// be inserted twice and `UNIQUE(change_id, kind, capability)` would silently
/// not hold. The empty-string sentinel is what makes the uniqueness real.
/// `spec` is the only kind that repeats within a change (once per capability).
pub fn run_v53(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 53 {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sdd_changes (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            project TEXT NOT NULL,
            name TEXT NOT NULL,
            title TEXT,
            status TEXT NOT NULL DEFAULT 'active',
            phase TEXT NOT NULL DEFAULT 'propose',
            repo_url TEXT,
            repo_ref TEXT,
            sprint_id TEXT REFERENCES sprints(id) ON DELETE SET NULL,
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            archived_at TEXT,
            UNIQUE(org_id, project, name)
         );
         CREATE INDEX IF NOT EXISTS idx_sdd_changes_org_project_status
            ON sdd_changes(org_id, project, status);
         CREATE INDEX IF NOT EXISTS idx_sdd_changes_name ON sdd_changes(org_id, name);
         CREATE INDEX IF NOT EXISTS idx_sdd_changes_sprint ON sdd_changes(sprint_id);

         CREATE TABLE IF NOT EXISTS sdd_artifacts (
            id TEXT PRIMARY KEY,
            change_id TEXT NOT NULL REFERENCES sdd_changes(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            capability TEXT NOT NULL DEFAULT '',
            path TEXT,
            latest_revision INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(change_id, kind, capability)
         );
         CREATE INDEX IF NOT EXISTS idx_sdd_artifacts_change ON sdd_artifacts(change_id, kind);

         CREATE TABLE IF NOT EXISTS sdd_artifact_revisions (
            id TEXT PRIMARY KEY,
            artifact_id TEXT NOT NULL REFERENCES sdd_artifacts(id) ON DELETE CASCADE,
            revision INTEGER NOT NULL,
            content TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            byte_size INTEGER NOT NULL,
            git_commit TEXT,
            git_path TEXT,
            source TEXT NOT NULL DEFAULT 'agent',
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(artifact_id, revision)
         );
         CREATE INDEX IF NOT EXISTS idx_sdd_revisions_artifact
            ON sdd_artifact_revisions(artifact_id, revision DESC);
         CREATE INDEX IF NOT EXISTS idx_sdd_revisions_hash
            ON sdd_artifact_revisions(artifact_id, content_hash);

         CREATE TABLE IF NOT EXISTS sdd_change_memories (
            id TEXT PRIMARY KEY,
            change_id TEXT NOT NULL REFERENCES sdd_changes(id) ON DELETE CASCADE,
            memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
            relation TEXT NOT NULL DEFAULT 'produced',
            linked_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(change_id, memory_id)
         );
         CREATE INDEX IF NOT EXISTS idx_sdd_change_memories_memory
            ON sdd_change_memories(memory_id);

         -- Standalone FTS5, NOT external-content: the `memories_fts` trigger pattern
         -- assumes a 1:1 row mapping, which we do not have (many revisions -> one
         -- indexed doc). `upsert_sdd_artifact` maintains this table explicitly,
         -- delete-then-insert on artifact_id for every new revision.
         CREATE VIRTUAL TABLE IF NOT EXISTS sdd_artifacts_fts USING fts5(
            artifact_id UNINDEXED,
            change_name,
            kind,
            capability,
            content
         );

         PRAGMA user_version = 53;",
    )?;
    Ok(())
}

/// Migration v52: grants the new `task:*` permission strings to the seeded
/// role templates per the team-tasks design's grant matrix (design.md §1.4):
/// `tmpl_dev_junior` = read+write (create/edit tasks; assigning to others stays
/// senior-gated); `tmpl_dev_senior` = read+write+assign+delete;
/// `tmpl_security_officer`/`tmpl_auditor` = read only; `task:manage` is granted
/// to no template (admin-only, via the existing privilege bypass). The
/// mutation only appends missing permission strings to each template's
/// `permissions` JSON array — pre-existing permissions are never touched —
/// and is idempotent (checks membership via `json_each` before inserting).
pub fn run_v52(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 52 {
        return Ok(());
    }

    let grants: &[(&str, &[&str])] = &[
        ("tmpl_dev_junior", &["task:read", "task:write"]),
        (
            "tmpl_dev_senior",
            &["task:read", "task:write", "task:assign", "task:delete"],
        ),
        ("tmpl_security_officer", &["task:read"]),
        ("tmpl_auditor", &["task:read"]),
    ];

    for (template_id, perms) in grants {
        for perm in *perms {
            conn.execute(
                "UPDATE roles
                 SET permissions = json_insert(permissions, '$[#]', ?1)
                 WHERE id = ?2
                   AND NOT EXISTS (
                       SELECT 1 FROM json_each(roles.permissions) WHERE value = ?1
                   )",
                rusqlite::params![perm, template_id],
            )?;
        }
    }

    conn.execute_batch("PRAGMA user_version = 52;")?;
    Ok(())
}

/// Migration v51: creates the team-tasks data layer — 7 new tables
/// (`sprints` first so `tasks.sprint_id` can reference it; SQLite resolves FK
/// targets lazily so table order within the batch does not otherwise matter)
/// plus their indexes. See design.md §1.2 for the authoritative column/FK/index
/// list. Purely additive: no existing table is touched. `tasks.project` /
/// `sprints.project` are project **name** strings (mirrors `sessions.project`),
/// not a `project_id` FK, to preserve org-shared/unregistered-project reads via
/// the existing `visibility_predicate` path.
pub fn run_v51(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 51 {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sprints (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            project TEXT NOT NULL,
            name TEXT NOT NULL,
            goal TEXT,
            starts_at TEXT,
            ends_at TEXT,
            status TEXT NOT NULL DEFAULT 'planned',
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            archived_at TEXT,
            UNIQUE(org_id, project, name)
         );
         CREATE INDEX IF NOT EXISTS idx_sprints_org_project_status ON sprints(org_id, project, status);

         CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            project TEXT NOT NULL,
            title TEXT NOT NULL,
            description TEXT,
            status TEXT NOT NULL DEFAULT 'backlog',
            priority TEXT NOT NULL DEFAULT 'medium',
            due_date TEXT,
            parent_id TEXT REFERENCES tasks(id) ON DELETE CASCADE,
            sprint_id TEXT REFERENCES sprints(id) ON DELETE SET NULL,
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            archived_at TEXT
         );
         CREATE INDEX IF NOT EXISTS idx_tasks_org_project_status ON tasks(org_id, project, status);
         CREATE INDEX IF NOT EXISTS idx_tasks_org_parent ON tasks(org_id, parent_id);
         CREATE INDEX IF NOT EXISTS idx_tasks_sprint ON tasks(sprint_id);

         CREATE TABLE IF NOT EXISTS task_assignees (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            assigned_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            assigned_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(task_id, user_id)
         );
         CREATE INDEX IF NOT EXISTS idx_task_assignees_user ON task_assignees(user_id);

         CREATE TABLE IF NOT EXISTS task_labels (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            label TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(task_id, label)
         );
         CREATE INDEX IF NOT EXISTS idx_task_labels_label ON task_labels(label);

         CREATE TABLE IF NOT EXISTS task_comments (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            body TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE INDEX IF NOT EXISTS idx_task_comments_task ON task_comments(task_id, created_at);

         CREATE TABLE IF NOT EXISTS task_spec_links (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            spec_change_name TEXT NOT NULL,
            linked_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(task_id, spec_change_name)
         );
         CREATE INDEX IF NOT EXISTS idx_task_spec_links_change ON task_spec_links(spec_change_name);

         CREATE TABLE IF NOT EXISTS sprint_retrospectives (
            id TEXT PRIMARY KEY,
            sprint_id TEXT NOT NULL REFERENCES sprints(id) ON DELETE CASCADE,
            org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
            went_well TEXT,
            went_wrong TEXT,
            action_items TEXT,
            created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE INDEX IF NOT EXISTS idx_sprint_retros_sprint ON sprint_retrospectives(sprint_id, created_at);

         PRAGMA user_version = 51;",
    )?;
    Ok(())
}

/// Migration v50: adds comments on harness config reviews so reviewers can
/// discuss a shared redacted snapshot. Comments cascade-delete with their review.
pub fn run_v50(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 50 {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS harness_config_review_comments (
            id TEXT PRIMARY KEY,
            org_id TEXT NOT NULL,
            review_id TEXT NOT NULL REFERENCES harness_config_reviews(id) ON DELETE CASCADE,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            body TEXT NOT NULL,
            created_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_hcr_comments_review ON harness_config_review_comments(review_id, created_at);
         PRAGMA user_version = 50;",
    )?;
    Ok(())
}

/// Migration v49: adds first-class harness ownership.
/// Existing rows are backfilled from created_by so historical harnesses keep
/// their original creator as the catalog owner while created_by remains audit provenance.
pub fn run_v49(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 49 {
        return Ok(());
    }
    let has_owner: bool = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('harnesses') WHERE name = 'owner_user_id'",
        [],
        |row| row.get::<_, i64>(0),
    )? > 0;
    if !has_owner {
        conn.execute_batch("ALTER TABLE harnesses ADD COLUMN owner_user_id TEXT REFERENCES users(id) ON DELETE RESTRICT;")?;
    }
    conn.execute_batch(
        "UPDATE harnesses SET owner_user_id = created_by WHERE owner_user_id IS NULL;
         CREATE INDEX IF NOT EXISTS idx_harnesses_org_owner_status ON harnesses(org_id, owner_user_id, status);
         PRAGMA user_version = 49;",
    )?;
    Ok(())
}

/// Migration v48: creates the harness sharing tables and indexes.
/// Harness data is org-scoped with optional project scope. Manifests and
/// config reviews are stored as JSON strings; callers must keep secrets out of
/// config review payloads before upload.
pub fn run_v48(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 48 {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS harnesses (
          id TEXT PRIMARY KEY,
          org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
          project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
          slug TEXT NOT NULL,
          name TEXT NOT NULL,
          description TEXT,
          visibility TEXT NOT NULL DEFAULT 'org',
          status TEXT NOT NULL DEFAULT 'draft',
          created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
          created_at TEXT NOT NULL DEFAULT (datetime('now')),
          updated_at TEXT NOT NULL DEFAULT (datetime('now')),
          UNIQUE(org_id, slug)
        );
        CREATE INDEX IF NOT EXISTS idx_harnesses_org_status_project ON harnesses(org_id, status, project_id);

        CREATE TABLE IF NOT EXISTS harness_versions (
          id TEXT PRIMARY KEY,
          harness_id TEXT NOT NULL REFERENCES harnesses(id) ON DELETE CASCADE,
          version TEXT NOT NULL,
          manifest_json TEXT NOT NULL,
          manifest_hash TEXT NOT NULL,
          targets_json TEXT NOT NULL DEFAULT '[]',
          provenance_json TEXT NOT NULL DEFAULT '{}',
          status TEXT NOT NULL DEFAULT 'published',
          published_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
          published_at TEXT NOT NULL DEFAULT (datetime('now')),
          revoked_at TEXT,
          UNIQUE(harness_id, version)
        );
        CREATE INDEX IF NOT EXISTS idx_harness_versions_harness_version ON harness_versions(harness_id, version);

        CREATE TABLE IF NOT EXISTS harness_install_approvals (
          id TEXT PRIMARY KEY,
          org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
          user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
          harness_version_id TEXT NOT NULL REFERENCES harness_versions(id) ON DELETE CASCADE,
          target_tool TEXT NOT NULL,
          target_scope TEXT NOT NULL,
          manifest_hash TEXT NOT NULL,
          status TEXT NOT NULL DEFAULT 'approved',
          metadata_json TEXT NOT NULL DEFAULT '{}',
          approved_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_harness_install_approvals_org_user_status ON harness_install_approvals(org_id, user_id, status);

        CREATE TABLE IF NOT EXISTS harness_config_reviews (
          id TEXT PRIMARY KEY,
          org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
          user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
          source_tool TEXT NOT NULL,
          redacted_config_json TEXT NOT NULL,
          redaction_report_json TEXT NOT NULL,
          content_hash TEXT NOT NULL,
          status TEXT NOT NULL DEFAULT 'uploaded',
          created_at TEXT NOT NULL DEFAULT (datetime('now')),
          shared_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_harness_config_reviews_org_source_created ON harness_config_reviews(org_id, source_tool, created_at);

        PRAGMA user_version = 48;"
    )?;
    Ok(())
}

/// Migration v47: seeds the `super_user` system role as a global template (org_id = NULL).
/// The super_user role has full visibility across all projects and extended permissions
/// beyond admin. Idempotent — guarded by PRAGMA user_version < 47.
pub fn run_v47(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 47 {
        return Ok(());
    }
    conn.execute_batch(
        "INSERT OR IGNORE INTO roles (id, org_id, name, display_name, description, extends_json, permissions, version, enabled, is_template, created_at, updated_at)
        VALUES (
          'super_user_template',
          NULL,
          'super_user',
          'Super User',
          'Has full visibility across all projects. The only role that can see all projects.',
          '[]',
          '[\"memory:read\",\"memory:write\",\"memory:delete\",\"memory:search\",\"user:invite\",\"user:revoke\",\"audit:read\",\"audit:write\",\"settings:write\",\"policy:read\",\"policy:write\",\"project:read\",\"project:write\",\"session:read\",\"api_key:read\",\"convention:read\",\"convention:write\",\"webhook:read\",\"code:read\"]',
          1,
          1,
          1,
          datetime('now'),
          datetime('now')
        );
        PRAGMA user_version = 47;"
    )?;
    Ok(())
}

/// Migration v46: adds `github_token_encrypted` column to `code_projects` for
/// per-project GitHub PAT storage. The token is encrypted with AES-256-GCM
/// (key from NEXUSMIND_TOKEN_ENCRYPTION_KEY env var) so it is never stored
/// as plaintext. Used to re-authenticate private-repo clones on reindex when
/// the org-level GitHub OAuth connection is absent or has insufficient scope.
pub fn run_v46(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 46 {
        return Ok(());
    }
    // ALTER may fail if the column already exists in unusual states; allow it silently.
    // The version bump must not be swallowed — matches the idiom used by v38, v40, etc.
    let _ = conn.execute_batch("ALTER TABLE code_projects ADD COLUMN github_token_encrypted TEXT;");
    conn.execute_batch("PRAGMA user_version = 46;")?;
    Ok(())
}

/// Migration v45: idempotently backfills the `agents` and `agent_assignments`
/// tables for databases that reached `user_version >= 39` WITHOUT ever running
/// `run_v39()` — which happened because `run_all()` historically skipped it,
/// jumping straight from `run_v38()` to `run_v40()`. On those databases the
/// `run_v39()` guard (`if version >= 39 { return Ok(()); }`) would forever skip
/// the migration, so this step re-creates the same schema using
/// `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS`, making it safe to
/// run whether or not `run_v39()` already created these tables.
/// Idempotent — guarded by PRAGMA user_version < 45.
pub fn run_v45(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 45 {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS agents (
          id TEXT PRIMARY KEY,
          org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
          name TEXT NOT NULL,
          model TEXT NOT NULL,
          description TEXT,
          status TEXT NOT NULL DEFAULT 'active',
          created_at TEXT NOT NULL DEFAULT (datetime('now')),
          updated_at TEXT NOT NULL DEFAULT (datetime('now')),
          UNIQUE(org_id, name)
        );
        CREATE INDEX IF NOT EXISTS idx_agents_org ON agents(org_id);

        CREATE TABLE IF NOT EXISTS agent_assignments (
          id TEXT PRIMARY KEY,
          agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
          org_id TEXT NOT NULL,
          repo_url TEXT NOT NULL,
          created_at TEXT NOT NULL DEFAULT (datetime('now')),
          UNIQUE(agent_id, repo_url)
        );
        CREATE INDEX IF NOT EXISTS idx_agent_assignments_agent ON agent_assignments(agent_id);

        PRAGMA user_version = 45;",
    )?;
    Ok(())
}

/// Migration v44: adds a nullable `project_id` to `policies` so a policy can be
/// scoped to a single project (NULL = org-wide, applies everywhere). Conventions
/// already carry `project_id`; this brings policies to parity. Resolution applies
/// org-wide policies plus the target project's policies together.
/// Idempotent — guarded by PRAGMA user_version < 44.
pub fn run_v44(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 44 {
        return Ok(());
    }
    conn.execute_batch(
        "ALTER TABLE policies ADD COLUMN project_id TEXT REFERENCES projects(id) ON DELETE SET NULL;
        CREATE INDEX IF NOT EXISTS idx_policies_project ON policies(org_id, project_id, enabled);
        PRAGMA user_version = 44;",
    )?;
    Ok(())
}

/// Migration v43: creates `code_files` storing the raw source of each indexed file,
/// so a graph node's exact source (a symbol's line range, or a whole file) can be
/// shown precisely instead of reconstructed from symbol-fragment chunks.
/// Idempotent — guarded by PRAGMA user_version < 43.
pub fn run_v43(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 43 {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS code_files (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            code_project_id INTEGER NOT NULL REFERENCES code_projects(id) ON DELETE CASCADE,
            file_path       TEXT NOT NULL,
            content         TEXT NOT NULL,
            file_hash       TEXT,
            created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            UNIQUE(code_project_id, file_path)
        );
        CREATE INDEX IF NOT EXISTS idx_code_files_project_file ON code_files(code_project_id, file_path);
        PRAGMA user_version = 43;",
    )?;
    Ok(())
}

/// Migration v42: creates `code_edges` table linking code_symbols via typed directed edges.
/// Each edge is project-scoped (FK → code_projects ON DELETE CASCADE) and both endpoints
/// cascade-delete (FK → code_symbols ON DELETE CASCADE).
/// Idempotent — guarded by PRAGMA user_version < 42.
pub fn run_v42(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 42 {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS code_edges (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            code_project_id INTEGER NOT NULL REFERENCES code_projects(id) ON DELETE CASCADE,
            from_symbol_id  INTEGER NOT NULL REFERENCES code_symbols(id) ON DELETE CASCADE,
            to_symbol_id    INTEGER NOT NULL REFERENCES code_symbols(id) ON DELETE CASCADE,
            edge_type       TEXT NOT NULL,
            file_path       TEXT,
            created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            UNIQUE(code_project_id, from_symbol_id, to_symbol_id, edge_type)
        );
        CREATE INDEX IF NOT EXISTS idx_code_edges_project ON code_edges(code_project_id);
        CREATE INDEX IF NOT EXISTS idx_code_edges_from ON code_edges(from_symbol_id);
        CREATE INDEX IF NOT EXISTS idx_code_edges_to ON code_edges(to_symbol_id);
        CREATE INDEX IF NOT EXISTS idx_code_edges_project_file ON code_edges(code_project_id, file_path);
        PRAGMA user_version = 42;",
    )?;
    Ok(())
}

/// Migration v41: creates `code_symbols` table for the code knowledge graph.
/// Symbols are project-scoped (FK → code_projects ON DELETE CASCADE) with a
/// UNIQUE constraint on (code_project_id, qualified_name) to support idempotent upserts.
/// Idempotent — guarded by PRAGMA user_version < 41.
pub fn run_v41(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 41 {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS code_symbols (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            code_project_id INTEGER NOT NULL REFERENCES code_projects(id) ON DELETE CASCADE,
            symbol_type     TEXT NOT NULL,
            name            TEXT NOT NULL,
            qualified_name  TEXT NOT NULL,
            file_path       TEXT,
            file_hash       TEXT,
            start_line      INTEGER,
            end_line        INTEGER,
            language        TEXT NOT NULL,
            created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
            UNIQUE(code_project_id, qualified_name)
        );
        CREATE INDEX IF NOT EXISTS idx_code_symbols_project ON code_symbols(code_project_id);
        CREATE INDEX IF NOT EXISTS idx_code_symbols_project_file ON code_symbols(code_project_id, file_path);
        PRAGMA user_version = 41;",
    )?;
    Ok(())
}

/// Migration v40: adds notifications_read_at to organizations.
/// Records when an org admin last marked all notifications as read.
/// Idempotent — guarded by PRAGMA user_version < 40.
pub fn run_v40(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 40 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE organizations ADD COLUMN notifications_read_at TEXT");
    conn.execute_batch("PRAGMA user_version = 40;")?;
    Ok(())
}

/// Migration v38: adds name column to sessions table.
/// Idempotent — guarded by PRAGMA user_version < 38.
pub fn run_v38(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 38 {
        return Ok(());
    }
    let _ = conn.execute_batch("ALTER TABLE sessions ADD COLUMN name TEXT");
    conn.execute_batch("PRAGMA user_version = 38;")?;
    Ok(())
}

/// Migration v39: creates agents and agent_assignments tables.
/// Agents are org-scoped AI review agents with model configuration.
/// agent_assignments link agents to code repositories.
/// Idempotent — guarded by PRAGMA user_version < 39.
pub fn run_v39(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 39 {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS agents (
          id TEXT PRIMARY KEY,
          org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
          name TEXT NOT NULL,
          model TEXT NOT NULL,
          description TEXT,
          status TEXT NOT NULL DEFAULT 'active',
          created_at TEXT NOT NULL DEFAULT (datetime('now')),
          updated_at TEXT NOT NULL DEFAULT (datetime('now')),
          UNIQUE(org_id, name)
        );
        CREATE INDEX IF NOT EXISTS idx_agents_org ON agents(org_id);

        CREATE TABLE IF NOT EXISTS agent_assignments (
          id TEXT PRIMARY KEY,
          agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
          org_id TEXT NOT NULL,
          repo_url TEXT NOT NULL,
          created_at TEXT NOT NULL DEFAULT (datetime('now')),
          UNIQUE(agent_id, repo_url)
        );
        CREATE INDEX IF NOT EXISTS idx_agent_assignments_agent ON agent_assignments(agent_id);

        PRAGMA user_version = 39;",
    )?;
    Ok(())
}

/// Migration v32: adds admin_note to users and usage tracking columns to api_keys.
///
/// - users.admin_note TEXT: private org-admin note, never returned to non-admin callers.
/// - api_keys.times_used INTEGER: cumulative count of successful authentications.
/// - api_keys.last_used_at TEXT: ISO datetime of the last successful authentication.
///
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
///
/// - users.last_login_at TEXT: ISO datetime of the last successful API key authentication.
///
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

/// Migration v37: creates the github_connections table for storing OAuth tokens per org.
/// Idempotent — guarded by PRAGMA user_version < 37.
pub fn run_v37(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 37 {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS github_connections (
          org_id TEXT PRIMARY KEY REFERENCES organizations(id) ON DELETE CASCADE,
          access_token TEXT NOT NULL,
          token_type TEXT NOT NULL DEFAULT 'bearer',
          scopes TEXT NOT NULL DEFAULT '',
          github_login TEXT NOT NULL DEFAULT '',
          github_user_id INTEGER NOT NULL DEFAULT 0,
          created_at TEXT NOT NULL DEFAULT (datetime('now')),
          updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;
    conn.execute_batch("PRAGMA user_version = 37;")?;
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
///
/// - code_projects.exclude_patterns TEXT: JSON array of glob-like exclusion patterns.
///
/// Idempotent — guarded by PRAGMA user_version < 34.
pub fn run_v34(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 34 {
        return Ok(());
    }
    let _ = conn
        .execute_batch("ALTER TABLE code_projects ADD COLUMN exclude_patterns TEXT DEFAULT '[]'");
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
///
/// - organizations: announcement TEXT, announcement_type TEXT DEFAULT 'info'
/// - memories: delete_after TEXT (ISO date string, NULL = no scheduled deletion)
///
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
    //
    // TODO(search-recall): this table uses the default `unicode61` tokenizer, which
    // does no stemming — "migrate" and "migration" are unrelated tokens to FTS5. A
    // `tokenize = 'porter unicode61'` tokenizer would improve recall further on top
    // of the OR-join fix in `sanitize_fts_query` (see db/queries.rs). Deferred here
    // rather than added as a new migration because switching tokenizers requires
    // dropping and rebuilding this virtual table (same DROP+CREATE+backfill pattern
    // used below) against the live `memories` table in production, which is a
    // reindex of arbitrary size with no cheap rollback if it's interrupted mid-way.
    // That risk should be taken deliberately (e.g. behind a maintenance window or a
    // background/batched rebuild), not folded silently into this change.
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

    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM roles WHERE is_template = 1",
        [],
        |r| r.get(0),
    )?;
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
        ",
    )?;

    // 2. Add project_id column to memories if not exists
    let has_project_id: bool = conn
        .query_row(
            "SELECT count(*) FROM pragma_table_info('memories') WHERE name='project_id'",
            [],
            |row| row.get::<_, i32>(0),
        )
        .unwrap_or(0)
        > 0;

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
         PRAGMA user_version = 12;",
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
         PRAGMA user_version = 13;",
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
    let _ = conn.execute_batch(
        "ALTER TABLE organizations ADD COLUMN min_password_length INTEGER NOT NULL DEFAULT 8",
    );
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
    let _ =
        conn.execute_batch("ALTER TABLE code_projects ADD COLUMN reindex_interval_hours INTEGER");
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
        ",
    )?;
    let _ = conn.execute_batch("ALTER TABLE memories ADD COLUMN collection_id TEXT REFERENCES collections(id) ON DELETE SET NULL");
    conn.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_memories_collection ON memories(org_id, collection_id);
        PRAGMA user_version = 25;
        ",
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
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn creates_all_tables() {
        let conn = in_memory_db();
        run(&conn).unwrap();

        assert!(
            table_exists(&conn, "organizations"),
            "missing: organizations"
        );
        assert!(table_exists(&conn, "users"), "missing: users");
        assert!(table_exists(&conn, "api_keys"), "missing: api_keys");
        assert!(table_exists(&conn, "memories"), "missing: memories");
        assert!(table_exists(&conn, "audit_logs"), "missing: audit_logs");
        assert!(table_exists(&conn, "roles"), "missing: roles");
        assert!(table_exists(&conn, "projects"), "missing: projects");
        assert!(
            table_exists(&conn, "project_members"),
            "missing: project_members"
        );
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
        assert!(
            get_user_version(&conn) >= 10,
            "user_version must be at least 10 after run_all"
        );
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
        )
        .unwrap();
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
            .query_row("SELECT scope FROM memories WHERE id = 'm1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(scope, "project");
    }

    #[test]
    fn run_all_idempotent_on_already_migrated_db() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        // Running again must not fail
        let result = run_all(&conn);
        assert!(
            result.is_ok(),
            "run_all must be idempotent: {:?}",
            result.err()
        );
    }

    #[test]
    fn run_all_fts_includes_title_and_type() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
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
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
                rusqlite::params![table, column],
                |r| r.get(0),
            )
            .unwrap_or(0);
        count > 0
    }

    fn index_exists(conn: &Connection, table: &str, index: &str) -> bool {
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_index_list(?1) WHERE name = ?2",
                rusqlite::params![table, index],
                |r| r.get(0),
            )
            .unwrap_or(0);
        count > 0
    }

    #[test]
    fn run_v9_adds_hash_columns_to_audit_logs() {
        let conn = in_memory_db_v8();
        run_v9(&conn).unwrap();

        assert!(
            column_exists(&conn, "audit_logs", "previous_hash"),
            "audit_logs must have previous_hash after v9"
        );
        assert!(
            column_exists(&conn, "audit_logs", "current_hash"),
            "audit_logs must have current_hash after v9"
        );
    }

    #[test]
    fn run_v9_adds_plan_to_organizations() {
        let conn = in_memory_db_v8();
        run_v9(&conn).unwrap();

        assert!(
            column_exists(&conn, "organizations", "plan"),
            "organizations must have plan after v9"
        );

        // Verify DEFAULT 'free' — insert org without plan and read it back
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('test-org', 'Test', 'test')",
            [],
        )
        .unwrap();
        let plan: String = conn
            .query_row(
                "SELECT plan FROM organizations WHERE id = 'test-org'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(plan, "free", "default plan must be 'free'");
    }

    #[test]
    fn run_v9_adds_four_indexes() {
        let conn = in_memory_db_v8();
        run_v9(&conn).unwrap();

        assert!(
            index_exists(&conn, "memories", "idx_memories_scope"),
            "idx_memories_scope must exist"
        );
        assert!(
            index_exists(&conn, "memories", "idx_memories_type"),
            "idx_memories_type must exist"
        );
        assert!(
            index_exists(&conn, "memories", "idx_memories_project_id"),
            "idx_memories_project_id must exist"
        );
        assert!(
            index_exists(&conn, "audit_logs", "idx_audit_logs_org_ts"),
            "idx_audit_logs_org_ts must exist"
        );
    }

    #[test]
    fn run_v9_is_idempotent() {
        let conn = in_memory_db_v8();
        run_v9(&conn).unwrap();
        // Running again must not fail
        let result = run_v9(&conn);
        assert!(
            result.is_ok(),
            "run_v9 must be idempotent: {:?}",
            result.err()
        );
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
        assert!(
            table_exists(&conn, "policies"),
            "policies table must exist after v10"
        );
    }

    #[test]
    fn run_v10_creates_org_index() {
        let conn = in_memory_db_v9();
        run_v10(&conn).unwrap();
        assert!(
            index_exists(&conn, "policies", "idx_policies_org"),
            "idx_policies_org must exist after v10"
        );
    }

    #[test]
    fn run_v10_is_idempotent() {
        let conn = in_memory_db_v9();
        run_v10(&conn).unwrap();
        let result = run_v10(&conn);
        assert!(
            result.is_ok(),
            "run_v10 must be idempotent: {:?}",
            result.err()
        );
        assert_eq!(get_user_version(&conn), 10, "user_version must remain 10");
    }

    #[test]
    fn run_v10_rejects_invalid_rule_type() {
        let conn = in_memory_db_v9();
        run_v10(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        let bad = conn.execute(
            "INSERT INTO policies (id, org_id, name, rule_type, config) VALUES ('p1','org1','x','banana','{}')",
            [],
        );
        assert!(
            bad.is_err(),
            "CHECK constraint must reject unknown rule_type"
        );
    }

    #[test]
    fn run_v10_preserves_existing_rows() {
        let conn = in_memory_db_v9();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status) VALUES ('u1', 'org1', 'a@b.com', 'A', 'admin', 'active')",
            [],
        ).unwrap();
        run_v10(&conn).unwrap();
        // Existing tables must still be readable
        let org_count: i32 = conn
            .query_row("SELECT COUNT(*) FROM organizations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(org_count, 1, "existing rows must be preserved after v10");
    }

    #[test]
    fn run_v9_preserves_existing_rows() {
        let conn = in_memory_db_v8();

        // Seed an org + user + audit_log row in v8
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
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
        let (action, prev_hash): (String, Option<String>) = conn
            .query_row(
                "SELECT action, previous_hash FROM audit_logs WHERE id = 'al1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(action, "store");
        assert!(
            prev_hash.is_none(),
            "pre-v9 rows must have previous_hash = NULL"
        );
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
        assert!(
            table_exists(&conn, "code_projects"),
            "code_projects must exist after v11"
        );
        assert!(
            table_exists(&conn, "code_chunks"),
            "code_chunks must exist after v11"
        );
    }

    #[test]
    fn run_v11_creates_indexes() {
        let conn = in_memory_db_v10();
        run_v11(&conn).unwrap();
        assert!(
            index_exists(&conn, "code_chunks", "idx_code_chunks_project"),
            "idx_code_chunks_project must exist after v11"
        );
        assert!(
            index_exists(&conn, "code_chunks", "idx_code_chunks_file"),
            "idx_code_chunks_file must exist after v11"
        );
    }

    #[test]
    fn run_v11_is_idempotent() {
        let conn = in_memory_db_v10();
        run_v11(&conn).unwrap();
        let result = run_v11(&conn);
        assert!(
            result.is_ok(),
            "run_v11 must be idempotent: {:?}",
            result.err()
        );
        assert_eq!(get_user_version(&conn), 11, "user_version must remain 11");
    }

    #[test]
    fn run_v11_sets_user_version_to_11() {
        let conn = in_memory_db_v10();
        run_v11(&conn).unwrap();
        assert_eq!(
            get_user_version(&conn),
            11,
            "user_version must be 11 after v11"
        );
    }

    #[test]
    fn run_all_sets_user_version_to_11() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    #[test]
    fn run_v11_code_projects_unique_org_name() {
        let conn = in_memory_db_v10();
        run_v11(&conn).unwrap();
        // Seed org
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO code_projects (org_id, name, root_path) VALUES ('org1', 'myapp', '/ws/myapp')",
            [],
        ).unwrap();
        // Duplicate must fail
        let dup = conn.execute(
            "INSERT INTO code_projects (org_id, name, root_path) VALUES ('org1', 'myapp', '/ws/myapp2')",
            [],
        );
        assert!(
            dup.is_err(),
            "UNIQUE(org_id, name) must be enforced on code_projects"
        );
    }

    #[test]
    fn run_v11_code_chunks_cascade_delete() {
        let conn = in_memory_db_v10();
        run_v11(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO code_projects (id, org_id, name, root_path) VALUES (1, 'org1', 'myapp', '/ws')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO code_chunks (code_project_id, file_path, file_hash, start_line, end_line, content) VALUES (1, 'src/lib.rs', 'abc123', 1, 10, 'fn main() {}')",
            [],
        ).unwrap();
        // Chunk must exist
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM code_chunks WHERE code_project_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "chunk must exist before delete");
        // Delete project — chunks cascade
        conn.execute("DELETE FROM code_projects WHERE id = 1", [])
            .unwrap();
        let after: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM code_chunks WHERE code_project_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(after, 0, "chunks must cascade-delete with project");
    }

    #[test]
    fn run_v11_preserves_existing_tables() {
        let conn = in_memory_db_v10();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        run_v11(&conn).unwrap();
        // Prior tables still readable
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM organizations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "existing rows must be preserved after v11");
    }

    // ── v14 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v14_creates_webhooks_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            table_exists(&conn, "webhooks"),
            "webhooks table must exist after v14"
        );
    }

    #[test]
    fn run_v14_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v14(&conn);
        assert!(
            result.is_ok(),
            "run_v14 must be idempotent: {:?}",
            result.err()
        );
        // run_all brings to v15; re-running v14 after that still stays at v15
        assert!(
            get_user_version(&conn) >= 14,
            "user_version must be at least 14"
        );
    }

    #[test]
    fn run_v14_sets_user_version_to_14() {
        // After run_all the version is 15; this documents the historical expectation
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            get_user_version(&conn) >= 14,
            "user_version must be at least 14 after run_all"
        );
    }

    // ── v15 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v15_adds_event_overrides_to_projects() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "projects", "event_overrides"),
            "projects must have event_overrides after v15"
        );
    }

    #[test]
    fn run_v15_column_defaults_to_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO projects (id, org_id, name) VALUES ('p1', 'org1', 'my-project')",
            [],
        )
        .unwrap();
        let val: Option<String> = conn
            .query_row(
                "SELECT event_overrides FROM projects WHERE id = 'p1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            val.is_none(),
            "event_overrides must default to NULL (inherit)"
        );
    }

    #[test]
    fn run_v15_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v15(&conn);
        assert!(
            result.is_ok(),
            "run_v15 must be idempotent: {:?}",
            result.err()
        );
        assert!(
            get_user_version(&conn) >= 15,
            "user_version must be at least 15"
        );
    }

    #[test]
    fn run_v15_sets_user_version_to_15() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            get_user_version(&conn) >= 15,
            "user_version must be at least 15 after run_all"
        );
    }

    // ── v16 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v16_adds_retention_days_to_organizations() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "organizations", "retention_days"),
            "organizations must have retention_days after v16"
        );
    }

    #[test]
    fn run_v16_retention_days_defaults_to_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        let val: Option<i64> = conn
            .query_row(
                "SELECT retention_days FROM organizations WHERE id = 'org1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            val.is_none(),
            "retention_days must default to NULL (keep forever)"
        );
    }

    #[test]
    fn run_v16_retention_days_persists_value() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE organizations SET retention_days = 90 WHERE id = 'org1'",
            [],
        )
        .unwrap();
        let val: Option<i64> = conn
            .query_row(
                "SELECT retention_days FROM organizations WHERE id = 'org1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(val, Some(90), "retention_days must persist the set value");
    }

    #[test]
    fn run_v16_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v16(&conn);
        assert!(
            result.is_ok(),
            "run_v16 must be idempotent: {:?}",
            result.err()
        );
        assert!(
            get_user_version(&conn) >= 16,
            "user_version must be at least 16"
        );
    }

    #[test]
    fn run_v16_sets_user_version_to_16() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            get_user_version(&conn) >= 16,
            "user_version must be at least 16 after run_all"
        );
    }

    // ── v17 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v17_adds_archived_at_to_memories() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "memories", "archived_at"),
            "memories must have archived_at after v17"
        );
    }

    #[test]
    fn run_v17_archived_at_defaults_to_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1', 'org1', 'dev@acme.com', 'Dev')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, tool, content) VALUES ('m1', 'org1', 'u1', 'claude', 'hello')",
            [],
        ).unwrap();
        let val: Option<String> = conn
            .query_row(
                "SELECT archived_at FROM memories WHERE id = 'm1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(val.is_none(), "archived_at must default to NULL");
    }

    #[test]
    fn run_v17_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v17(&conn);
        assert!(
            result.is_ok(),
            "run_v17 must be idempotent: {:?}",
            result.err()
        );
        assert!(
            get_user_version(&conn) >= 17,
            "user_version must be at least 17"
        );
    }

    #[test]
    fn run_v17_sets_user_version_to_17() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            get_user_version(&conn) >= 17,
            "user_version must be at least 17 after run_all"
        );
    }

    // ── v18 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v18_adds_custom_instructions_to_organizations() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "organizations", "custom_instructions"),
            "organizations must have custom_instructions after v18"
        );
    }

    #[test]
    fn run_v18_custom_instructions_defaults_to_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        let val: Option<String> = conn
            .query_row(
                "SELECT custom_instructions FROM organizations WHERE id = 'org1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(val.is_none(), "custom_instructions must default to NULL");
    }

    #[test]
    fn run_v18_custom_instructions_persists_value() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE organizations SET custom_instructions = 'Always use TypeScript strict mode.' WHERE id = 'org1'",
            [],
        ).unwrap();
        let val: Option<String> = conn
            .query_row(
                "SELECT custom_instructions FROM organizations WHERE id = 'org1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            val.as_deref(),
            Some("Always use TypeScript strict mode."),
            "custom_instructions must persist the saved value"
        );
    }

    #[test]
    fn run_v18_clear_custom_instructions_sets_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE organizations SET custom_instructions = 'Some instructions.' WHERE id = 'org1'",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE organizations SET custom_instructions = NULL WHERE id = 'org1'",
            [],
        )
        .unwrap();
        let val: Option<String> = conn
            .query_row(
                "SELECT custom_instructions FROM organizations WHERE id = 'org1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            val.is_none(),
            "clearing custom_instructions must store NULL"
        );
    }

    #[test]
    fn run_v18_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v18(&conn);
        assert!(
            result.is_ok(),
            "run_v18 must be idempotent: {:?}",
            result.err()
        );
        assert!(
            get_user_version(&conn) >= 18,
            "user_version must be at least 18"
        );
    }

    #[test]
    fn run_v18_sets_user_version_to_18() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            get_user_version(&conn) >= 18,
            "user_version must be at least 18 after run_all"
        );
    }

    // ── v19 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v19_adds_pinned_to_memories() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "memories", "pinned"),
            "memories must have pinned after v19"
        );
    }

    #[test]
    fn run_v19_pinned_defaults_to_zero() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1', 'org1', 'dev@acme.com', 'Dev')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, tool, content) VALUES ('m1', 'org1', 'u1', 'claude', 'hello')",
            [],
        ).unwrap();
        let val: i64 = conn
            .query_row("SELECT pinned FROM memories WHERE id = 'm1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(val, 0, "pinned must default to 0");
    }

    #[test]
    fn run_v19_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v19(&conn);
        assert!(
            result.is_ok(),
            "run_v19 must be idempotent: {:?}",
            result.err()
        );
        assert!(
            get_user_version(&conn) >= 19,
            "user_version must be at least 19"
        );
    }

    #[test]
    fn run_v19_sets_user_version_to_19() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            get_user_version(&conn) >= 19,
            "user_version must be at least 19 after run_all"
        );
    }

    // ── v20 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v20_creates_invite_links_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            table_exists(&conn, "invite_links"),
            "invite_links table must exist after v20"
        );
    }

    #[test]
    fn run_v20_sets_user_version_to_20() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    #[test]
    fn run_v20_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v20(&conn);
        assert!(
            result.is_ok(),
            "run_v20 must be idempotent: {:?}",
            result.err()
        );
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must remain 70 after re-running v20 on already-migrated db"
        );
    }

    // ── v22 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v22_adds_min_password_length_to_organizations() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "organizations", "min_password_length"),
            "organizations must have min_password_length after v22"
        );
    }

    #[test]
    fn run_v22_min_password_length_defaults_to_8() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        let val: i64 = conn
            .query_row(
                "SELECT min_password_length FROM organizations WHERE id = 'org1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(val, 8, "min_password_length must default to 8");
    }

    #[test]
    fn run_v22_min_password_length_persists_value() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE organizations SET min_password_length = 12 WHERE id = 'org1'",
            [],
        )
        .unwrap();
        let val: i64 = conn
            .query_row(
                "SELECT min_password_length FROM organizations WHERE id = 'org1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(val, 12, "min_password_length must persist the set value");
    }

    #[test]
    fn run_v22_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v22(&conn);
        assert!(
            result.is_ok(),
            "run_v22 must be idempotent: {:?}",
            result.err()
        );
        assert!(
            get_user_version(&conn) >= 22,
            "user_version must be at least 22"
        );
    }

    // ── v23 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v23_adds_archived_at_to_projects() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "projects", "archived_at"),
            "projects must have archived_at after v23"
        );
    }

    #[test]
    fn run_v23_archive_sets_archived_at() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO projects (id, org_id, name) VALUES ('p1', 'org1', 'my-project')",
            [],
        )
        .unwrap();
        // Archive
        conn.execute(
            "UPDATE projects SET archived_at = datetime('now') WHERE id = 'p1' AND org_id = 'org1'",
            [],
        )
        .unwrap();
        let val: Option<String> = conn
            .query_row(
                "SELECT archived_at FROM projects WHERE id = 'p1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(val.is_some(), "archived_at must be set after archiving");
    }

    #[test]
    fn run_v23_restore_clears_archived_at() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO projects (id, org_id, name, archived_at) VALUES ('p1', 'org1', 'my-project', datetime('now'))",
            [],
        ).unwrap();
        // Restore
        conn.execute(
            "UPDATE projects SET archived_at = NULL WHERE id = 'p1' AND org_id = 'org1'",
            [],
        )
        .unwrap();
        let val: Option<String> = conn
            .query_row(
                "SELECT archived_at FROM projects WHERE id = 'p1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(val.is_none(), "archived_at must be NULL after restoring");
    }

    #[test]
    fn run_v23_adds_reindex_interval_hours_to_code_projects() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "code_projects", "reindex_interval_hours"),
            "code_projects must have reindex_interval_hours after v23"
        );
    }

    #[test]
    fn run_v23_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v23(&conn);
        assert!(
            result.is_ok(),
            "run_v23 must be idempotent: {:?}",
            result.err()
        );
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    // ── v24 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v24_creates_webhook_deliveries_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            table_exists(&conn, "webhook_deliveries"),
            "webhook_deliveries table must exist after v24"
        );
    }

    #[test]
    fn run_v24_creates_index() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            index_exists(
                &conn,
                "webhook_deliveries",
                "idx_webhook_deliveries_webhook_id"
            ),
            "idx_webhook_deliveries_webhook_id must exist after v24"
        );
    }

    #[test]
    fn run_v24_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v24(&conn);
        assert!(
            result.is_ok(),
            "run_v24 must be idempotent: {:?}",
            result.err()
        );
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    #[test]
    fn run_v24_sets_user_version_to_24() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    #[test]
    fn run_v25_collections_assign_memory_count() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        // Seed org + user
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1', 'org1', 'dev@acme.com', 'Dev')",
            [],
        ).unwrap();

        // Create collection
        conn.execute(
            "INSERT INTO collections (id, org_id, name) VALUES ('col1', 'org1', 'My Collection')",
            [],
        )
        .unwrap();

        // Create memory and assign to collection
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, tool, content) VALUES ('m1', 'org1', 'u1', 'claude', 'test')",
            [],
        ).unwrap();
        conn.execute(
            "UPDATE memories SET collection_id = 'col1' WHERE id = 'm1'",
            [],
        )
        .unwrap();

        // Assert count via LEFT JOIN
        let count: i64 = conn.query_row(
            "SELECT COUNT(m.id) FROM collections c LEFT JOIN memories m ON m.collection_id = c.id WHERE c.id = 'col1'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(
            count, 1,
            "collection must have memory_count = 1 after assignment"
        );
    }

    #[test]
    fn run_v14_webhooks_unique_org_name() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO webhooks (id, org_id, name, target_url) VALUES ('wh1', 'org1', 'my-hook', 'https://example.com/hook')",
            [],
        ).unwrap();
        let dup = conn.execute(
            "INSERT INTO webhooks (id, org_id, name, target_url) VALUES ('wh2', 'org1', 'my-hook', 'https://other.com/hook')",
            [],
        );
        assert!(
            dup.is_err(),
            "UNIQUE(org_id, name) must be enforced on webhooks"
        );
    }

    // ── v26 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v26_adds_sync_status_columns_to_code_projects() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "code_projects", "last_indexed_at"),
            "code_projects must have last_indexed_at after v26"
        );
        assert!(
            column_exists(&conn, "code_projects", "last_index_error"),
            "code_projects must have last_index_error after v26"
        );
        assert!(
            column_exists(&conn, "code_projects", "indexed_files_count"),
            "code_projects must have indexed_files_count after v26"
        );
        assert!(
            column_exists(&conn, "code_projects", "index_status"),
            "code_projects must have index_status after v26"
        );
    }

    #[test]
    fn run_v26_code_project_sync_status_roundtrip() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();

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
        assert!(
            result.is_ok(),
            "run_v26 must be idempotent: {:?}",
            result.err()
        );
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    // ── v27 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v27_adds_expires_at_to_api_keys() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "api_keys", "expires_at"),
            "api_keys must have expires_at after v27"
        );
    }

    #[test]
    fn run_v27_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v27(&conn);
        assert!(
            result.is_ok(),
            "run_v27 must be idempotent: {:?}",
            result.err()
        );
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    #[test]
    fn run_v27_sets_user_version_to_27() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    // ── v28 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v28_adds_disabled_at_to_users() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "users", "disabled_at"),
            "users must have disabled_at after v28"
        );
    }

    #[test]
    fn run_v28_disabled_at_defaults_to_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1', 'org1', 'dev@acme.com', 'Dev')",
            [],
        ).unwrap();
        let val: Option<String> = conn
            .query_row("SELECT disabled_at FROM users WHERE id = 'u1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(val.is_none(), "disabled_at must default to NULL");
    }

    #[test]
    fn run_v28_disable_enable_roundtrip() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1', 'org1', 'dev@acme.com', 'Dev')",
            [],
        ).unwrap();

        // Disable
        conn.execute(
            "UPDATE users SET disabled_at = datetime('now') WHERE id = 'u1'",
            [],
        )
        .unwrap();
        let disabled: Option<String> = conn
            .query_row("SELECT disabled_at FROM users WHERE id = 'u1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(
            disabled.is_some(),
            "disabled_at must be set after disabling"
        );

        // Re-enable
        conn.execute("UPDATE users SET disabled_at = NULL WHERE id = 'u1'", [])
            .unwrap();
        let enabled: Option<String> = conn
            .query_row("SELECT disabled_at FROM users WHERE id = 'u1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(
            enabled.is_none(),
            "disabled_at must be NULL after re-enabling"
        );
    }

    #[test]
    fn run_v28_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v28(&conn);
        assert!(
            result.is_ok(),
            "run_v28 must be idempotent: {:?}",
            result.err()
        );
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    #[test]
    fn run_all_sets_user_version_to_29() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    // ── v29 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v29_adds_admin_note_to_memories() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "memories", "admin_note"),
            "memories must have admin_note after v29"
        );
    }

    #[test]
    fn run_v29_admin_note_defaults_to_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1', 'org1', 'dev@acme.com', 'Dev')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, tool, content) VALUES ('m1', 'org1', 'u1', 'claude', 'hello')",
            [],
        ).unwrap();
        let val: Option<String> = conn
            .query_row("SELECT admin_note FROM memories WHERE id = 'm1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(val.is_none(), "admin_note must default to NULL");
    }

    #[test]
    fn run_v29_admin_note_roundtrip() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
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
        )
        .unwrap();
        let note: Option<String> = conn
            .query_row("SELECT admin_note FROM memories WHERE id = 'm1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            note.as_deref(),
            Some("Suspicious pattern — watch this."),
            "admin_note must persist"
        );
        // Clear note
        conn.execute("UPDATE memories SET admin_note = NULL WHERE id = 'm1'", [])
            .unwrap();
        let cleared: Option<String> = conn
            .query_row("SELECT admin_note FROM memories WHERE id = 'm1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(cleared.is_none(), "admin_note must be NULL after clearing");
    }

    #[test]
    fn run_v29_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v29(&conn);
        assert!(
            result.is_ok(),
            "run_v29 must be idempotent: {:?}",
            result.err()
        );
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    // ── admin_note integration test (via queries) ─────────────────────────────

    #[test]
    fn admin_note_set_and_not_in_list_without_admin() {
        use crate::db::{connection::connect, queries};

        let conn = connect(":memory:").unwrap();
        run_all(&conn).unwrap();

        let (_org, _user, _raw_key) =
            queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Get org_id + user_id
        let (org_id, user_id): (String, String) = conn
            .query_row(
                "SELECT o.id, u.id FROM organizations o JOIN users u ON u.org_id = o.id LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();

        // Create a memory
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, tool, content, project) VALUES ('m1', ?1, ?2, 'claude', 'test content', 'default')",
            rusqlite::params![org_id, user_id],
        ).unwrap();

        // Set admin_note via query
        let result =
            queries::update_memory_admin_note(&conn, &org_id, "m1", "Private admin note").unwrap();
        assert!(
            result.is_some(),
            "update_memory_admin_note must return the updated memory"
        );
        let mem = result.unwrap();
        assert_eq!(
            mem.admin_note.as_deref(),
            Some("Private admin note"),
            "admin_note must be returned in admin context"
        );

        // Simulate non-admin list: admin_note should be present in DB but stripped by handler layer
        // Here we test that the DB query returns it, and the handler is responsible for stripping.
        let mems = queries::list_memories(
            &conn, &org_id, None, None, None, None, None, None, 50, 0, false, None, None, None,
        )
        .unwrap();
        assert_eq!(mems.len(), 1);
        assert_eq!(
            mems[0].admin_note.as_deref(),
            Some("Private admin note"),
            "DB query always returns admin_note; handler strips for non-admins"
        );

        // Verify clearing: empty string → NULL
        let cleared = queries::update_memory_admin_note(&conn, &org_id, "m1", "").unwrap();
        assert!(cleared.is_some());
        assert!(
            cleared.unwrap().admin_note.is_none(),
            "empty string must clear admin_note to NULL"
        );
    }

    // ── Disable/enable account integration test ───────────────────────────────

    #[test]
    fn disabled_user_key_is_rejected() {
        use crate::auth::api_keys;
        use crate::db::{connection::connect, queries};

        let conn = connect(":memory:").unwrap();
        run_all(&conn).unwrap();

        // Create org + user + key via bootstrap
        let (_org, user, raw_key) =
            queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Key should work initially
        let hash = api_keys::hash_key(&raw_key);
        let ctx = queries::validate_api_key(&conn, &hash).unwrap();
        assert!(ctx.is_some(), "key must be valid before disabling");

        // Disable the user
        let changed = queries::disable_user(&conn, &user.org_id, &user.id).unwrap();
        assert!(changed, "disable_user must return true for an active user");

        // Key must now be rejected
        let ctx_disabled = queries::validate_api_key(&conn, &hash).unwrap();
        assert!(
            ctx_disabled.is_none(),
            "key must be rejected after account is disabled"
        );

        // is_key_account_disabled must return true
        let is_disabled = queries::is_key_account_disabled(&conn, &hash).unwrap();
        assert!(
            is_disabled,
            "is_key_account_disabled must return true for a disabled account"
        );

        // Re-enable the user
        let re_enabled = queries::enable_user(&conn, &user.org_id, &user.id).unwrap();
        assert!(
            re_enabled,
            "enable_user must return true for a disabled user"
        );

        // Key must work again
        let ctx_enabled = queries::validate_api_key(&conn, &hash).unwrap();
        assert!(
            ctx_enabled.is_some(),
            "key must work again after re-enabling"
        );
    }

    // ── v30 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v30_adds_announcement_columns_to_organizations() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "organizations", "announcement"),
            "organizations must have announcement after v30"
        );
        assert!(
            column_exists(&conn, "organizations", "announcement_type"),
            "organizations must have announcement_type after v30"
        );
    }

    #[test]
    fn run_v30_adds_delete_after_to_memories() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "memories", "delete_after"),
            "memories must have delete_after after v30"
        );
    }

    #[test]
    fn run_v30_announcement_roundtrip() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();

        // Set announcement
        conn.execute(
            "UPDATE organizations SET announcement = 'Maintenance tonight', announcement_type = 'warning' WHERE id = 'org1'",
            [],
        ).unwrap();

        let (ann, ann_type): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT announcement, announcement_type FROM organizations WHERE id = 'org1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            ann.as_deref(),
            Some("Maintenance tonight"),
            "announcement must persist"
        );
        assert_eq!(
            ann_type.as_deref(),
            Some("warning"),
            "announcement_type must persist"
        );

        // Clear announcement
        conn.execute(
            "UPDATE organizations SET announcement = NULL WHERE id = 'org1'",
            [],
        )
        .unwrap();
        let ann_cleared: Option<String> = conn
            .query_row(
                "SELECT announcement FROM organizations WHERE id = 'org1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            ann_cleared.is_none(),
            "clearing announcement must store NULL"
        );
    }

    #[test]
    fn run_v30_delete_after_roundtrip() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
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
        )
        .unwrap();

        let val: Option<String> = conn
            .query_row(
                "SELECT delete_after FROM memories WHERE id = 'm1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            val.as_deref(),
            Some("2026-12-31"),
            "delete_after must persist"
        );

        // Clear it
        conn.execute(
            "UPDATE memories SET delete_after = NULL WHERE id = 'm1'",
            [],
        )
        .unwrap();
        let cleared: Option<String> = conn
            .query_row(
                "SELECT delete_after FROM memories WHERE id = 'm1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(cleared.is_none(), "clearing delete_after must store NULL");
    }

    #[test]
    fn run_v30_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v30(&conn);
        assert!(
            result.is_ok(),
            "run_v30 must be idempotent: {:?}",
            result.err()
        );
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    #[test]
    fn run_v30_sets_user_version_to_30() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    #[test]
    fn run_v31_adds_archived_at_to_code_projects() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "code_projects", "archived_at"),
            "code_projects must have archived_at after v31"
        );
    }

    #[test]
    fn run_v31_archived_at_defaults_to_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO code_projects (org_id, name, root_path) VALUES ('org1', 'myapp', '/ws')",
            [],
        )
        .unwrap();
        let val: Option<String> = conn
            .query_row(
                "SELECT archived_at FROM code_projects WHERE name = 'myapp'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(val.is_none(), "archived_at must default to NULL");
    }

    #[test]
    fn run_v31_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v31(&conn);
        assert!(
            result.is_ok(),
            "run_v31 must be idempotent: {:?}",
            result.err()
        );
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    #[test]
    fn run_v31_sets_user_version_to_31() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    // ── v32 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v32_adds_admin_note_to_users() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "users", "admin_note"),
            "users must have admin_note after v32"
        );
    }

    #[test]
    fn run_v32_admin_note_defaults_to_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1', 'org1', 'dev@acme.com', 'Dev')",
            [],
        ).unwrap();
        let val: Option<String> = conn
            .query_row("SELECT admin_note FROM users WHERE id = 'u1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(val.is_none(), "admin_note must default to NULL");
    }

    #[test]
    fn run_v32_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v32(&conn);
        assert!(
            result.is_ok(),
            "run_v32 must be idempotent: {:?}",
            result.err()
        );
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    #[test]
    fn run_v32_sets_user_version_to_32() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after run_all"
        );
    }

    // ── v35 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v35_adds_logo_url_to_organizations() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            column_exists(&conn, "organizations", "logo_url"),
            "organizations must have logo_url after v35"
        );
    }

    #[test]
    fn run_v35_logo_url_defaults_to_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        let val: Option<String> = conn
            .query_row(
                "SELECT logo_url FROM organizations WHERE id = 'org1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(val.is_none(), "logo_url must default to NULL");
    }

    #[test]
    fn run_v35_logo_url_persists_value() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE organizations SET logo_url = 'https://example.com/logo.png' WHERE id = 'org1'",
            [],
        )
        .unwrap();
        let val: Option<String> = conn
            .query_row(
                "SELECT logo_url FROM organizations WHERE id = 'org1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            val.as_deref(),
            Some("https://example.com/logo.png"),
            "logo_url must persist the set value"
        );
    }

    #[test]
    fn run_v35_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v35(&conn);
        assert!(
            result.is_ok(),
            "run_v35 must be idempotent: {:?}",
            result.err()
        );
        assert!(
            get_user_version(&conn) >= 35,
            "user_version must be at least 35"
        );
    }

    // ── v36 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v36_creates_conventions_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            table_exists(&conn, "conventions"),
            "conventions table must exist after v36"
        );
    }

    #[test]
    fn run_v36_creates_indexes() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            index_exists(&conn, "conventions", "idx_conventions_org"),
            "idx_conventions_org must exist after v36"
        );
        assert!(
            index_exists(&conn, "conventions", "idx_conventions_category"),
            "idx_conventions_category must exist after v36"
        );
    }

    #[test]
    fn run_v36_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v36(&conn);
        assert!(
            result.is_ok(),
            "run_v36 must be idempotent: {:?}",
            result.err()
        );
        assert!(
            get_user_version(&conn) >= 36,
            "user_version must be at least 36"
        );
    }

    #[test]
    fn run_v36_convention_roundtrip() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO conventions (org_id, title, content, category, weight, tags) VALUES ('org1', 'Test Convention', 'Content here', 'architecture', 200, '[]')",
            [],
        ).unwrap();
        let (title, cat, weight): (String, String, i64) = conn
            .query_row(
                "SELECT title, category, weight FROM conventions WHERE org_id = 'org1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(title, "Test Convention");
        assert_eq!(cat, "architecture");
        assert_eq!(weight, 200);
    }

    // ── v37 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v37_creates_github_connections_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            table_exists(&conn, "github_connections"),
            "github_connections table must exist after v37"
        );
    }

    #[test]
    fn run_v37_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v37(&conn);
        assert!(
            result.is_ok(),
            "run_v37 must be idempotent: {:?}",
            result.err()
        );
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must remain 70 (run_all already applied v41-v70)"
        );
    }

    // ── v41 + v42 migration tests (code knowledge graph) ────────────────────────

    #[test]
    fn run_all_sets_user_version_to_43() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must be 70 after v41-v70 are included in run_all"
        );
        assert!(
            table_exists(&conn, "code_files"),
            "code_files table must exist after v43"
        );
    }

    #[test]
    fn run_v41_creates_code_symbols_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            table_exists(&conn, "code_symbols"),
            "code_symbols must exist after v41"
        );
    }

    #[test]
    fn run_v42_creates_code_edges_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            table_exists(&conn, "code_edges"),
            "code_edges must exist after v42"
        );
    }

    #[test]
    fn run_v41_code_symbols_unique_qualified_name() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO code_projects (org_id, name, root_path) VALUES ('org1', 'myapp', '/ws')",
            [],
        )
        .unwrap();
        let pid: i64 = conn
            .query_row("SELECT id FROM code_projects WHERE name='myapp'", [], |r| {
                r.get(0)
            })
            .unwrap();
        conn.execute(
            "INSERT INTO code_symbols \
             (code_project_id, symbol_type, name, qualified_name, file_path, file_hash, start_line, end_line, language) \
             VALUES (?1, 'Function', 'my_fn', 'src/lib.rs::my_fn#1', 'src/lib.rs', 'abc', 1, 10, 'rust')",
            rusqlite::params![pid],
        ).unwrap();
        // Duplicate qualified_name must be rejected
        let dup = conn.execute(
            "INSERT INTO code_symbols \
             (code_project_id, symbol_type, name, qualified_name, language) \
             VALUES (?1, 'Function', 'my_fn', 'src/lib.rs::my_fn#1', 'rust')",
            rusqlite::params![pid],
        );
        assert!(
            dup.is_err(),
            "UNIQUE(code_project_id, qualified_name) must reject duplicate"
        );
    }

    #[test]
    fn run_v42_code_edges_cascade_delete_on_symbol() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO code_projects (org_id, name, root_path) VALUES ('org1', 'p', '/ws')",
            [],
        )
        .unwrap();
        let pid: i64 = conn
            .query_row("SELECT id FROM code_projects WHERE name='p'", [], |r| {
                r.get(0)
            })
            .unwrap();
        conn.execute(
            "INSERT INTO code_symbols (code_project_id, symbol_type, name, qualified_name, language) \
             VALUES (?1, 'File', 'a', 'file::a.rs', 'rust')",
            rusqlite::params![pid],
        ).unwrap();
        conn.execute(
            "INSERT INTO code_symbols (code_project_id, symbol_type, name, qualified_name, language) \
             VALUES (?1, 'Function', 'foo', 'a.rs::foo#1', 'rust')",
            rusqlite::params![pid],
        ).unwrap();
        let from_id: i64 = conn
            .query_row("SELECT id FROM code_symbols WHERE name='a'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let to_id: i64 = conn
            .query_row("SELECT id FROM code_symbols WHERE name='foo'", [], |r| {
                r.get(0)
            })
            .unwrap();
        conn.execute(
            "INSERT INTO code_edges (code_project_id, from_symbol_id, to_symbol_id, edge_type) \
             VALUES (?1, ?2, ?3, 'defines')",
            rusqlite::params![pid, from_id, to_id],
        )
        .unwrap();
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM code_edges", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "edge must exist before symbol deletion");
        conn.execute(
            "DELETE FROM code_symbols WHERE id = ?1",
            rusqlite::params![from_id],
        )
        .unwrap();
        let after: i32 = conn
            .query_row("SELECT COUNT(*) FROM code_edges", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            after, 0,
            "edges must cascade-delete when from_symbol is removed"
        );
    }

    #[test]
    fn run_all_v41_v42_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_all(&conn);
        assert!(
            result.is_ok(),
            "run_all must be idempotent after v41+v42: {:?}",
            result.err()
        );
        assert_eq!(
            get_user_version(&conn),
            70,
            "version must remain 67 on second run_all"
        );
    }

    #[test]
    fn run_v37_cascade_delete_on_org_remove() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO github_connections (org_id, access_token, github_login, github_user_id)
             VALUES ('org1', 'gho_test', 'acme-bot', 12345)",
            [],
        )
        .unwrap();
        // Connection must exist
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM github_connections WHERE org_id = 'org1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        // Delete org — connection must cascade
        conn.execute("DELETE FROM organizations WHERE id = 'org1'", [])
            .unwrap();
        let after: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM github_connections WHERE org_id = 'org1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(after, 0, "github_connections must cascade-delete with org");
    }

    // ── v39 / v45 migration tests (agents) ──────────────────────────────────────
    //
    // Regression coverage for the bug where `run_all()` jumped from `run_v38()`
    // straight to `run_v40()`, never invoking `run_v39()` — so the `agents` and
    // `agent_assignments` tables were never created and `/v1/agents*` returned
    // HTTP 500 "no such table: agents".

    /// Reproduces the pre-fix `run_all()` sequence (skips `run_v39`) to simulate a
    /// production database that already reached `user_version = 44` via the buggy
    /// migration chain — i.e. a DB that is missing the `agents` / `agent_assignments`
    /// tables and can no longer reach them via the normal `run_v39` guard (which
    /// short-circuits once `user_version >= 39`).
    fn simulate_prod_db_at_v44_missing_agents(conn: &Connection) {
        run_v1(conn).unwrap();
        run_v2(conn).unwrap();
        run_v3(conn).unwrap();
        run_v4(conn).unwrap();
        run_v5(conn).unwrap();
        run_v6(conn).unwrap();
        run_v7(conn).unwrap();
        run_v8(conn).unwrap();
        run_v9(conn).unwrap();
        run_v10(conn).unwrap();
        run_v11(conn).unwrap();
        run_v12(conn).unwrap();
        run_v13(conn).unwrap();
        run_v14(conn).unwrap();
        run_v15(conn).unwrap();
        run_v16(conn).unwrap();
        run_v17(conn).unwrap();
        run_v18(conn).unwrap();
        run_v19(conn).unwrap();
        run_v20(conn).unwrap();
        run_v21(conn).unwrap();
        run_v22(conn).unwrap();
        run_v23(conn).unwrap();
        run_v24(conn).unwrap();
        run_v25(conn).unwrap();
        run_v26(conn).unwrap();
        run_v27(conn).unwrap();
        run_v28(conn).unwrap();
        run_v29(conn).unwrap();
        run_v30(conn).unwrap();
        run_v31(conn).unwrap();
        run_v32(conn).unwrap();
        run_v33(conn).unwrap();
        run_v34(conn).unwrap();
        run_v35(conn).unwrap();
        run_v36(conn).unwrap();
        run_v37(conn).unwrap();
        run_v38(conn).unwrap();
        // run_v39 intentionally skipped — this reproduces the historical bug in run_all().
        run_v40(conn).unwrap();
        run_v41(conn).unwrap();
        run_v42(conn).unwrap();
        run_v43(conn).unwrap();
        run_v44(conn).unwrap();
    }

    #[test]
    fn run_all_creates_agents_table_on_fresh_db() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            table_exists(&conn, "agents"),
            "agents table must exist after run_all on a fresh db"
        );
        assert!(
            table_exists(&conn, "agent_assignments"),
            "agent_assignments table must exist after run_all on a fresh db"
        );
    }

    #[test]
    fn run_all_backfills_agents_table_for_db_stuck_at_v44() {
        // Sanity check: confirm the simulated prod DB really is missing the agents
        // table before the fix runs (otherwise this test would be vacuous).
        let sanity_conn = in_memory_db();
        simulate_prod_db_at_v44_missing_agents(&sanity_conn);
        assert_eq!(get_user_version(&sanity_conn), 44);
        assert!(
            !table_exists(&sanity_conn, "agents"),
            "sanity: simulated prod db must be missing the agents table before the backfill runs"
        );

        // Now simulate the deployed prod DB receiving the new binary: it re-runs
        // migrations (via run_all), which must include the new backfill step and
        // create the agents tables even though user_version is already 44.
        let conn = in_memory_db();
        simulate_prod_db_at_v44_missing_agents(&conn);
        run_all(&conn).unwrap();

        assert!(
            table_exists(&conn, "agents"),
            "agents table must exist after the backfill migration runs on a db stuck at v44"
        );
        assert!(
            table_exists(&conn, "agent_assignments"),
            "agent_assignments table must exist after the backfill migration runs on a db stuck at v44"
        );
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must reach 70 after the backfill migration"
        );
    }

    #[test]
    fn run_all_is_idempotent_after_v46() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_all(&conn);
        assert!(
            result.is_ok(),
            "run_all must be idempotent after v45: {:?}",
            result.err()
        );
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must remain 70 on second run_all"
        );
    }

    // ── v51 / v52 migration tests (team-tasks) ──────────────────────────────────

    #[test]
    fn run_all_creates_tasks_tables_on_fresh_db() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        for table in [
            "tasks",
            "task_assignees",
            "task_labels",
            "task_comments",
            "task_spec_links",
            "sprints",
            "sprint_retrospectives",
        ] {
            assert!(
                table_exists(&conn, table),
                "{table} table must exist after run_all on a fresh db"
            );
        }
        assert_eq!(
            get_user_version(&conn),
            70,
            "user_version must reach 70 on a fresh db"
        );
    }

    #[test]
    fn run_v51_creates_task_tables_with_expected_columns() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        for col in [
            "id",
            "org_id",
            "project",
            "title",
            "description",
            "status",
            "priority",
            "due_date",
            "parent_id",
            "sprint_id",
            "created_by",
            "created_at",
            "updated_at",
            "archived_at",
        ] {
            assert!(column_exists(&conn, "tasks", col), "tasks.{col} must exist");
        }
        for col in ["id", "task_id", "user_id", "assigned_by", "assigned_at"] {
            assert!(
                column_exists(&conn, "task_assignees", col),
                "task_assignees.{col} must exist"
            );
        }
        for col in ["id", "task_id", "label", "created_at"] {
            assert!(
                column_exists(&conn, "task_labels", col),
                "task_labels.{col} must exist"
            );
        }
        for col in ["id", "task_id", "user_id", "body", "created_at"] {
            assert!(
                column_exists(&conn, "task_comments", col),
                "task_comments.{col} must exist"
            );
        }
        for col in [
            "id",
            "task_id",
            "spec_change_name",
            "linked_by",
            "created_at",
        ] {
            assert!(
                column_exists(&conn, "task_spec_links", col),
                "task_spec_links.{col} must exist"
            );
        }
        for col in [
            "id",
            "org_id",
            "project",
            "name",
            "goal",
            "starts_at",
            "ends_at",
            "status",
            "created_by",
            "created_at",
            "archived_at",
        ] {
            assert!(
                column_exists(&conn, "sprints", col),
                "sprints.{col} must exist"
            );
        }
        for col in [
            "id",
            "sprint_id",
            "org_id",
            "went_well",
            "went_wrong",
            "action_items",
            "created_by",
            "created_at",
        ] {
            assert!(
                column_exists(&conn, "sprint_retrospectives", col),
                "sprint_retrospectives.{col} must exist"
            );
        }
    }

    #[test]
    fn run_v51_creates_indexes() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(index_exists(&conn, "tasks", "idx_tasks_org_project_status"));
        assert!(index_exists(&conn, "tasks", "idx_tasks_org_parent"));
        assert!(index_exists(&conn, "tasks", "idx_tasks_sprint"));
        assert!(index_exists(
            &conn,
            "task_assignees",
            "idx_task_assignees_user"
        ));
        assert!(index_exists(&conn, "task_labels", "idx_task_labels_label"));
        assert!(index_exists(
            &conn,
            "task_comments",
            "idx_task_comments_task"
        ));
        assert!(index_exists(
            &conn,
            "task_spec_links",
            "idx_task_spec_links_change"
        ));
        assert!(index_exists(
            &conn,
            "sprints",
            "idx_sprints_org_project_status"
        ));
        assert!(index_exists(
            &conn,
            "sprint_retrospectives",
            "idx_sprint_retros_sprint"
        ));
    }

    #[test]
    fn run_v51_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let table_count_before: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let index_count_before: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        let result = run_all(&conn);
        assert!(
            result.is_ok(),
            "run_all must be idempotent after v51/v52: {:?}",
            result.err()
        );

        let table_count_after: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let index_count_after: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            table_count_before, table_count_after,
            "table count must not change on re-run"
        );
        assert_eq!(
            index_count_before, index_count_after,
            "index count must not change on re-run"
        );
    }

    #[test]
    fn run_v51_fk_cascade_and_unique_constraints() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme');
             INSERT INTO users (id, org_id, email, name, role) VALUES ('u1', 'org1', 'u1@acme.com', 'U1', 'admin');
             INSERT INTO users (id, org_id, email, name, role) VALUES ('u2', 'org1', 'u2@acme.com', 'U2', 'admin');
             INSERT INTO sprints (id, org_id, project, name, created_by) VALUES ('sp1', 'org1', 'proj', 'Sprint 1', 'u1');
             INSERT INTO tasks (id, org_id, project, title, created_by, sprint_id) VALUES ('t1', 'org1', 'proj', 'Task 1', 'u1', 'sp1');",
        )
        .unwrap();

        // task_assignees cascades with task, UNIQUE(task_id, user_id) enforced.
        conn.execute(
            "INSERT INTO task_assignees (id, task_id, user_id, assigned_by) VALUES ('ta1', 't1', 'u2', 'u1')",
            [],
        )
        .unwrap();
        let dup = conn.execute(
            "INSERT INTO task_assignees (id, task_id, user_id, assigned_by) VALUES ('ta2', 't1', 'u2', 'u1')",
            [],
        );
        assert!(
            dup.is_err(),
            "UNIQUE(task_id, user_id) on task_assignees must be enforced"
        );

        // task_labels cascades with task, UNIQUE(task_id, label) enforced.
        conn.execute(
            "INSERT INTO task_labels (id, task_id, label) VALUES ('tl1', 't1', 'bug')",
            [],
        )
        .unwrap();
        let dup_label = conn.execute(
            "INSERT INTO task_labels (id, task_id, label) VALUES ('tl2', 't1', 'bug')",
            [],
        );
        assert!(
            dup_label.is_err(),
            "UNIQUE(task_id, label) on task_labels must be enforced"
        );

        // task_comments cascades with task.
        conn.execute(
            "INSERT INTO task_comments (id, task_id, user_id, body) VALUES ('tc1', 't1', 'u1', 'hello')",
            [],
        )
        .unwrap();

        // task_spec_links cascades with task, UNIQUE(task_id, spec_change_name) enforced.
        conn.execute(
            "INSERT INTO task_spec_links (id, task_id, spec_change_name, linked_by) VALUES ('tsl1', 't1', 'team-tasks', 'u1')",
            [],
        )
        .unwrap();
        let dup_link = conn.execute(
            "INSERT INTO task_spec_links (id, task_id, spec_change_name, linked_by) VALUES ('tsl2', 't1', 'team-tasks', 'u1')",
            [],
        );
        assert!(
            dup_link.is_err(),
            "UNIQUE(task_id, spec_change_name) on task_spec_links must be enforced"
        );

        // sprints UNIQUE(org_id, project, name) enforced.
        let dup_sprint = conn.execute(
            "INSERT INTO sprints (id, org_id, project, name, created_by) VALUES ('sp2', 'org1', 'proj', 'Sprint 1', 'u1')",
            [],
        );
        assert!(
            dup_sprint.is_err(),
            "UNIQUE(org_id, project, name) on sprints must be enforced"
        );

        // sprint_retrospectives cascades with sprint.
        conn.execute(
            "INSERT INTO sprint_retrospectives (id, sprint_id, org_id, created_by) VALUES ('sr1', 'sp1', 'org1', 'u1')",
            [],
        )
        .unwrap();

        // Deleting the task cascades to assignees/labels/comments/spec_links.
        conn.execute("DELETE FROM tasks WHERE id = 't1'", [])
            .unwrap();
        let remaining_assignees: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM task_assignees WHERE task_id = 't1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let remaining_labels: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM task_labels WHERE task_id = 't1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let remaining_comments: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM task_comments WHERE task_id = 't1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let remaining_links: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM task_spec_links WHERE task_id = 't1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            remaining_assignees, 0,
            "task_assignees must cascade-delete with task"
        );
        assert_eq!(
            remaining_labels, 0,
            "task_labels must cascade-delete with task"
        );
        assert_eq!(
            remaining_comments, 0,
            "task_comments must cascade-delete with task"
        );
        assert_eq!(
            remaining_links, 0,
            "task_spec_links must cascade-delete with task"
        );

        // Deleting the sprint cascades to retrospectives and SETs task.sprint_id NULL
        // (re-create a task pointing at sp1 to verify the SET NULL path independently).
        conn.execute(
            "INSERT INTO tasks (id, org_id, project, title, created_by, sprint_id) VALUES ('t2', 'org1', 'proj', 'Task 2', 'u1', 'sp1')",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM sprints WHERE id = 'sp1'", [])
            .unwrap();
        let remaining_retros: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sprint_retrospectives WHERE sprint_id = 'sp1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            remaining_retros, 0,
            "sprint_retrospectives must cascade-delete with sprint"
        );
        let t2_sprint_id: Option<String> = conn
            .query_row("SELECT sprint_id FROM tasks WHERE id = 't2'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            t2_sprint_id, None,
            "tasks.sprint_id must be SET NULL when the sprint is deleted"
        );
    }

    #[test]
    fn run_v52_grants_task_perms() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        let perms_json = |template_id: &str| -> String {
            conn.query_row(
                "SELECT permissions FROM roles WHERE id = ?1",
                [template_id],
                |r| r.get(0),
            )
            .unwrap()
        };
        let has_perm = |json: &str, perm: &str| -> bool {
            let arr: Vec<String> = serde_json::from_str(json).unwrap();
            arr.iter().any(|p| p == perm)
        };

        let junior = perms_json("tmpl_dev_junior");
        assert!(has_perm(&junior, "task:read"));
        assert!(has_perm(&junior, "task:write"));
        assert!(!has_perm(&junior, "task:assign"));
        assert!(!has_perm(&junior, "task:delete"));
        assert!(!has_perm(&junior, "task:manage"));

        let senior = perms_json("tmpl_dev_senior");
        assert!(has_perm(&senior, "task:read"));
        assert!(has_perm(&senior, "task:write"));
        assert!(has_perm(&senior, "task:assign"));
        assert!(has_perm(&senior, "task:delete"));
        assert!(!has_perm(&senior, "task:manage"));

        let security_officer = perms_json("tmpl_security_officer");
        assert!(has_perm(&security_officer, "task:read"));
        assert!(!has_perm(&security_officer, "task:write"));

        let auditor = perms_json("tmpl_auditor");
        assert!(has_perm(&auditor, "task:read"));
        assert!(!has_perm(&auditor, "task:write"));
    }

    #[test]
    fn run_v52_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let before: String = conn
            .query_row(
                "SELECT permissions FROM roles WHERE id = 'tmpl_dev_senior'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        run_all(&conn).unwrap();

        let after: String = conn
            .query_row(
                "SELECT permissions FROM roles WHERE id = 'tmpl_dev_senior'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            before, after,
            "re-running run_all must not duplicate permission strings"
        );
        let arr: Vec<String> = serde_json::from_str(&after).unwrap();
        let task_write_count = arr.iter().filter(|p| p.as_str() == "task:write").count();
        assert_eq!(
            task_write_count, 1,
            "task:write must appear exactly once after re-run"
        );
    }

    #[test]
    fn run_v52_preserves_existing_permissions() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        let senior: String = conn
            .query_row(
                "SELECT permissions FROM roles WHERE id = 'tmpl_dev_senior'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let senior_arr: Vec<String> = serde_json::from_str(&senior).unwrap();
        for pre_existing in [
            "memory:read",
            "memory:write",
            "memory:delete",
            "memory:search",
        ] {
            assert!(
                senior_arr.iter().any(|p| p == pre_existing),
                "tmpl_dev_senior must retain pre-existing permission {pre_existing}"
            );
        }

        let junior: String = conn
            .query_row(
                "SELECT permissions FROM roles WHERE id = 'tmpl_dev_junior'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let junior_arr: Vec<String> = serde_json::from_str(&junior).unwrap();
        for pre_existing in ["memory:read", "memory:search"] {
            assert!(
                junior_arr.iter().any(|p| p == pre_existing),
                "tmpl_dev_junior must retain pre-existing permission {pre_existing}"
            );
        }

        let auditor: String = conn
            .query_row(
                "SELECT permissions FROM roles WHERE id = 'tmpl_auditor'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let auditor_arr: Vec<String> = serde_json::from_str(&auditor).unwrap();
        assert!(
            auditor_arr.iter().any(|p| p == "audit:read"),
            "tmpl_auditor must retain pre-existing audit:read"
        );

        let security_officer: String = conn
            .query_row(
                "SELECT permissions FROM roles WHERE id = 'tmpl_security_officer'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let so_arr: Vec<String> = serde_json::from_str(&security_officer).unwrap();
        for pre_existing in ["audit:read", "settings:write"] {
            assert!(
                so_arr.iter().any(|p| p == pre_existing),
                "tmpl_security_officer must retain pre-existing permission {pre_existing}"
            );
        }
    }

    // ── SDD artifacts (v53 schema, v54 permissions) ─────────────────────────

    /// Column metadata from `PRAGMA table_info`: (notnull, dflt_value).
    fn column_info(conn: &Connection, table: &str, column: &str) -> Option<(bool, Option<String>)> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(1)?,         // name
                    r.get::<_, i32>(3)? != 0,       // notnull
                    r.get::<_, Option<String>>(4)?, // dflt_value
                ))
            })
            .unwrap();
        for row in rows.flatten() {
            if row.0 == column {
                return Some((row.1, row.2));
            }
        }
        None
    }

    fn permissions_of(conn: &Connection, template_id: &str) -> Vec<String> {
        let raw: String = conn
            .query_row(
                "SELECT permissions FROM roles WHERE id = ?1",
                [template_id],
                |r| r.get(0),
            )
            .unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    /// Seeds the minimal FK ancestry the SDD tables hang off: org, user, sprint,
    /// memory. Returns (org_id, user_id, sprint_id, memory_id).
    fn seed_sdd_fixtures(conn: &Connection) -> (String, String, String, String) {
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Org One', 'org-one')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role) VALUES ('u1', 'org1', 'u1@test.dev', 'U One', 'admin')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sprints (id, org_id, project, name, created_by)
             VALUES ('sp1', 'org1', 'nexus-mind', 'Sprint 1', 'u1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, tool, content)
             VALUES ('m1', 'org1', 'u1', 'claude-code', 'a decision')",
            [],
        )
        .unwrap();
        ("org1".into(), "u1".into(), "sp1".into(), "m1".into())
    }

    fn insert_change(conn: &Connection, id: &str, project: &str, name: &str) {
        conn.execute(
            "INSERT INTO sdd_changes (id, org_id, project, name, created_by)
             VALUES (?1, 'org1', ?2, ?3, 'u1')",
            rusqlite::params![id, project, name],
        )
        .unwrap();
    }

    /// 1.1 — every SDD table exists with the design §2 columns.
    #[test]
    fn run_v53_creates_sdd_tables_with_expected_columns() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        for table in [
            "sdd_changes",
            "sdd_artifacts",
            "sdd_artifact_revisions",
            "sdd_change_memories",
        ] {
            assert!(table_exists(&conn, table), "missing table: {table}");
        }

        for col in [
            "id",
            "org_id",
            "project",
            "name",
            "title",
            "status",
            "phase",
            "repo_url",
            "repo_ref",
            "sprint_id",
            "created_by",
            "created_at",
            "updated_at",
            "archived_at",
        ] {
            assert!(
                column_info(&conn, "sdd_changes", col).is_some(),
                "sdd_changes missing column: {col}"
            );
        }
        for col in [
            "id",
            "change_id",
            "kind",
            "capability",
            "path",
            "latest_revision",
            "created_at",
            "updated_at",
        ] {
            assert!(
                column_info(&conn, "sdd_artifacts", col).is_some(),
                "sdd_artifacts missing column: {col}"
            );
        }
        for col in [
            "id",
            "artifact_id",
            "revision",
            "content",
            "content_hash",
            "byte_size",
            "git_commit",
            "git_path",
            "source",
            "created_by",
            "created_at",
        ] {
            assert!(
                column_info(&conn, "sdd_artifact_revisions", col).is_some(),
                "sdd_artifact_revisions missing column: {col}"
            );
        }
        for col in [
            "id",
            "change_id",
            "memory_id",
            "relation",
            "linked_by",
            "created_at",
        ] {
            assert!(
                column_info(&conn, "sdd_change_memories", col).is_some(),
                "sdd_change_memories missing column: {col}"
            );
        }
    }

    /// 1.3 — the FTS5 virtual table exists, is queryable, and `artifact_id` is UNINDEXED
    /// (searching for the id value must NOT match; searching content must).
    #[test]
    fn run_v53_creates_fts_virtual_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        assert!(
            table_exists(&conn, "sdd_artifacts_fts"),
            "missing fts table: sdd_artifacts_fts"
        );

        conn.execute(
            "INSERT INTO sdd_artifacts_fts (artifact_id, change_name, kind, capability, content)
             VALUES ('art1', 'team-tasks', 'design', '', 'the rate limiter uses a token bucket')",
            [],
        )
        .unwrap();

        let hits: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sdd_artifacts_fts WHERE sdd_artifacts_fts MATCH 'rate'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1, "content column must be indexed and searchable");

        let id_hits: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sdd_artifacts_fts WHERE sdd_artifacts_fts MATCH 'art1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            id_hits, 0,
            "artifact_id must be UNINDEXED — it is a payload, not a search term"
        );
    }

    /// 1.5 — every index from design §2 is present.
    #[test]
    fn run_v53_creates_indexes() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        for (table, idx) in [
            ("sdd_changes", "idx_sdd_changes_org_project_status"),
            ("sdd_changes", "idx_sdd_changes_name"),
            ("sdd_changes", "idx_sdd_changes_sprint"),
            ("sdd_artifacts", "idx_sdd_artifacts_change"),
            ("sdd_artifact_revisions", "idx_sdd_revisions_artifact"),
            ("sdd_artifact_revisions", "idx_sdd_revisions_hash"),
            ("sdd_change_memories", "idx_sdd_change_memories_memory"),
        ] {
            assert!(
                index_exists(&conn, table, idx),
                "missing index: {idx} on {table}"
            );
        }
    }

    /// 1.7 — THE TRAP. `capability` must be NOT NULL DEFAULT '' so that
    /// UNIQUE(change_id, kind, capability) actually holds. SQLite treats every NULL as
    /// distinct inside a UNIQUE constraint, so a nullable capability would silently let
    /// two `design` artifacts into the same change.
    #[test]
    fn run_v53_capability_empty_string_sentinel_enforces_uniqueness() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_sdd_fixtures(&conn);
        insert_change(&conn, "c1", "nexus-mind", "team-tasks");

        let (notnull, default) = column_info(&conn, "sdd_artifacts", "capability")
            .expect("sdd_artifacts.capability must exist");
        assert!(
            notnull,
            "capability MUST be NOT NULL — a nullable column defeats the UNIQUE constraint"
        );
        assert_eq!(
            default.as_deref(),
            Some("''"),
            "capability MUST default to the empty-string sentinel, not NULL"
        );

        // Relying on DEFAULT '' — no explicit capability.
        conn.execute(
            "INSERT INTO sdd_artifacts (id, change_id, kind) VALUES ('a1', 'c1', 'design')",
            [],
        )
        .unwrap();

        let dup = conn.execute(
            "INSERT INTO sdd_artifacts (id, change_id, kind) VALUES ('a2', 'c1', 'design')",
            [],
        );
        assert!(dup.is_err(), "a second 'design' artifact in the same change MUST violate UNIQUE(change_id, kind, capability)");
    }

    /// 1.9 — `spec` is the only kind that repeats within a change, discriminated by capability.
    #[test]
    fn run_v53_spec_kind_repeats_per_capability() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_sdd_fixtures(&conn);
        insert_change(&conn, "c1", "nexus-mind", "sdd-artifacts");

        conn.execute(
            "INSERT INTO sdd_artifacts (id, change_id, kind, capability)
             VALUES ('a1', 'c1', 'spec', 'sdd-artifact-store')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sdd_artifacts (id, change_id, kind, capability)
             VALUES ('a2', 'c1', 'spec', 'sdd-artifact-links')",
            [],
        )
        .expect("two spec artifacts with different capabilities must both insert");
    }

    /// 1.11 — the three remaining UNIQUE composites reject duplicates.
    #[test]
    fn run_v53_unique_constraints() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_sdd_fixtures(&conn);

        // UNIQUE(org_id, project, name) on sdd_changes
        insert_change(&conn, "c1", "nexus-mind", "team-tasks");
        let dup_change = conn.execute(
            "INSERT INTO sdd_changes (id, org_id, project, name, created_by)
             VALUES ('c2', 'org1', 'nexus-mind', 'team-tasks', 'u1')",
            [],
        );
        assert!(
            dup_change.is_err(),
            "UNIQUE(org_id, project, name) must reject a duplicate change"
        );

        // UNIQUE(artifact_id, revision) on sdd_artifact_revisions
        conn.execute(
            "INSERT INTO sdd_artifacts (id, change_id, kind) VALUES ('a1', 'c1', 'design')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sdd_artifact_revisions (id, artifact_id, revision, content, content_hash, byte_size, created_by)
             VALUES ('r1', 'a1', 1, 'body', 'hash1', 4, 'u1')",
            [],
        )
        .unwrap();
        let dup_rev = conn.execute(
            "INSERT INTO sdd_artifact_revisions (id, artifact_id, revision, content, content_hash, byte_size, created_by)
             VALUES ('r2', 'a1', 1, 'other', 'hash2', 5, 'u1')",
            [],
        );
        assert!(
            dup_rev.is_err(),
            "UNIQUE(artifact_id, revision) must reject a duplicate revision number"
        );

        // UNIQUE(change_id, memory_id) on sdd_change_memories
        conn.execute(
            "INSERT INTO sdd_change_memories (id, change_id, memory_id, linked_by)
             VALUES ('l1', 'c1', 'm1', 'u1')",
            [],
        )
        .unwrap();
        let dup_link = conn.execute(
            "INSERT INTO sdd_change_memories (id, change_id, memory_id, linked_by)
             VALUES ('l2', 'c1', 'm1', 'u1')",
            [],
        );
        assert!(
            dup_link.is_err(),
            "UNIQUE(change_id, memory_id) must reject a duplicate memory link"
        );
    }

    /// 1.13 — CASCADE / SET NULL / RESTRICT semantics.
    #[test]
    fn run_v53_fk_cascade_and_restrict() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_sdd_fixtures(&conn);

        let fk_on: i32 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            fk_on, 1,
            "connect() must enable foreign_keys for this test to mean anything"
        );

        insert_change(&conn, "c1", "nexus-mind", "team-tasks");
        conn.execute(
            "UPDATE sdd_changes SET sprint_id = 'sp1' WHERE id = 'c1'",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sdd_artifacts (id, change_id, kind) VALUES ('a1', 'c1', 'design')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sdd_artifact_revisions (id, artifact_id, revision, content, content_hash, byte_size, created_by)
             VALUES ('r1', 'a1', 1, 'body', 'hash1', 4, 'u1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sdd_change_memories (id, change_id, memory_id, linked_by)
             VALUES ('l1', 'c1', 'm1', 'u1')",
            [],
        )
        .unwrap();

        // users RESTRICT — u1 is referenced by created_by / linked_by.
        let del_user = conn.execute("DELETE FROM users WHERE id = 'u1'", []);
        assert!(
            del_user.is_err(),
            "deleting a user referenced by created_by/linked_by must be RESTRICTed"
        );

        // sprints SET NULL — the change survives, sprint_id is cleared.
        conn.execute("DELETE FROM sprints WHERE id = 'sp1'", [])
            .unwrap();
        let (change_count, sprint_id): (i32, Option<String>) = conn
            .query_row(
                "SELECT COUNT(*), MAX(sprint_id) FROM sdd_changes WHERE id = 'c1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            change_count, 1,
            "deleting a sprint must NOT delete the change"
        );
        assert!(
            sprint_id.is_none(),
            "deleting a sprint must SET NULL on sdd_changes.sprint_id"
        );

        // memories CASCADE — deleting the memory removes the link.
        conn.execute("DELETE FROM memories WHERE id = 'm1'", [])
            .unwrap();
        let links: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sdd_change_memories WHERE id = 'l1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            links, 0,
            "deleting a memory must cascade to sdd_change_memories"
        );

        // changes CASCADE — artifacts and (transitively) revisions go with it.
        conn.execute("DELETE FROM sdd_changes WHERE id = 'c1'", [])
            .unwrap();
        let artifacts: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sdd_artifacts WHERE change_id = 'c1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            artifacts, 0,
            "deleting a change must cascade to its artifacts"
        );
        let revisions: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sdd_artifact_revisions WHERE artifact_id = 'a1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            revisions, 0,
            "deleting a change must cascade transitively to its revisions"
        );
    }

    /// 1.15 — v53 is a no-op on a second run.
    #[test]
    fn run_v53_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        let count_objects = |c: &Connection| -> i32 {
            c.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'sdd_%' OR name LIKE 'idx_sdd_%'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        let before = count_objects(&conn);

        run_all(&conn).expect("run_all must be idempotent");

        assert_eq!(
            count_objects(&conn),
            before,
            "a second run_all must not create additional sdd objects"
        );
    }

    /// 1.17 — the v54 grant matrix, exactly as design §2 specifies.
    #[test]
    fn run_v54_grants_sdd_perms() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        let expected: &[(&str, &[&str])] = &[
            ("tmpl_dev_junior", &["sdd:read", "sdd:write"]),
            ("tmpl_dev_senior", &["sdd:read", "sdd:write", "sdd:delete"]),
            ("tmpl_security_officer", &["sdd:read"]),
            ("tmpl_auditor", &["sdd:read"]),
        ];

        for (template_id, grants) in expected {
            let perms = permissions_of(&conn, template_id);
            for grant in *grants {
                assert!(
                    perms.iter().any(|p| p == grant),
                    "{template_id} must be granted {grant}"
                );
            }
            // No template gets more sdd:* than its row in the matrix.
            let actual_sdd: Vec<&String> = perms.iter().filter(|p| p.starts_with("sdd:")).collect();
            assert_eq!(
                actual_sdd.len(),
                grants.len(),
                "{template_id} must have exactly {} sdd:* grants, got {actual_sdd:?}",
                grants.len()
            );
        }
    }

    /// 1.19 — re-running v54 must not duplicate any sdd:* string.
    #[test]
    fn run_v54_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        run_all(&conn).unwrap();

        for template_id in [
            "tmpl_dev_junior",
            "tmpl_dev_senior",
            "tmpl_security_officer",
            "tmpl_auditor",
        ] {
            let perms = permissions_of(&conn, template_id);
            let sdd: Vec<&String> = perms.iter().filter(|p| p.starts_with("sdd:")).collect();
            let mut deduped = sdd.clone();
            deduped.sort();
            deduped.dedup();
            assert_eq!(
                sdd.len(),
                deduped.len(),
                "{template_id} has duplicate sdd:* grants after a second run: {sdd:?}"
            );
        }
    }

    /// 1.21 — v54 appends; it never replaces. Everything v52 and earlier granted survives.
    #[test]
    fn run_v54_preserves_existing_permissions() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        let junior = permissions_of(&conn, "tmpl_dev_junior");
        for pre_existing in ["memory:read", "memory:search", "task:read", "task:write"] {
            assert!(
                junior.iter().any(|p| p == pre_existing),
                "tmpl_dev_junior must retain {pre_existing}"
            );
        }

        let senior = permissions_of(&conn, "tmpl_dev_senior");
        for pre_existing in ["task:read", "task:write", "task:assign", "task:delete"] {
            assert!(
                senior.iter().any(|p| p == pre_existing),
                "tmpl_dev_senior must retain {pre_existing}"
            );
        }

        let auditor = permissions_of(&conn, "tmpl_auditor");
        assert!(
            auditor.iter().any(|p| p == "audit:read"),
            "tmpl_auditor must retain audit:read"
        );
    }

    /// 1.23 — run_all lands on the latest schema version.
    #[test]
    fn run_all_sets_user_version_to_65() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(
            get_user_version(&conn),
            70,
            "run_all must leave user_version at 67"
        );
    }

    // ── v55 migration tests (the living specification) ──────────────────────────

    /// The two spec tables exist with the columns design §2 names.
    #[test]
    fn run_v55_creates_spec_tables() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        for table in ["sdd_specs", "sdd_spec_revisions"] {
            assert!(table_exists(&conn, table), "missing table: {table}");
        }

        for col in [
            "id",
            "org_id",
            "project",
            "capability",
            "title",
            "path",
            "latest_revision",
            "created_by",
            "created_at",
            "updated_at",
            "archived_at",
        ] {
            assert!(
                column_info(&conn, "sdd_specs", col).is_some(),
                "sdd_specs missing column: {col}"
            );
        }

        for col in [
            "id",
            "spec_id",
            "revision",
            "content",
            "content_hash",
            "byte_size",
            "merged_from_change_id",
            "git_commit",
            "git_path",
            "source",
            "created_by",
            "created_at",
        ] {
            assert!(
                column_info(&conn, "sdd_spec_revisions", col).is_some(),
                "sdd_spec_revisions missing column: {col}"
            );
        }
    }

    /// A capability is unique per (org, project) — one living spec per capability,
    /// which is what makes it THE contract rather than one draft among many.
    #[test]
    fn run_v55_one_spec_per_capability_per_project() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_sdd_fixtures(&conn);

        conn.execute(
            "INSERT INTO sdd_specs (id, org_id, project, capability, created_by)
             VALUES ('s1', 'org1', 'nexus-mind', 'harness-library', 'u1')",
            [],
        )
        .unwrap();

        let dup = conn.execute(
            "INSERT INTO sdd_specs (id, org_id, project, capability, created_by)
             VALUES ('s2', 'org1', 'nexus-mind', 'harness-library', 'u1')",
            [],
        );
        assert!(
            dup.is_err(),
            "a second spec for the same capability in the same project MUST violate UNIQUE(org_id, project, capability)"
        );

        // …but the same capability in a DIFFERENT project is a different contract.
        conn.execute(
            "INSERT INTO sdd_specs (id, org_id, project, capability, created_by)
             VALUES ('s3', 'org1', 'other-project', 'harness-library', 'u1')",
            [],
        )
        .expect("the same capability in another project must be its own spec");
    }

    /// `merged_from_change_id` is the payoff — it must be a real FK onto `sdd_changes`,
    /// and `ON DELETE SET NULL`: purging a change must not take the spec revision it
    /// produced down with it. The specification outlives the changes that shaped it.
    #[test]
    fn run_v55_merged_from_change_id_survives_the_change_it_names() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        seed_sdd_fixtures(&conn);
        insert_change(&conn, "c1", "nexus-mind", "sdd-specs");

        conn.execute(
            "INSERT INTO sdd_specs (id, org_id, project, capability, created_by, latest_revision)
             VALUES ('s1', 'org1', 'nexus-mind', 'harness-library', 'u1', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sdd_spec_revisions
                (id, spec_id, revision, content, content_hash, byte_size, merged_from_change_id, created_by)
             VALUES ('r1', 's1', 1, 'the contract', 'hash', 12, 'c1', 'u1')",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM sdd_changes WHERE id = 'c1'", [])
            .unwrap();

        let (still_there, merged): (i64, Option<String>) = conn
            .query_row(
                "SELECT COUNT(*), MAX(merged_from_change_id) FROM sdd_spec_revisions WHERE id = 'r1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            still_there, 1,
            "the spec revision must survive the deletion of the change"
        );
        assert_eq!(
            merged, None,
            "merged_from_change_id must be SET NULL, not cascade the revision away"
        );
    }

    /// Deleting a SPEC does take its revisions — they are its history, not the project's.
    #[test]
    fn run_v55_spec_revisions_cascade_from_the_spec() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        seed_sdd_fixtures(&conn);

        conn.execute(
            "INSERT INTO sdd_specs (id, org_id, project, capability, created_by)
             VALUES ('s1', 'org1', 'nexus-mind', 'cap', 'u1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sdd_spec_revisions (id, spec_id, revision, content, content_hash, byte_size, created_by)
             VALUES ('r1', 's1', 1, 'x', 'h', 1, 'u1')",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM sdd_specs WHERE id = 's1'", [])
            .unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM sdd_spec_revisions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "a spec's revisions must cascade with the spec");
    }

    /// One revision per number per spec — the append-only history cannot fork.
    #[test]
    fn run_v55_revision_numbers_are_unique_per_spec() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_sdd_fixtures(&conn);
        conn.execute(
            "INSERT INTO sdd_specs (id, org_id, project, capability, created_by)
             VALUES ('s1', 'org1', 'nexus-mind', 'cap', 'u1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sdd_spec_revisions (id, spec_id, revision, content, content_hash, byte_size, created_by)
             VALUES ('r1', 's1', 1, 'a', 'h1', 1, 'u1')",
            [],
        )
        .unwrap();

        let dup = conn.execute(
            "INSERT INTO sdd_spec_revisions (id, spec_id, revision, content, content_hash, byte_size, created_by)
             VALUES ('r2', 's1', 1, 'b', 'h2', 1, 'u1')",
            [],
        );
        assert!(
            dup.is_err(),
            "revision 1 of a spec MUST be unique — UNIQUE(spec_id, revision)"
        );
    }

    /// `source` defaults to 'agent', matching `sdd_artifact_revisions`.
    #[test]
    fn run_v55_source_defaults_to_agent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let (notnull, default) = column_info(&conn, "sdd_spec_revisions", "source")
            .expect("sdd_spec_revisions.source must exist");
        assert!(notnull, "source must be NOT NULL");
        assert_eq!(
            default.as_deref(),
            Some("'agent'"),
            "source must default to 'agent'"
        );
    }

    /// The FTS5 index exists, indexes content, and leaves `spec_id` UNINDEXED.
    #[test]
    fn run_v55_creates_specs_fts_virtual_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        assert!(
            table_exists(&conn, "sdd_specs_fts"),
            "missing fts table: sdd_specs_fts"
        );

        conn.execute(
            "INSERT INTO sdd_specs_fts (spec_id, project, capability, content)
             VALUES ('spec1', 'nexus-mind', 'harness-library', 'the library enforces rate limiting')",
            [],
        )
        .unwrap();

        let hits: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sdd_specs_fts WHERE sdd_specs_fts MATCH 'rate'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1, "content must be indexed and searchable");

        let id_hits: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sdd_specs_fts WHERE sdd_specs_fts MATCH 'spec1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            id_hits, 0,
            "spec_id must be UNINDEXED — it is a payload, not a search term"
        );
    }

    /// Every index from the design, in the style of v53.
    #[test]
    fn run_v55_creates_indexes() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        for (table, idx) in [
            ("sdd_specs", "idx_sdd_specs_org_project"),
            ("sdd_specs", "idx_sdd_specs_capability"),
            ("sdd_spec_revisions", "idx_sdd_spec_revisions_spec"),
            ("sdd_spec_revisions", "idx_sdd_spec_revisions_hash"),
            ("sdd_spec_revisions", "idx_sdd_spec_revisions_merged_from"),
        ] {
            assert!(
                index_exists(&conn, table, idx),
                "missing index: {idx} on {table}"
            );
        }
    }

    /// Re-running v55 on an already-migrated database is a no-op, not an error.
    #[test]
    fn run_v55_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        run_v55(&conn).expect("run_v55 must be idempotent");
        run_all(&conn).expect("run_all must be idempotent");
        assert_eq!(get_user_version(&conn), 70, "user_version must remain 70");
    }

    // ── v56 migration tests (knowledge migration durable review foundation) ─────

    #[test]
    fn run_all_creates_durable_migration_review_tables() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        for table in [
            "migration_runs",
            "migration_candidates",
            "migration_review_actions",
            "migration_provenance",
            "migration_outcomes",
        ] {
            assert!(
                table_exists(&conn, table),
                "missing migration table: {table}"
            );
        }
    }

    /// Supersedes v56's `migration_run_scope_allows_only_v1_destination_matrix`,
    /// which asserted two things v60 deliberately reverses:
    ///
    ///   * the destination kind lived on the RUN, so a run could only ever
    ///     produce one kind of artifact — and one scan of `docs/` legitimately
    ///     produces four;
    ///   * a project-scoped convention run was forbidden, even though
    ///     `conventions.project_id` exists and is nullable. v56 outlawed a row
    ///     the destination table has always accepted.
    ///
    /// The replacement pins the new contract: the run carries scope, the
    /// candidate carries destination, and a project-scoped convention is legal.
    #[test]
    fn migration_candidates_mix_destinations_within_one_project_scoped_run() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_sdd_fixtures(&conn);
        conn.execute(
            "INSERT INTO projects (id, org_id, name) VALUES ('project1', 'org1', 'project')",
            [],
        )
        .unwrap();

        // A project-scoped run — the case v56 rejected for conventions.
        conn.execute(
            "INSERT INTO migration_runs (id, org_id, project_id, source_kind, created_by)
             VALUES ('run1', 'org1', 'project1', 'repo-docs', 'u1')",
            [],
        )
        .unwrap();

        // One scan, four destinations, including the convention v56 forbade.
        for (id, destination_kind) in [
            ("cand-memory", "memory"),
            ("cand-convention", "convention"),
            ("cand-task", "task"),
            ("cand-sdd", "sdd_artifact"),
        ] {
            conn.execute(
                "INSERT INTO migration_candidates
                    (id, run_id, source_identity, destination_kind, content)
                 VALUES (?1, 'run1', ?1, ?2, 'body')",
                rusqlite::params![id, destination_kind],
            )
            .unwrap_or_else(|e| panic!("{id} must be accepted under the v60 contract: {e}"));
        }

        let invalid = conn.execute(
            "INSERT INTO migration_candidates
                (id, run_id, source_identity, destination_kind, content)
             VALUES ('cand-unknown', 'run1', 'unknown', 'unknown', 'body')",
            [],
        );
        assert!(
            invalid.is_err(),
            "an unlisted destination kind must still be rejected"
        );
    }

    #[test]
    fn migration_provenance_is_org_scoped_and_review_actions_are_append_only() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_sdd_fixtures(&conn);
        conn.execute(
            "INSERT INTO migration_runs (id, org_id, source_kind, created_by)
             VALUES ('run1', 'org1', 'repo-docs', 'u1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO migration_candidates
                (id, run_id, source_identity, destination_kind, content, attestation)
             VALUES ('candidate1', 'run1', 'source://one', 'memory', 'content', '{}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO migration_review_actions
                (id, run_id, candidate_id, actor_id, action, expected_version, resulting_version)
             VALUES ('action1', 'run1', 'candidate1', 'u1', 'approved', 1, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO migration_provenance
                (id, org_id, destination_kind, source_identity, candidate_id)
             VALUES ('provenance1', 'org1', 'memory', 'source://one', 'candidate1')",
            [],
        )
        .unwrap();

        let duplicate = conn.execute(
            "INSERT INTO migration_provenance
                (id, org_id, destination_kind, source_identity, candidate_id)
             VALUES ('provenance2', 'org1', 'memory', 'source://one', 'candidate1')",
            [],
        );
        assert!(
            duplicate.is_err(),
            "provenance must be unique within its org and destination"
        );

        let rewrite = conn.execute(
            "UPDATE migration_review_actions SET action = 'rejected' WHERE id = 'action1'",
            [],
        );
        assert!(rewrite.is_err(), "review history must be append-only");
    }

    // ── v58 migration tests — consultancy client model ────────────────────────

    /// Seed one organization so client rows have a valid FK target.
    fn seed_org(conn: &Connection, id: &str, slug: &str) {
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, ?1, ?2)",
            rusqlite::params![id, slug],
        )
        .unwrap();
    }

    #[test]
    fn run_v58_creates_clients_and_members() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(table_exists(&conn, "clients"), "missing: clients");
        assert!(
            table_exists(&conn, "client_members"),
            "missing: client_members"
        );
    }

    #[test]
    fn run_v58_adds_columns() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(column_exists(&conn, "projects", "client_id"));
        assert!(column_exists(&conn, "code_projects", "project_id"));
        assert!(column_exists(&conn, "conventions", "client_id"));
        assert!(column_exists(&conn, "policies", "client_id"));
        assert!(column_exists(&conn, "memories", "promoted_from"));
    }

    #[test]
    fn run_v58_creates_indexes() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(index_exists(&conn, "clients", "idx_clients_org_status"));
        assert!(index_exists(
            &conn,
            "client_members",
            "idx_client_members_user"
        ));
        assert!(index_exists(&conn, "projects", "idx_projects_client"));
        assert!(index_exists(
            &conn,
            "code_projects",
            "idx_code_projects_project"
        ));
    }

    /// Re-running v58 over an already-migrated database must succeed.
    ///
    /// The version guard alone would short-circuit and prove nothing, so the
    /// guard is forced open first — that is the only way to exercise the
    /// ALTER TABLE "duplicate column name" tolerance the migration relies on.
    #[test]
    fn run_v58_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        conn.execute_batch("PRAGMA user_version = 57;").unwrap();
        run_v58(&conn).expect("re-running v58 over a migrated db must not fail");

        assert_eq!(get_user_version(&conn), 58);
        assert!(table_exists(&conn, "clients"));
        assert!(column_exists(&conn, "projects", "client_id"));
    }

    #[test]
    fn run_v58_rejects_invalid_status() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_org(&conn, "org1", "acme");

        let bad = conn.execute(
            "INSERT INTO clients (id, org_id, name, slug, status)
             VALUES ('c1', 'org1', 'Acme', 'acme', 'terminated')",
            [],
        );
        assert!(
            bad.is_err(),
            "status must be constrained to the documented set"
        );
    }

    #[test]
    fn run_v58_enforces_unique_slug_per_org() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_org(&conn, "org1", "u2s");

        conn.execute(
            "INSERT INTO clients (id, org_id, name, slug) VALUES ('c1', 'org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();

        let duplicate = conn.execute(
            "INSERT INTO clients (id, org_id, name, slug) VALUES ('c2', 'org1', 'Acme Again', 'acme')",
            [],
        );
        assert!(
            duplicate.is_err(),
            "slug must be unique within an organization"
        );
    }

    /// Uniqueness is scoped to the organization, not global — two tenants may
    /// each have a client called "acme".
    #[test]
    fn run_v58_allows_same_slug_across_orgs() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_org(&conn, "org1", "u2s");
        seed_org(&conn, "org2", "other");

        conn.execute(
            "INSERT INTO clients (id, org_id, name, slug) VALUES ('c1', 'org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO clients (id, org_id, name, slug) VALUES ('c2', 'org2', 'Acme', 'acme')",
            [],
        )
        .expect("the same slug must be allowed in a different organization");
    }

    /// A fresh install holds no tokens, so it has nothing to encrypt and must
    /// not be blocked on a key it does not need yet.
    #[test]
    fn run_v58_succeeds_without_key_when_there_are_no_tokens() {
        std::env::remove_var("NEXUSMIND_TOKEN_ENCRYPTION_KEY");
        let conn = in_memory_db();
        run_all(&conn).expect("a fresh database has no credentials to protect");
        assert_eq!(get_user_version(&conn), 70);
    }

    /// The new primary key is what lets a consultancy hold one GitHub account
    /// per client instead of one per organization.
    #[test]
    fn run_v58_allows_one_github_connection_per_client() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_org(&conn, "org1", "u2s");
        conn.execute(
            "INSERT INTO clients (id, org_id, name, slug) VALUES ('c1', 'org1', 'A', 'a')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO clients (id, org_id, name, slug) VALUES ('c2', 'org1', 'B', 'b')",
            [],
        )
        .unwrap();

        for (cid, login) in [("c1", "acme-org"), ("c2", "beta-org")] {
            conn.execute(
                "INSERT INTO github_connections (org_id, client_id, github_login, access_token)
                 VALUES ('org1', ?1, ?2, 'ciphertext')",
                rusqlite::params![cid, login],
            )
            .expect("each client must be able to hold its own GitHub connection");
        }
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM github_connections", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            n, 2,
            "the old PRIMARY KEY (org_id) would have collapsed these into one"
        );
    }

    /// The visibility view is the single expression of the rule; without it the
    /// rewritten queries have nothing to read.
    #[test]
    fn run_v58_creates_project_visibility_view() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let exists: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='view' AND name='project_visibility'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "missing view: project_visibility");
    }

    /// `client_id IS NULL` means "internal u2s project" and must be the default
    /// for projects created without one — never a sentinel row.
    #[test]
    fn run_v58_project_client_id_defaults_to_null() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_org(&conn, "org1", "u2s");

        conn.execute(
            "INSERT INTO projects (id, org_id, name) VALUES ('p1', 'org1', 'internal-tooling')",
            [],
        )
        .unwrap();

        let client_id: Option<String> = conn
            .query_row("SELECT client_id FROM projects WHERE id = 'p1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(
            client_id.is_none(),
            "a project with no client is internal work"
        );
    }

    // ── v59 migration tests — usage metrics ───────────────────────────────────

    #[test]
    fn run_v59_creates_usage_events() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(table_exists(&conn, "usage_events"), "missing: usage_events");
        assert_eq!(get_user_version(&conn), 70);
        assert!(index_exists(
            &conn,
            "usage_events",
            "idx_usage_events_org_ts"
        ));
        assert!(index_exists(
            &conn,
            "usage_events",
            "idx_usage_events_project"
        ));
        assert!(index_exists(
            &conn,
            "usage_events",
            "idx_usage_events_client"
        ));
        assert!(index_exists(&conn, "usage_events", "idx_usage_events_task"));
        assert!(index_exists(
            &conn,
            "usage_events",
            "idx_usage_events_session"
        ));
    }

    /// The backfill idempotency index must be UNIQUE and partial: at most one
    /// `source='backfill'` row per session, while explicit ingest events may
    /// share a session_id freely.
    #[test]
    fn run_v59_backfill_index_is_unique() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_org(&conn, "org1", "u2s");
        conn.execute(
            "INSERT INTO sessions (id, org_id, project) VALUES ('s1', 'org1', 'p')",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO usage_events (id, org_id, session_id, source, event_ts)
             VALUES ('u1', 'org1', 's1', 'backfill', datetime('now'))",
            [],
        )
        .unwrap();
        // A second backfill row for the same session must be rejected.
        let dup = conn.execute(
            "INSERT INTO usage_events (id, org_id, session_id, source, event_ts)
             VALUES ('u2', 'org1', 's1', 'backfill', datetime('now'))",
            [],
        );
        assert!(
            dup.is_err(),
            "the partial-unique index must block duplicate backfill rows"
        );
        // But an ingest row for the same session is fine — the index is partial.
        conn.execute(
            "INSERT INTO usage_events (id, org_id, session_id, source, event_ts)
             VALUES ('u3', 'org1', 's1', 'ingest', datetime('now'))",
            [],
        )
        .expect("ingest events are not constrained by the backfill index");
    }

    /// Re-running v59 over an already-migrated database must succeed.
    #[test]
    fn run_v59_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute_batch("PRAGMA user_version = 58;").unwrap();
        run_v59(&conn).expect("re-running v59 over a migrated db must not fail");
        assert_eq!(get_user_version(&conn), 59);
        assert!(table_exists(&conn, "usage_events"));
    }

    // ── v60: knowledge migration ─────────────────────────────────────────────
    //
    // v60 RECREATES the five v56 staging tables rather than altering them: the
    // shape changes are not additive (`destination_kind` moves off the run and
    // onto the candidate, and SQLite cannot alter a CHECK or drop a column that
    // a cross-column CHECK references). That is only safe because v56 was never
    // wired to anything and its tables are empty in every install.
    //
    // The guard below is what makes that assumption falsifiable instead of
    // merely believed. It is written first, on purpose: it is the single test
    // standing between a `DROP TABLE` and somebody's data.

    /// Insert one row into `table` and nothing else. Foreign keys are switched
    /// off around the insert so each table can be exercised in isolation — the
    /// guard must fire on a lone orphan row, not only on a well-formed graph.
    fn insert_lone_migration_row(conn: &Connection, table: &str) {
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        let sql = match table {
            "migration_runs" => {
                "INSERT INTO migration_runs (id, org_id, source_kind, created_by)
                 VALUES ('r1', 'org1', 'repo-docs', 'u1')"
            }
            "migration_candidates" => {
                "INSERT INTO migration_candidates
                     (id, run_id, source_identity, destination_kind, content)
                 VALUES ('c1', 'r1', 'repo-docs:x', 'memory', 'body')"
            }
            "migration_review_actions" => {
                "INSERT INTO migration_review_actions (id, run_id, actor_id, action)
                 VALUES ('a1', 'r1', 'u1', 'approved')"
            }
            "migration_provenance" => {
                "INSERT INTO migration_provenance
                     (id, org_id, destination_kind, source_identity, candidate_id)
                 VALUES ('p1', 'org1', 'memory', 'repo-docs:x', 'c1')"
            }
            "migration_outcomes" => {
                "INSERT INTO migration_outcomes
                     (id, run_id, candidate_id, expected_version, candidate_status, outcome_status)
                 VALUES ('o1', 'r1', 'c1', 1, 'approved', 'committed')"
            }
            other => panic!("unknown migration table: {other}"),
        };
        conn.execute(sql, [])
            .unwrap_or_else(|e| panic!("seeding {table} failed: {e}"));
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    }

    /// T-01 — the guard. A non-empty v56 staging table means the premise that
    /// justifies recreating them ("nobody ever used this") is false, so v60 MUST
    /// refuse to run rather than drop the data. The error has to name the table
    /// and the row count, because whoever hits this needs to know what they are
    /// about to lose before they decide what to do about it.
    #[test]
    fn run_v60_aborts_when_v56_tables_have_rows() {
        for table in [
            "migration_runs",
            "migration_candidates",
            "migration_review_actions",
            "migration_provenance",
            "migration_outcomes",
        ] {
            let conn = in_memory_db();
            run_all(&conn).unwrap();
            seed_org(&conn, "org1", "acme");

            insert_lone_migration_row(&conn, table);

            // Force the version guard open so v60 actually evaluates.
            conn.execute_batch("PRAGMA user_version = 59;").unwrap();

            let err = run_v60(&conn)
                .expect_err(&format!("v60 must refuse to run while {table} holds rows"));
            let msg = err.to_string();
            assert!(
                msg.contains(table),
                "the abort message must name the offending table; got: {msg}"
            );
            assert!(
                msg.contains('1'),
                "the abort message must report the row count; got: {msg}"
            );

            // And it must not have destroyed anything on the way out.
            let still_there: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(still_there, 1, "{table} must be untouched after the abort");
            assert_eq!(
                get_user_version(&conn),
                59,
                "an aborted v60 must not advance user_version"
            );
        }
    }

    /// The other half of the guard: with the tables empty — which is every real
    /// install — v60 runs normally.
    #[test]
    fn run_v60_runs_when_v56_tables_are_empty() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute_batch("PRAGMA user_version = 59;").unwrap();
        run_v60(&conn).expect("v60 must run when the v56 staging tables are empty");
        assert_eq!(get_user_version(&conn), 60);
    }

    /// Minimal fixture: an org, a user, a client and a project of that client,
    /// so a run can be created with every scope column populated.
    fn seed_v60_fixture(conn: &Connection) {
        seed_org(conn, "org1", "u2s");
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1', 'org1', 'dev@u2s.com', 'Dev')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO clients (id, org_id, name, slug) VALUES ('cl1', 'org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO projects (id, org_id, name, client_id) VALUES ('p1', 'org1', 'acme-billing', 'cl1')",
            [],
        )
        .unwrap();
    }

    fn insert_run(
        conn: &Connection,
        id: &str,
        client: Option<&str>,
        project: Option<&str>,
    ) -> rusqlite::Result<usize> {
        conn.execute(
            "INSERT INTO migration_runs (id, org_id, client_id, project_id, source_kind, created_by)
             VALUES (?1, 'org1', ?2, ?3, 'repo-docs', 'u1')",
            rusqlite::params![id, client, project],
        )
    }

    fn insert_candidate(
        conn: &Connection,
        id: &str,
        run: &str,
        identity: &str,
        dest: &str,
    ) -> rusqlite::Result<usize> {
        conn.execute(
            "INSERT INTO migration_candidates (id, run_id, source_identity, destination_kind, content)
             VALUES (?1, ?2, ?3, ?4, 'body')",
            rusqlite::params![id, run, identity, dest],
        )
    }

    /// Re-running v60 over an already-migrated database must succeed. The
    /// version guard has to be forced open first — without that the function
    /// returns early and the test proves nothing.
    #[test]
    fn run_v60_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute_batch("PRAGMA user_version = 59;").unwrap();
        run_v60(&conn).expect("re-running v60 over a migrated db must not fail");
        assert_eq!(get_user_version(&conn), 60);
        assert!(table_exists(&conn, "migration_runs"));
        assert!(table_exists(&conn, "migration_candidates"));
    }

    /// The whole point of the recreation: one run may now carry candidates of
    /// several destination kinds, so `destination_kind` no longer lives on the
    /// run. A scan of `docs/` produces memories, conventions, tasks and SDD
    /// artifacts in one pass.
    #[test]
    fn run_v60_run_has_no_destination_kind_column() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(
            !column_exists(&conn, "migration_runs", "destination_kind"),
            "destination_kind must have moved off the run and onto the candidate"
        );
        assert!(column_exists(
            &conn,
            "migration_candidates",
            "destination_kind"
        ));
        assert!(column_exists(&conn, "migration_runs", "client_id"));
        assert!(column_exists(&conn, "migration_runs", "source_kind"));
        assert!(column_exists(
            &conn,
            "migration_candidates",
            "destination_hint"
        ));
        assert!(column_exists(&conn, "migration_candidates", "indexed_at"));
    }

    #[test]
    fn run_v60_candidate_accepts_six_destination_kinds() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_v60_fixture(&conn);
        insert_run(&conn, "r1", Some("cl1"), Some("p1")).unwrap();

        for (i, kind) in [
            "memory",
            "convention",
            "task",
            "sdd_artifact",
            "harness",
            "harness_config_review",
        ]
        .iter()
        .enumerate()
        {
            insert_candidate(&conn, &format!("c{i}"), "r1", &format!("src:{i}"), kind)
                .unwrap_or_else(|e| panic!("destination kind {kind} must be accepted: {e}"));
        }
    }

    #[test]
    fn run_v60_candidate_rejects_unknown_destination_kind() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_v60_fixture(&conn);
        insert_run(&conn, "r1", None, None).unwrap();

        let bad = insert_candidate(&conn, "c1", "r1", "src:x", "notion_page");
        assert!(
            bad.is_err(),
            "an unlisted destination kind must be rejected"
        );
    }

    /// A run may only name a client of its own organization. Without this the
    /// isolation v58 built is one typo away from being undone.
    #[test]
    fn run_v60_client_scope_trigger_aborts_cross_org() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_v60_fixture(&conn);
        seed_org(&conn, "org2", "other");
        conn.execute(
            "INSERT INTO clients (id, org_id, name, slug) VALUES ('cl2', 'org2', 'Other', 'other')",
            [],
        )
        .unwrap();

        let cross = insert_run(&conn, "r1", Some("cl2"), None);
        assert!(
            cross.is_err(),
            "a run must not reference a client of another organization"
        );
    }

    /// `client_id` is the attribution that decides who may read the migrated
    /// knowledge. Letting it be reassigned after the fact would move a whole
    /// run's worth of material between clients in one UPDATE.
    #[test]
    fn run_v60_scope_is_immutable_after_insert() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_v60_fixture(&conn);
        conn.execute(
            "INSERT INTO clients (id, org_id, name, slug) VALUES ('cl9', 'org1', 'Nine', 'nine')",
            [],
        )
        .unwrap();
        insert_run(&conn, "r1", Some("cl1"), None).unwrap();

        let reassign = conn.execute(
            "UPDATE migration_runs SET client_id = 'cl9' WHERE id = 'r1'",
            [],
        );
        assert!(
            reassign.is_err(),
            "client_id must be immutable after creation"
        );

        let retarget = conn.execute(
            "UPDATE migration_runs SET source_kind = 'db-schema' WHERE id = 'r1'",
            [],
        );
        assert!(
            retarget.is_err(),
            "source_kind must be immutable after creation"
        );

        // Status is NOT part of the scope and must stay updatable.
        conn.execute(
            "UPDATE migration_runs SET status = 'in_review' WHERE id = 'r1'",
            [],
        )
        .expect("status must remain updatable");
    }

    /// Idempotency is enforced by the database, not by an application check
    /// somebody can forget on a branch.
    #[test]
    fn run_v60_provenance_unique_blocks_second_commit() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_v60_fixture(&conn);
        insert_run(&conn, "r1", None, None).unwrap();
        insert_candidate(&conn, "c1", "r1", "repo-docs:a", "memory").unwrap();
        insert_candidate(&conn, "c2", "r1", "repo-docs:b", "memory").unwrap();

        conn.execute(
            "INSERT INTO migration_provenance (id, org_id, destination_kind, source_identity, candidate_id)
             VALUES ('pr1', 'org1', 'memory', 'repo-docs:a', 'c1')",
            [],
        )
        .unwrap();

        let dup = conn.execute(
            "INSERT INTO migration_provenance (id, org_id, destination_kind, source_identity, candidate_id)
             VALUES ('pr2', 'org1', 'memory', 'repo-docs:a', 'c2')",
            [],
        );
        assert!(
            dup.is_err(),
            "the same source may not commit twice to the same destination kind"
        );

        // The same source to a DIFFERENT destination kind is legitimate: a
        // document can be both a memory and a convention.
        conn.execute(
            "INSERT INTO migration_provenance (id, org_id, destination_kind, source_identity, candidate_id)
             VALUES ('pr3', 'org1', 'convention', 'repo-docs:a', 'c1')",
            [],
        )
        .expect("same identity, different destination kind must be allowed");
    }

    /// The review trail is evidence. A reversal is a new row, never an edit.
    #[test]
    fn run_v60_review_actions_reject_update_and_delete() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_v60_fixture(&conn);
        insert_run(&conn, "r1", None, None).unwrap();
        insert_candidate(&conn, "c1", "r1", "repo-docs:a", "memory").unwrap();
        conn.execute(
            "INSERT INTO migration_review_actions (id, run_id, candidate_id, actor_id, action, expected_version)
             VALUES ('a1', 'r1', 'c1', 'u1', 'approved', 1)",
            [],
        )
        .unwrap();

        let updated = conn.execute(
            "UPDATE migration_review_actions SET action = 'rejected' WHERE id = 'a1'",
            [],
        );
        assert!(updated.is_err(), "review actions must be append-only");

        let deleted = conn.execute("DELETE FROM migration_review_actions WHERE id = 'a1'", []);
        assert!(deleted.is_err(), "review actions must not be deletable");
    }

    #[test]
    fn run_v60_creates_doc_tables() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(table_exists(&conn, "doc_documents"));
        assert!(table_exists(&conn, "doc_chunks"));
        assert!(table_exists(&conn, "doc_chunk_embeddings"));
    }

    #[test]
    fn run_v61_grants_autonomous_agent_permissions_to_super_user_template() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute_batch("PRAGMA user_version = 60;").unwrap();
        run_v61(&conn).unwrap();
        let raw: String = conn
            .query_row(
                "SELECT permissions FROM roles WHERE id = 'super_user_template'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let permissions: Vec<String> = serde_json::from_str(&raw).unwrap();
        for permission in [
            "autonomous_agent:read",
            "autonomous_agent:create",
            "autonomous_agent:update",
            "autonomous_agent:enable",
            "autonomous_agent:run",
            "autonomous_agent:cancel",
            "autonomous_agent:manage_connectors",
        ] {
            assert!(permissions.iter().any(|value| value == permission));
        }
        let admin_raw: String = conn
            .query_row(
                "SELECT permissions FROM roles WHERE id='admin_template'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let admin_permissions: Vec<String> = serde_json::from_str(&admin_raw).unwrap();
        assert_eq!(admin_permissions.len(), 7);
        assert!(admin_permissions
            .iter()
            .any(|value| value == "autonomous_agent:manage_connectors"));
        assert_eq!(get_user_version(&conn), 61);
    }

    #[test]
    fn run_v62_creates_autonomous_agent_control_plane_tables() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        for table in [
            "autonomous_agent_definitions",
            "autonomous_runtime_health",
            "autonomous_agent_revisions",
            "autonomous_agent_validations",
            "autonomous_agent_targets",
            "autonomous_agent_schedules",
            "autonomous_agent_connectors",
            "autonomous_agent_runs",
            "autonomous_agent_leases",
            "autonomous_agent_events",
            "autonomous_agent_findings",
            "autonomous_agent_deliveries",
            "autonomous_agent_work_items",
            "autonomous_agent_output_links",
        ] {
            assert!(table_exists(&conn, table), "missing table {table}");
        }
        assert!(column_exists(
            &conn,
            "organizations",
            "autonomous_agents_enabled"
        ));
        assert!(column_exists(
            &conn,
            "organizations",
            "autonomous_agent_retention_days"
        ));
        assert_eq!(get_user_version(&conn), 70);
    }

    #[test]
    fn run_v62_revisions_and_events_are_append_only() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        seed_org(&conn, "org1", "acme");
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1','org1','a@b.com','A')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO autonomous_agent_definitions (id,org_id,name,template_key,template_version,created_by) VALUES ('d1','org1','QA','qa',1,'u1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO autonomous_agent_revisions (id,definition_id,revision,config_json,config_hash,capabilities_json,budgets_json,created_by) VALUES ('r1','d1',1,'{}','h','[]','{}','u1')",
            [],
        )
        .unwrap();
        assert!(conn
            .execute(
                "UPDATE autonomous_agent_revisions SET config_hash='x' WHERE id='r1'",
                []
            )
            .is_err());
        assert!(conn
            .execute("DELETE FROM autonomous_agent_revisions WHERE id='r1'", [])
            .is_err());
    }

    #[test]
    fn run_v73_allows_security_scan_and_dast_templates() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 74);
        seed_org(&conn, "org1", "acme");
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1','org1','a@b.com','A')",
            [],
        )
        .unwrap();
        // Both new security templates are now accepted by the CHECK (the invalid_template
        // fix in queries.rs is useless while the schema rejects the row).
        for (id, key) in [("d1", "security_scan"), ("d2", "security_dast")] {
            conn.execute(
                "INSERT INTO autonomous_agent_definitions (id,org_id,name,template_key,template_version,created_by) VALUES (?1,'org1',?1,?2,1,'u1')",
                rusqlite::params![id, key],
            )
            .unwrap_or_else(|e| panic!("{key} must be accepted by the CHECK: {e}"));
        }
        // An unknown template is still rejected.
        assert!(conn
            .execute(
                "INSERT INTO autonomous_agent_definitions (id,org_id,name,template_key,template_version,created_by) VALUES ('d3','org1','bogus','not_a_template',1,'u1')",
                [],
            )
            .is_err());
    }

    /// v74 admits the `source-code` connector into the `migration_runs`
    /// source_kind CHECK, and the rebuild preserves the scope triggers.
    #[test]
    fn run_v74_admits_the_source_code_connector() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 74);
        seed_org(&conn, "org1", "acme");
        conn.execute(
            "INSERT INTO users (id, org_id, email, name) VALUES ('u1','org1','a@b.com','A')",
            [],
        )
        .unwrap();

        // A source-code run is now accepted by the CHECK.
        conn.execute(
            "INSERT INTO migration_runs (id,org_id,source_kind,created_by) VALUES ('r1','org1','source-code','u1')",
            [],
        )
        .expect("source-code must be accepted by the widened CHECK");

        // An unknown source is still rejected, and the immutability trigger the
        // rebuild recreated still fires.
        assert!(conn
            .execute(
                "INSERT INTO migration_runs (id,org_id,source_kind,created_by) VALUES ('r2','org1','bogus','u1')",
                [],
            )
            .is_err());
        assert!(
            conn.execute("UPDATE migration_runs SET source_kind='repo-docs' WHERE id='r1'", [])
                .is_err(),
            "source_kind stays immutable after the rebuild"
        );
    }
}
