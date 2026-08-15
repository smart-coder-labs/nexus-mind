use std::fs;
use std::path::Path;

use nexusmind::auth::api_keys;
use nexusmind::db::{connection, migrations, queries};
use nexusmind::models::types::StoreMemoryRequest;
use rusqlite::Connection;
use uuid::Uuid;

// ── Demo data ─────────────────────────────────────────────────────────────────

struct OrgSpec {
    name: &'static str,
    slug: &'static str,
}

const ORGS: &[OrgSpec] = &[
    OrgSpec { name: "Acme Corp", slug: "acme" },
    OrgSpec { name: "TechStartup", slug: "techstartup" },
    OrgSpec { name: "DevShop", slug: "devshop" },
];

struct UserSpec {
    email_prefix: &'static str,
    name_suffix: &'static str,
    role: &'static str,
}

const USERS: &[UserSpec] = &[
    UserSpec { email_prefix: "admin",  name_suffix: "Admin User",      role: "admin"  },
    UserSpec { email_prefix: "sarah",  name_suffix: "Sarah Chen",      role: "member" },
    UserSpec { email_prefix: "marcus", name_suffix: "Marcus Johnson",  role: "member" },
    UserSpec { email_prefix: "ana",    name_suffix: "Ana García",      role: "member" },
    UserSpec { email_prefix: "david",  name_suffix: "David Park",      role: "viewer" },
];

