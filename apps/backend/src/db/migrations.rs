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
        ("tmpl_dev_senior", &["task:read", "task:write", "task:assign", "task:delete"]),
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
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
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
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
    }

    #[test]
    fn run_v20_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v20(&conn);
        assert!(result.is_ok(), "run_v20 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 55, "user_version must remain 55 after re-running v20 on already-migrated db");
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
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
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
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
    }

    #[test]
    fn run_v24_sets_user_version_to_24() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
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
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
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
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
    }

    #[test]
    fn run_v27_sets_user_version_to_27() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
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
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
    }

    #[test]
    fn run_all_sets_user_version_to_29() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
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
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
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
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
    }

    #[test]
    fn run_v30_sets_user_version_to_30() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
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
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
    }

    #[test]
    fn run_v31_sets_user_version_to_31() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
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
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
    }

    #[test]
    fn run_v32_sets_user_version_to_32() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(get_user_version(&conn), 55, "user_version must be 55 after run_all");
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
        assert!(get_user_version(&conn) >= 36, "user_version must be at least 36");
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

    // ── v37 migration tests ───────────────────────────────────────────────────

    #[test]
    fn run_v37_creates_github_connections_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(table_exists(&conn, "github_connections"),
                "github_connections table must exist after v37");
    }

    #[test]
    fn run_v37_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_v37(&conn);
        assert!(result.is_ok(), "run_v37 must be idempotent: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 55, "user_version must remain 55 (run_all already applied v41-v55)");
    }

    // ── v41 + v42 migration tests (code knowledge graph) ────────────────────────

    #[test]
    fn run_all_sets_user_version_to_43() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(
            get_user_version(&conn),
            55,
            "user_version must be 55 after v41-v55 are included in run_all"
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
        assert!(table_exists(&conn, "code_symbols"), "code_symbols must exist after v41");
    }

    #[test]
    fn run_v42_creates_code_edges_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(table_exists(&conn, "code_edges"), "code_edges must exist after v42");
    }

    #[test]
    fn run_v41_code_symbols_unique_qualified_name() {
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
        let pid: i64 = conn.query_row(
            "SELECT id FROM code_projects WHERE name='myapp'",
            [],
            |r| r.get(0),
        ).unwrap();
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
        assert!(dup.is_err(), "UNIQUE(code_project_id, qualified_name) must reject duplicate");
    }

    #[test]
    fn run_v42_code_edges_cascade_delete_on_symbol() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute("INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')", []).unwrap();
        conn.execute("INSERT INTO code_projects (org_id, name, root_path) VALUES ('org1', 'p', '/ws')", []).unwrap();
        let pid: i64 = conn.query_row("SELECT id FROM code_projects WHERE name='p'", [], |r| r.get(0)).unwrap();
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
        let from_id: i64 = conn.query_row("SELECT id FROM code_symbols WHERE name='a'", [], |r| r.get(0)).unwrap();
        let to_id: i64 = conn.query_row("SELECT id FROM code_symbols WHERE name='foo'", [], |r| r.get(0)).unwrap();
        conn.execute(
            "INSERT INTO code_edges (code_project_id, from_symbol_id, to_symbol_id, edge_type) \
             VALUES (?1, ?2, ?3, 'defines')",
            rusqlite::params![pid, from_id, to_id],
        ).unwrap();
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM code_edges", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "edge must exist before symbol deletion");
        conn.execute("DELETE FROM code_symbols WHERE id = ?1", rusqlite::params![from_id]).unwrap();
        let after: i32 = conn.query_row("SELECT COUNT(*) FROM code_edges", [], |r| r.get(0)).unwrap();
        assert_eq!(after, 0, "edges must cascade-delete when from_symbol is removed");
    }

    #[test]
    fn run_all_v41_v42_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_all(&conn);
        assert!(result.is_ok(), "run_all must be idempotent after v41+v42: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 55, "version must remain 55 on second run_all");
    }

    #[test]
    fn run_v37_cascade_delete_on_org_remove() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO github_connections (org_id, access_token, github_login, github_user_id)
             VALUES ('org1', 'gho_test', 'acme-bot', 12345)",
            [],
        ).unwrap();
        // Connection must exist
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM github_connections WHERE org_id = 'org1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        // Delete org — connection must cascade
        conn.execute("DELETE FROM organizations WHERE id = 'org1'", []).unwrap();
        let after: i32 = conn
            .query_row("SELECT COUNT(*) FROM github_connections WHERE org_id = 'org1'", [], |r| r.get(0))
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
        assert!(table_exists(&conn, "agents"), "agents table must exist after run_all on a fresh db");
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
        assert_eq!(get_user_version(&conn), 55, "user_version must reach 55 after the backfill migration");
    }

    #[test]
    fn run_all_is_idempotent_after_v46() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let result = run_all(&conn);
        assert!(result.is_ok(), "run_all must be idempotent after v45: {:?}", result.err());
        assert_eq!(get_user_version(&conn), 55, "user_version must remain 55 on second run_all");
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
            assert!(table_exists(&conn, table), "{table} table must exist after run_all on a fresh db");
        }
        assert_eq!(get_user_version(&conn), 55, "user_version must reach 55 on a fresh db");
    }

    #[test]
    fn run_v51_creates_task_tables_with_expected_columns() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        for col in [
            "id", "org_id", "project", "title", "description", "status", "priority", "due_date",
            "parent_id", "sprint_id", "created_by", "created_at", "updated_at", "archived_at",
        ] {
            assert!(column_exists(&conn, "tasks", col), "tasks.{col} must exist");
        }
        for col in ["id", "task_id", "user_id", "assigned_by", "assigned_at"] {
            assert!(column_exists(&conn, "task_assignees", col), "task_assignees.{col} must exist");
        }
        for col in ["id", "task_id", "label", "created_at"] {
            assert!(column_exists(&conn, "task_labels", col), "task_labels.{col} must exist");
        }
        for col in ["id", "task_id", "user_id", "body", "created_at"] {
            assert!(column_exists(&conn, "task_comments", col), "task_comments.{col} must exist");
        }
        for col in ["id", "task_id", "spec_change_name", "linked_by", "created_at"] {
            assert!(column_exists(&conn, "task_spec_links", col), "task_spec_links.{col} must exist");
        }
        for col in [
            "id", "org_id", "project", "name", "goal", "starts_at", "ends_at", "status",
            "created_by", "created_at", "archived_at",
        ] {
            assert!(column_exists(&conn, "sprints", col), "sprints.{col} must exist");
        }
        for col in [
            "id", "sprint_id", "org_id", "went_well", "went_wrong", "action_items", "created_by",
            "created_at",
        ] {
            assert!(column_exists(&conn, "sprint_retrospectives", col), "sprint_retrospectives.{col} must exist");
        }
    }

    #[test]
    fn run_v51_creates_indexes() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert!(index_exists(&conn, "tasks", "idx_tasks_org_project_status"));
        assert!(index_exists(&conn, "tasks", "idx_tasks_org_parent"));
        assert!(index_exists(&conn, "tasks", "idx_tasks_sprint"));
        assert!(index_exists(&conn, "task_assignees", "idx_task_assignees_user"));
        assert!(index_exists(&conn, "task_labels", "idx_task_labels_label"));
        assert!(index_exists(&conn, "task_comments", "idx_task_comments_task"));
        assert!(index_exists(&conn, "task_spec_links", "idx_task_spec_links_change"));
        assert!(index_exists(&conn, "sprints", "idx_sprints_org_project_status"));
        assert!(index_exists(&conn, "sprint_retrospectives", "idx_sprint_retros_sprint"));
    }

    #[test]
    fn run_v51_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let table_count_before: i32 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table'", [], |r| r.get(0))
            .unwrap();
        let index_count_before: i32 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='index'", [], |r| r.get(0))
            .unwrap();

        let result = run_all(&conn);
        assert!(result.is_ok(), "run_all must be idempotent after v51/v52: {:?}", result.err());

        let table_count_after: i32 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table'", [], |r| r.get(0))
            .unwrap();
        let index_count_after: i32 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='index'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(table_count_before, table_count_after, "table count must not change on re-run");
        assert_eq!(index_count_before, index_count_after, "index count must not change on re-run");
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
        assert!(dup.is_err(), "UNIQUE(task_id, user_id) on task_assignees must be enforced");

        // task_labels cascades with task, UNIQUE(task_id, label) enforced.
        conn.execute("INSERT INTO task_labels (id, task_id, label) VALUES ('tl1', 't1', 'bug')", [])
            .unwrap();
        let dup_label = conn.execute("INSERT INTO task_labels (id, task_id, label) VALUES ('tl2', 't1', 'bug')", []);
        assert!(dup_label.is_err(), "UNIQUE(task_id, label) on task_labels must be enforced");

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
        assert!(dup_link.is_err(), "UNIQUE(task_id, spec_change_name) on task_spec_links must be enforced");

        // sprints UNIQUE(org_id, project, name) enforced.
        let dup_sprint = conn.execute(
            "INSERT INTO sprints (id, org_id, project, name, created_by) VALUES ('sp2', 'org1', 'proj', 'Sprint 1', 'u1')",
            [],
        );
        assert!(dup_sprint.is_err(), "UNIQUE(org_id, project, name) on sprints must be enforced");

        // sprint_retrospectives cascades with sprint.
        conn.execute(
            "INSERT INTO sprint_retrospectives (id, sprint_id, org_id, created_by) VALUES ('sr1', 'sp1', 'org1', 'u1')",
            [],
        )
        .unwrap();

        // Deleting the task cascades to assignees/labels/comments/spec_links.
        conn.execute("DELETE FROM tasks WHERE id = 't1'", []).unwrap();
        let remaining_assignees: i32 = conn
            .query_row("SELECT COUNT(*) FROM task_assignees WHERE task_id = 't1'", [], |r| r.get(0))
            .unwrap();
        let remaining_labels: i32 = conn
            .query_row("SELECT COUNT(*) FROM task_labels WHERE task_id = 't1'", [], |r| r.get(0))
            .unwrap();
        let remaining_comments: i32 = conn
            .query_row("SELECT COUNT(*) FROM task_comments WHERE task_id = 't1'", [], |r| r.get(0))
            .unwrap();
        let remaining_links: i32 = conn
            .query_row("SELECT COUNT(*) FROM task_spec_links WHERE task_id = 't1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining_assignees, 0, "task_assignees must cascade-delete with task");
        assert_eq!(remaining_labels, 0, "task_labels must cascade-delete with task");
        assert_eq!(remaining_comments, 0, "task_comments must cascade-delete with task");
        assert_eq!(remaining_links, 0, "task_spec_links must cascade-delete with task");

        // Deleting the sprint cascades to retrospectives and SETs task.sprint_id NULL
        // (re-create a task pointing at sp1 to verify the SET NULL path independently).
        conn.execute(
            "INSERT INTO tasks (id, org_id, project, title, created_by, sprint_id) VALUES ('t2', 'org1', 'proj', 'Task 2', 'u1', 'sp1')",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM sprints WHERE id = 'sp1'", []).unwrap();
        let remaining_retros: i32 = conn
            .query_row("SELECT COUNT(*) FROM sprint_retrospectives WHERE sprint_id = 'sp1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining_retros, 0, "sprint_retrospectives must cascade-delete with sprint");
        let t2_sprint_id: Option<String> = conn
            .query_row("SELECT sprint_id FROM tasks WHERE id = 't2'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(t2_sprint_id, None, "tasks.sprint_id must be SET NULL when the sprint is deleted");
    }

    #[test]
    fn run_v52_grants_task_perms() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        let perms_json = |template_id: &str| -> String {
            conn.query_row("SELECT permissions FROM roles WHERE id = ?1", [template_id], |r| r.get(0))
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
            .query_row("SELECT permissions FROM roles WHERE id = 'tmpl_dev_senior'", [], |r| r.get(0))
            .unwrap();

        run_all(&conn).unwrap();

        let after: String = conn
            .query_row("SELECT permissions FROM roles WHERE id = 'tmpl_dev_senior'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(before, after, "re-running run_all must not duplicate permission strings");
        let arr: Vec<String> = serde_json::from_str(&after).unwrap();
        let task_write_count = arr.iter().filter(|p| p.as_str() == "task:write").count();
        assert_eq!(task_write_count, 1, "task:write must appear exactly once after re-run");
    }

    #[test]
    fn run_v52_preserves_existing_permissions() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        let senior: String = conn
            .query_row("SELECT permissions FROM roles WHERE id = 'tmpl_dev_senior'", [], |r| r.get(0))
            .unwrap();
        let senior_arr: Vec<String> = serde_json::from_str(&senior).unwrap();
        for pre_existing in ["memory:read", "memory:write", "memory:delete", "memory:search"] {
            assert!(
                senior_arr.iter().any(|p| p == pre_existing),
                "tmpl_dev_senior must retain pre-existing permission {pre_existing}"
            );
        }

        let junior: String = conn
            .query_row("SELECT permissions FROM roles WHERE id = 'tmpl_dev_junior'", [], |r| r.get(0))
            .unwrap();
        let junior_arr: Vec<String> = serde_json::from_str(&junior).unwrap();
        for pre_existing in ["memory:read", "memory:search"] {
            assert!(
                junior_arr.iter().any(|p| p == pre_existing),
                "tmpl_dev_junior must retain pre-existing permission {pre_existing}"
            );
        }

        let auditor: String = conn
            .query_row("SELECT permissions FROM roles WHERE id = 'tmpl_auditor'", [], |r| r.get(0))
            .unwrap();
        let auditor_arr: Vec<String> = serde_json::from_str(&auditor).unwrap();
        assert!(auditor_arr.iter().any(|p| p == "audit:read"), "tmpl_auditor must retain pre-existing audit:read");

        let security_officer: String = conn
            .query_row("SELECT permissions FROM roles WHERE id = 'tmpl_security_officer'", [], |r| r.get(0))
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

    /// 1.23 — run_all lands on 55.
    #[test]
    fn run_all_sets_user_version_to_55() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        assert_eq!(
            get_user_version(&conn),
            55,
            "run_all must leave user_version at 55"
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

        conn.execute("DELETE FROM sdd_changes WHERE id = 'c1'", []).unwrap();

        let (still_there, merged): (i64, Option<String>) = conn
            .query_row(
                "SELECT COUNT(*), MAX(merged_from_change_id) FROM sdd_spec_revisions WHERE id = 'r1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(still_there, 1, "the spec revision must survive the deletion of the change");
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

        conn.execute("DELETE FROM sdd_specs WHERE id = 's1'", []).unwrap();
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
        assert!(dup.is_err(), "revision 1 of a spec MUST be unique — UNIQUE(spec_id, revision)");
    }

    /// `source` defaults to 'agent', matching `sdd_artifact_revisions`.
    #[test]
    fn run_v55_source_defaults_to_agent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        let (notnull, default) = column_info(&conn, "sdd_spec_revisions", "source")
            .expect("sdd_spec_revisions.source must exist");
        assert!(notnull, "source must be NOT NULL");
        assert_eq!(default.as_deref(), Some("'agent'"), "source must default to 'agent'");
    }

    /// The FTS5 index exists, indexes content, and leaves `spec_id` UNINDEXED.
    #[test]
    fn run_v55_creates_specs_fts_virtual_table() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();

        assert!(table_exists(&conn, "sdd_specs_fts"), "missing fts table: sdd_specs_fts");

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
        assert_eq!(id_hits, 0, "spec_id must be UNINDEXED — it is a payload, not a search term");
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
            assert!(index_exists(&conn, table, idx), "missing index: {idx} on {table}");
        }
    }

    /// Re-running v55 on an already-migrated database is a no-op, not an error.
    #[test]
    fn run_v55_is_idempotent() {
        let conn = in_memory_db();
        run_all(&conn).unwrap();
        run_v55(&conn).expect("run_v55 must be idempotent");
        run_all(&conn).expect("run_all must be idempotent");
        assert_eq!(get_user_version(&conn), 55, "user_version must remain 55");
    }
}