type MemorySpec = (&'static str, &'static str, &'static str, &'static [&'static str]);

const MEMORIES: &[MemorySpec] = &[
    ("claude-code",     "nexusmind", "Use snake_case for all API endpoints — team convention",                            &["convention", "api"]),
    ("claude-code",     "nexusmind", "Database connection pool set to 20 — was timing out at 10",                        &["performance", "db"]),
    ("cursor",          "nexusmind", "Migrated auth from JWT to OAuth2 — see PR #234",                                   &["auth", "migration"]),
    ("claude-code",     "payments",  "Stripe API v3 only — v2 deprecated as of Jan 2026",                               &["payments", "stripe"]),
    ("cursor",          "payments",  "Payment webhook secret stored in env var STRIPE_WEBHOOK_SECRET",                   &["payments", "security"]),
    ("github-copilot",  "nexusmind", "All DB queries must include org_id filter — multi-tenant rule",                    &["convention", "db", "multi-tenant"]),
    ("claude-code",     "nexusmind", "Use anyhow::Result for all internal functions, ApiError for HTTP",                 &["convention", "error-handling"]),
    ("cursor",          "infra",     "Docker compose uses named volume nexusmind_data — do not use bind mount in prod",  &["infra", "docker"]),
    ("claude-code",     "infra",     "SQLite WAL mode enabled — supports concurrent reads",                              &["db", "performance"]),
    ("github-copilot",  "nexusmind", "FTS5 triggers keep memories_fts in sync — never update memories_fts directly",    &["db", "fts"]),
    ("claude-code",     "payments",  "Refund flow requires manual approval above $500",                                  &["payments", "policy"]),
    ("cursor",          "nexusmind", "API keys are hashed with SHA-256 before storage — raw key shown once",             &["security", "auth"]),
    ("claude-code",     "nexusmind", "Audit log is append-only — no deletes allowed",                                   &["convention", "audit"]),
    ("github-copilot",  "infra",     "Health endpoint at /v1/health — used by docker compose healthcheck",              &["infra", "ops"]),
    ("claude-code",     "payments",  "Currency always stored in cents (integer) — never floats",                         &["convention", "payments"]),
    ("cursor",          "nexusmind", "Bootstrap endpoint disabled after first org is created — returns 409",             &["api", "security"]),
    ("claude-code",     "nexusmind", "Role hierarchy: admin > member > viewer — viewers cannot store memories",          &["auth", "roles"]),
    ("github-copilot",  "nexusmind", "org_id never comes from request body — always derived from API key",               &["security", "convention"]),
    ("claude-code",     "infra",     "Cargo build uses bundled SQLite feature — no system lib dependency",               &["infra", "build"]),
    ("cursor",          "payments",  "Payment events published to audit log with metadata including amount and currency", &["payments", "audit"]),
];

const AUDIT_ACTIONS: &[(&str, &str)] = &[
    ("store",  "memory"),
    ("search", "memory"),
    ("invite", "user"),
    ("store",  "memory"),
    ("search", "memory"),
    ("store",  "memory"),
    ("search", "memory"),
    ("rotate", "api_key"),
    ("store",  "memory"),
    ("search", "memory"),
];

// ── Seed logic ────────────────────────────────────────────────────────────────

fn seed_org(
    conn: &Connection,
    org_spec: &OrgSpec,
) -> anyhow::Result<()> {
    let slug = org_spec.slug;

    // create_org has no "only one org" guard — safe to call for all 3 demo orgs.
    // We replace the generated key with a deterministic demo key afterward.
    let (org, admin_user, _random_key) = queries::create_org(
        conn,
        org_spec.name,
        slug,
        &format!("admin@{slug}.com"),
        "Admin User",
    )?;

    // Replace the admin's random key with a deterministic demo key.
    let admin_raw = format!("nm_demo_{slug}_admin");
    let admin_hash = api_keys::hash_key(&admin_raw);
    conn.execute("DELETE FROM api_keys WHERE user_id = ?1", [&admin_user.id])?;
    conn.execute(
        "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
         VALUES (?1, ?2, ?3, ?4, 'demo-admin', datetime('now'))",
        rusqlite::params![Uuid::new_v4().to_string(), admin_user.id, org.id, admin_hash],
    )?;

    // Invite non-admin users and replace their keys with deterministic ones.
    let mut non_viewer_user_ids: Vec<String> = vec![admin_user.id.clone()];

    for spec in USERS.iter().skip(1) {
        let email = format!("{}@{slug}.com", spec.email_prefix);
        let (user, _random_key) = queries::invite_user(conn, &org.id, &email, spec.name_suffix, spec.role)?;

        let raw = format!("nm_demo_{slug}_{}", spec.email_prefix);
        let hash = api_keys::hash_key(&raw);

        conn.execute("DELETE FROM api_keys WHERE user_id = ?1", [&user.id])?;
        conn.execute(
            "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
             VALUES (?1, ?2, ?3, ?4, 'demo', datetime('now'))",
            rusqlite::params![Uuid::new_v4().to_string(), user.id, org.id, hash],
        )?;

        if spec.role != "viewer" {
            non_viewer_user_ids.push(user.id.clone());
        }
    }

    // Implicit project creation is disabled at the store layer, so pre-create every
    // project referenced by demo memories and keep its id for membership enrolment.
    // Storing a memory no longer auto-enrols its author (that write-time behaviour was
    // removed in favour of explicit membership — see queries::create_org), so without
    // the enrolment below the demo data is invisible under the project-scoped
    // visibility model: every list/search returns "Access denied" or 0 results.
    let mut project_ids = std::collections::HashMap::new();
    for &(_, project, _, _) in MEMORIES.iter() {
        if !project_ids.contains_key(project) {
            let id = queries::get_or_create_project(conn, &org.id, project)?;
            project_ids.insert(project, id);
        }
    }

    // Store memories round-robin across non-viewer users, enrolling each author as a
    // member of the project they write to so they can see their own demo memories.
    for (i, &(tool, project, content, tags)) in MEMORIES.iter().enumerate() {
        let user_id = &non_viewer_user_ids[i % non_viewer_user_ids.len()];
        let tag_strings: Vec<String> = tags.iter().map(|s| s.to_string()).collect();
        queries::upsert_memory(conn, &org.id, user_id, &StoreMemoryRequest {
            project: Some(project.to_string()),
            tool: tool.to_string(),
            content: content.to_string(),
            tags: Some(tag_strings.clone()),
            title: None,
            memory_type: None,
            scope: None,
            topic_key: None,
            session_id: None,
        })?;
        queries::upsert_project_member(conn, &project_ids[project], user_id, "member")?;
    }

    // Log audit events across users.
    for (i, &(action, resource_type)) in AUDIT_ACTIONS.iter().enumerate() {
        let user_id = &non_viewer_user_ids[i % non_viewer_user_ids.len()];
        queries::log_audit(conn, &org.id, user_id, action, resource_type, None, serde_json::json!({}))?;
    }

    Ok(())
}

fn print_summary(slug: &str, org_name: &str, memory_count: usize) {
    println!("\nOrg: {org_name} ({slug})");
    for spec in USERS {
        let raw = format!("nm_demo_{slug}_{}", spec.email_prefix);
        let email = format!("{}@{slug}.com", spec.email_prefix);
        println!("  {email:<28} [{:<6}]  key: {raw}", spec.role);
    }
    println!("  Memories: {memory_count}");
}

// ── Main ──────────────────────────────────────────────────────────────────────

/// Bail out if another connection holds the database, so we never delete a file
/// a live backend is using. Opens the existing DB and tries to take the write
/// lock via `BEGIN EXCLUSIVE`; if it cannot be acquired within a short timeout,
/// someone is writing it and we refuse to proceed with a clear message.
fn ensure_not_in_use(db_path: &str) -> anyhow::Result<()> {
    let conn = Connection::open(db_path)?;
    conn.busy_timeout(std::time::Duration::from_millis(500))?;
    conn.execute_batch("BEGIN EXCLUSIVE; COMMIT;").map_err(|e| {
        anyhow::anyhow!(
            "Database at {db_path} is in use by another process (could not acquire \
             an exclusive lock: {e}).\nStop the backend before seeding, e.g.:\n  \
             docker compose stop backend"
        )
    })?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let db_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "./data/nexusmind.db".to_string());

    // Refuse to seed a database another process holds open. Seeding deletes and
    // recreates the file; doing that under a live backend leaves it writing to a
    // now-unlinked inode (silent data loss) and produces SQLITE_IOERR_SHORT_READ
    // (522) from the stale -wal/-shm sidecars. An exclusive lock is our portable
    // "is anyone writing this?" probe — it catches a backend that is up and
    // serving, which is the case that actually bites. A fully idle open handle
    // can still slip past it, so we also drop the WAL sidecars below.
    if Path::new(&db_path).exists() {
        ensure_not_in_use(&db_path)?;
    }

    // Always start fresh — remove the main file AND its WAL sidecars. Removing
    // only the .db (the old behaviour) left -wal/-shm orphaned, so the next open
    // recovered a WAL against an empty file and failed with error 522.
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let path = format!("{db_path}{suffix}");
        if Path::new(&path).exists() {
            fs::remove_file(&path)?;
        }
    }
    if let Some(parent) = Path::new(&db_path).parent() {
        fs::create_dir_all(parent)?;
    }

    let conn = connection::connect(&db_path)?;
    migrations::run(&conn)?;

    println!("=== NexusMind Demo Data ===");

    for org_spec in ORGS {
        seed_org(&conn, org_spec)?;
        print_summary(org_spec.slug, org_spec.name, MEMORIES.len());
    }

    println!("\nDemo ready! Open http://localhost:8080/v1/health");

    Ok(())
}
