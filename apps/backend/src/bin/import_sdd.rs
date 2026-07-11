//! One-shot, idempotent importer: backfills `openspec/changes/**` and the legacy
//! `sdd/*` memories into the SDD artifact store (design.md §5).
//!
//! Usage:
//!   cargo run --bin import-sdd -- --db ./data/nexusmind.db --org-id <id> \
//!       --project nexus-mind --root . [--dry-run]
//!
//! Safe to re-run: every write goes through `queries::upsert_sdd_artifact`, which
//! is idempotent by content hash, so a second run creates ZERO revisions.
//!
//! Two sources, imported in this order:
//!   1. the legacy `sdd/{change}/{artifact}` memories (older) — become revision 1,
//!   2. the filesystem (newer, reviewable) — becomes revision 2 and therefore wins.
//!
//! The legacy memories are TAGGED `sdd-migrated` and left in place: whether to
//! retire them is an explicit user decision, made after they can see the imported
//! artifacts in the admin. The test `importer_never_removes_a_memory` scans this
//! file to keep it that way — which is why neither that test nor this comment may
//! spell the statement it forbids: the scan reads its own source and would match
//! itself.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Result};
use clap::Parser;
use nexusmind::db::{connection::connect, migrations, queries};
use nexusmind::models::types::{
    PatchChangeRequest, SaveArtifactRequest, SddArtifactKind, SddPhase,
};
use rusqlite::{Connection, OptionalExtension};

/// Provenance stamped on every revision this binary writes.
const SOURCE: &str = "import";

/// Tag applied to every legacy memory carried into the artifact store.
const MIGRATED_TAG: &str = "sdd-migrated";

#[derive(Parser)]
#[command(
    about = "Import openspec/changes/** and legacy sdd/* memories into the SDD artifact store"
)]
struct Args {
    #[arg(long = "db", alias = "db-path", env = "DB_PATH", default_value = "./data/nexusmind.db")]
    db: String,

    #[arg(long, help = "Org to import into. Omit to resolve the single org.")]
    org_id: Option<String>,

    #[arg(long, default_value = "nexus-mind", help = "Project name for the imported changes")]
    project: String,

    #[arg(long, default_value = ".", help = "Repo root — the folder containing openspec/")]
    root: String,

    #[arg(long, help = "Report what would be imported without writing anything")]
    dry_run: bool,
}

/// What the importer did — or, under `--dry-run`, would do.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImportStats {
    pub changes_created: usize,
    pub artifacts_created: usize,
    pub revisions_created: usize,
    pub memories_tagged: usize,
    pub skipped: usize,
}

impl ImportStats {
    pub fn merge(&mut self, other: &ImportStats) {
        self.changes_created += other.changes_created;
        self.artifacts_created += other.artifacts_created;
        self.revisions_created += other.revisions_created;
        self.memories_tagged += other.memories_tagged;
        self.skipped += other.skipped;
    }
}

/// One file on disk that maps onto an artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredArtifact {
    pub kind: SddArtifactKind,
    /// `""` for every kind but `spec`.
    pub capability: String,
    /// Absolute path, for reading the content.
    pub abs_path: PathBuf,
    /// Path relative to the change folder, e.g. `specs/foo/spec.md`.
    pub rel_path: String,
}

// ── pure mapping helpers ────────────────────────────────────────────────────

/// Maps a path relative to a change folder onto the artifact it represents.
/// `None` for anything the convention does not name (README.md, notes, …).
///
/// The `spec` kind arrives two ways: `specs/{capability}/spec.md` (the convention)
/// and a bare `spec.md` at the change root (what the older changes in this repo
/// actually wrote). The second yields an empty capability, which is exactly what
/// the store normalizes non-spec kinds to, and is still unique per change.
pub fn kind_for_path(rel: &Path) -> Option<(SddArtifactKind, String)> {
    let parts: Vec<&str> = rel.iter().filter_map(|p| p.to_str()).collect();

    // specs/{capability}/spec.md
    if let [dir, capability, "spec.md"] = parts.as_slice() {
        if *dir == "specs" && !capability.is_empty() {
            return Some((SddArtifactKind::Spec, (*capability).to_string()));
        }
        return None;
    }

    let [file] = parts.as_slice() else {
        return None;
    };

    let kind = match *file {
        "proposal.md" => SddArtifactKind::Proposal,
        "design.md" => SddArtifactKind::Design,
        "tasks.md" => SddArtifactKind::Tasks,
        // `exploration.md` is the convention; `explore.md` is what is on disk.
        "exploration.md" | "explore.md" => SddArtifactKind::Exploration,
        "apply-progress.md" => SddArtifactKind::ApplyProgress,
        "verify-report.md" => SddArtifactKind::VerifyReport,
        "archive-report.md" => SddArtifactKind::ArchiveReport,
        "state.yaml" => SddArtifactKind::State,
        "spec.md" => SddArtifactKind::Spec,
        _ => return None,
    };
    Some((kind, String::new()))
}

/// The phase a given artifact marks as reached. `state.yaml` is bookkeeping, not
/// a phase output, so it contributes nothing.
pub fn phase_for_kind(kind: SddArtifactKind) -> Option<SddPhase> {
    Some(match kind {
        SddArtifactKind::Exploration => SddPhase::Explore,
        SddArtifactKind::Proposal => SddPhase::Propose,
        SddArtifactKind::Spec => SddPhase::Spec,
        SddArtifactKind::Design => SddPhase::Design,
        SddArtifactKind::Tasks => SddPhase::Tasks,
        SddArtifactKind::ApplyProgress => SddPhase::Apply,
        SddArtifactKind::VerifyReport => SddPhase::Verify,
        SddArtifactKind::ArchiveReport => SddPhase::Archive,
        SddArtifactKind::State => return None,
    })
}

/// A change's phase is the furthest phase present in its artifact inventory —
/// `SddPhase::rank` is the DAG order. An inventory that names no phase at all
/// (empty, or `state.yaml` only) falls back to the store's own default.
pub fn infer_phase(kinds: &[SddArtifactKind]) -> SddPhase {
    kinds
        .iter()
        .filter_map(|k| phase_for_kind(*k))
        .max_by_key(|p| p.rank())
        .unwrap_or(SddPhase::Propose)
}

/// `2026-05-01-old-change` → `old-change`. Anything else is returned unchanged.
/// The prefix is `YYYY-MM-DD-`: a full date, not a bare year.
pub fn strip_date_prefix(folder: &str) -> &str {
    let bytes = folder.as_bytes();
    if bytes.len() > 11
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
        && bytes[10] == b'-'
    {
        return &folder[11..];
    }
    folder
}

// ── source A: the filesystem ────────────────────────────────────────────────

/// Every artifact file inside one change folder, top level plus `specs/*/spec.md`.
/// Files the convention does not name come back with `kind == None` — the caller
/// counts them as skipped rather than guessing at them.
pub fn scan_change_dir(dir: &Path) -> Result<(Vec<DiscoveredArtifact>, usize)> {
    let mut found = Vec::new();
    let mut skipped = 0usize;

    for rel in list_candidate_files(dir)? {
        let abs_path = dir.join(&rel);
        match kind_for_path(Path::new(&rel)) {
            Some((kind, capability)) => {
                found.push(DiscoveredArtifact { kind, capability, abs_path, rel_path: rel })
            }
            None => skipped += 1,
        }
    }

    found.sort_by(|a, b| {
        (a.kind.to_string(), a.capability.clone()).cmp(&(b.kind.to_string(), b.capability.clone()))
    });
    Ok((found, skipped))
}

/// Paths relative to `dir`: every top-level file, plus every file exactly two
/// levels down inside `specs/`. Nothing deeper — the convention has no such file.
fn list_candidate_files(dir: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let file_type = entry.file_type()?;

        if file_type.is_file() {
            out.push(name);
            continue;
        }
        if !file_type.is_dir() || name != "specs" {
            continue;
        }
        for cap_entry in std::fs::read_dir(entry.path())? {
            let cap_entry = cap_entry?;
            if !cap_entry.file_type()?.is_dir() {
                continue;
            }
            let capability = cap_entry.file_name().to_string_lossy().to_string();
            for spec_entry in std::fs::read_dir(cap_entry.path())? {
                let spec_entry = spec_entry?;
                if spec_entry.file_type()?.is_file() {
                    let file = spec_entry.file_name().to_string_lossy().to_string();
                    out.push(format!("specs/{capability}/{file}"));
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

/// `git rev-parse HEAD`, or `None` when git is absent or `root` is not a repo.
/// Never panics and never fails the import — provenance is nice to have, not a
/// precondition, and the importer must run on a tarball too.
pub fn git_head(root: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let sha = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(sha)
    } else {
        None
    }
}

/// One change folder — either `openspec/changes/{name}` or, when `archived`,
/// `openspec/changes/archive/{YYYY-MM-DD-name}`.
struct ChangeDir {
    /// The `sdd_changes.name`: the folder name, date prefix stripped for archives.
    name: String,
    dir: PathBuf,
    /// Path of the folder relative to the repo root — the prefix of every git_path.
    rel_dir: String,
    archived: bool,
}

fn discover_change_dirs(root: &Path) -> Result<Vec<ChangeDir>> {
    let changes_root = root.join("openspec").join("changes");
    if !changes_root.is_dir() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for entry in std::fs::read_dir(&changes_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();

        // `archive/` is a container of change folders, never a change itself.
        if name == "archive" {
            for archived in std::fs::read_dir(entry.path())? {
                let archived = archived?;
                if !archived.file_type()?.is_dir() {
                    continue;
                }
                let folder = archived.file_name().to_string_lossy().to_string();
                out.push(ChangeDir {
                    name: strip_date_prefix(&folder).to_string(),
                    dir: archived.path(),
                    rel_dir: format!("openspec/changes/archive/{folder}"),
                    archived: true,
                });
            }
            continue;
        }

        out.push(ChangeDir {
            name: name.clone(),
            dir: entry.path(),
            rel_dir: format!("openspec/changes/{name}"),
            archived: false,
        });
    }
    out.sort_by(|a, b| a.rel_dir.cmp(&b.rel_dir));
    Ok(out)
}

/// Walks `{root}/openspec/changes/*/` and `{root}/openspec/changes/archive/*/`.
///
/// Active changes get their phase inferred from the artifact inventory; archive
/// folders get their date prefix stripped and are forced to
/// `status='archived' / phase='archive'`.
///
/// Every write goes through `queries::upsert_sdd_artifact`, so a second run is a
/// no-op: the importer deliberately owns no insert path of its own.
pub fn import_filesystem(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    project: &str,
    root: &Path,
    dry_run: bool,
) -> Result<ImportStats> {
    let mut stats = ImportStats::default();
    let mut ledger = DryLedger::default();
    let git_commit = git_head(root);

    for change in discover_change_dirs(root)? {
        let (artifacts, skipped) = scan_change_dir(&change.dir)?;
        stats.skipped += skipped;
        if artifacts.is_empty() {
            continue;
        }

        count_change(conn, org_id, project, &change.name, &mut ledger, &mut stats)?;

        for artifact in &artifacts {
            let content = std::fs::read_to_string(&artifact.abs_path)?;
            let git_path = format!("{}/{}", change.rel_dir, artifact.rel_path);
            let req = SaveArtifactRequest {
                project: project.to_string(),
                change_name: change.name.clone(),
                kind: artifact.kind.to_string(),
                capability: Some(artifact.capability.clone()),
                content,
                path: Some(git_path),
                git_commit: git_commit.clone(),
                git_ref: None,
                source: Some(SOURCE.to_string()),
            };
            let outcome = save_artifact(conn, org_id, user_id, &req, dry_run, &mut ledger)?;
            stats.merge(&outcome);
        }

        if dry_run {
            continue;
        }

        // Phase/status are set AFTER the artifacts, because the inventory is what
        // decides the phase. Archives are forced — a folder under archive/ is
        // archived whatever its inventory says.
        let (status, phase) = if change.archived {
            ("archived".to_string(), SddPhase::Archive.to_string())
        } else {
            let kinds: Vec<SddArtifactKind> = artifacts.iter().map(|a| a.kind).collect();
            ("active".to_string(), infer_phase(&kinds).to_string())
        };
        let stored = queries::get_sdd_change_by_name(conn, org_id, project, &change.name)?
            .ok_or_else(|| anyhow!("change {} vanished mid-import", change.name))?;
        queries::patch_sdd_change(
            conn,
            org_id,
            &stored.id,
            &PatchChangeRequest {
                status: Some(status),
                phase: Some(phase),
                ..Default::default()
            },
        )?;

        // `status` and `archived_at` are two representations of the same fact, and
        // `list_sdd_changes` filters on `archived_at IS NULL` — not on `status`. Setting
        // only `status='archived'` would leave every imported archive folder showing up
        // in the admin's default list forever. Set both, or the two disagree.
        //
        // This does NOT withdraw the change: `sdd_change_exists` carries no `archived_at`
        // predicate, so an archived change stays a valid `link_task_spec` target (A8),
        // and `get_sdd_change` by id still returns its full artifact inventory.
        if change.archived {
            queries::archive_sdd_change(conn, org_id, &stored.id)?;
        }
    }

    Ok(stats)
}

/// `--dry-run` writes nothing, so the DB cannot tell it what it already "created"
/// earlier in the same pass. This remembers, so two memories of one artifact are
/// reported as one artifact and two revisions — the same as a real run.
#[derive(Default)]
struct DryLedger {
    changes: std::collections::HashSet<String>,
    artifacts: std::collections::HashSet<String>,
}

/// Counts a change as created the first time it is seen and does not yet exist.
fn count_change(
    conn: &Connection,
    org_id: &str,
    project: &str,
    name: &str,
    ledger: &mut DryLedger,
    stats: &mut ImportStats,
) -> Result<()> {
    if queries::get_sdd_change_by_name(conn, org_id, project, name)?.is_none()
        && ledger.changes.insert(name.to_string())
    {
        stats.changes_created += 1;
    }
    Ok(())
}

/// The single write path. `--dry-run` answers the same questions with reads only,
/// so the numbers it reports are the numbers a real run would produce.
fn save_artifact(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    req: &SaveArtifactRequest,
    dry_run: bool,
    ledger: &mut DryLedger,
) -> Result<ImportStats> {
    let mut stats = ImportStats::default();

    if dry_run {
        let capability = req.capability.as_deref().unwrap_or("");
        let key = format!("{}\u{1}{}\u{1}{}", req.change_name, req.kind, capability);
        match latest_hash(conn, org_id, &req.project, &req.change_name, &req.kind, capability)? {
            None if ledger.artifacts.insert(key) => {
                stats.artifacts_created += 1;
                stats.revisions_created += 1;
            }
            // Already counted as new earlier in this same dry run — this is a
            // further revision of it, not a second artifact.
            None => stats.revisions_created += 1,
            Some(hash) if hash != sha256_hex(&req.content) => stats.revisions_created += 1,
            Some(_) => stats.skipped += 1,
        }
        return Ok(stats);
    }

    // 5.18 — idempotency is not the importer's to implement. upsert_sdd_artifact
    // compares the content hash against the latest revision and returns
    // `created_revision = false` when they match, which is why the importer owns
    // no insert path of its own and a second run writes nothing.
    let (artifact, created_revision) =
        queries::upsert_sdd_artifact(conn, org_id, user_id, req, SOURCE)?;
    if created_revision {
        stats.revisions_created += 1;
        if artifact.latest_revision == 1 {
            stats.artifacts_created += 1;
        }
    } else {
        stats.skipped += 1;
    }
    Ok(stats)
}

/// Content hash of the latest revision of `(change, kind, capability)`, or `None`
/// when the artifact does not exist yet. Read-only — this is how `--dry-run`
/// predicts what `upsert_sdd_artifact` would decide.
fn latest_hash(
    conn: &Connection,
    org_id: &str,
    project: &str,
    change_name: &str,
    kind: &str,
    capability: &str,
) -> Result<Option<String>> {
    let hash = conn
        .query_row(
            "SELECT r.content_hash
             FROM sdd_artifact_revisions r
             JOIN sdd_artifacts a ON a.id = r.artifact_id
             JOIN sdd_changes   c ON c.id = a.change_id
             WHERE c.org_id = ?1 AND c.project = ?2 AND c.name = ?3
               AND a.kind = ?4 AND a.capability = ?5
             ORDER BY r.revision DESC LIMIT 1",
            rusqlite::params![org_id, project, change_name, kind, capability],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
    Ok(hash)
}

fn sha256_hex(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ── source B: the legacy sdd/* memories ─────────────────────────────────────

/// Parses a legacy topic key. `sdd/{change}/{artifact}` → `(change, kind)`.
/// `None` for a key of the wrong shape, or one naming a kind we do not know:
/// such a memory is left alone, not guessed at.
pub fn parse_sdd_topic_key(topic_key: &str) -> Option<(String, SddArtifactKind)> {
    let mut parts = topic_key.split('/');
    if parts.next()? != "sdd" {
        return None;
    }
    let change = parts.next()?;
    let token = parts.next()?;
    if parts.next().is_some() || change.is_empty() {
        return None;
    }

    // The skills wrote the phase's own name; the enum spells the artifact's.
    let kind = match token {
        "explore" => SddArtifactKind::Exploration,
        other => SddArtifactKind::from_str(other).ok()?,
    };
    Some((change.to_string(), kind))
}

/// Carries every `sdd/{change}/{artifact}` memory into the artifact store,
/// oldest first, and tags each one `sdd-migrated`.
///
/// Runs BEFORE `import_filesystem` on purpose (5.16): the memory is the older
/// record, so it lands as revision 1 and the file — newer, reviewable — lands on
/// top of it as revision 2 and wins the read.
///
/// The memories themselves are left in place, tagged. Retiring them is a
/// separate, explicit decision for the user, taken once they can see the
/// imported artifacts in the admin.
///
/// `created_by` on the resulting revision is the memory's own author, not the
/// operator running the import — the provenance the memory carries is better
/// than the one the CLI could supply, which is why this fn takes no user id.
pub fn import_legacy_memories(
    conn: &Connection,
    org_id: &str,
    project: &str,
    dry_run: bool,
) -> Result<ImportStats> {
    let mut stats = ImportStats::default();
    let mut ledger = DryLedger::default();

    // ORDER BY created_at: the replay order IS the revision order (design §5).
    let mut stmt = conn.prepare(
        "SELECT id, user_id, topic_key, content, COALESCE(tags, '[]')
         FROM memories
         WHERE org_id = ?1 AND topic_key LIKE 'sdd/%'
         ORDER BY created_at ASC, rowid ASC",
    )?;
    let rows: Vec<(String, String, String, String, String)> = stmt
        .query_map([org_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<rusqlite::Result<_>>()?;

    for (memory_id, author, topic_key, content, tags) in rows {
        let Some((change_name, kind)) = parse_sdd_topic_key(&topic_key) else {
            tracing::warn!(%memory_id, %topic_key, "skipping memory: not an sdd artifact key");
            stats.skipped += 1;
            continue;
        };

        count_change(conn, org_id, project, &change_name, &mut ledger, &mut stats)?;

        let req = SaveArtifactRequest {
            project: project.to_string(),
            change_name,
            kind: kind.to_string(),
            capability: None,
            content,
            path: None,
            git_commit: None,
            git_ref: None,
            source: Some(SOURCE.to_string()),
        };
        let outcome = save_artifact(conn, org_id, &author, &req, dry_run, &mut ledger)?;
        stats.merge(&outcome);

        if tag_memory(conn, &memory_id, &tags, dry_run)? {
            stats.memories_tagged += 1;
        }
    }

    Ok(stats)
}

/// Appends `sdd-migrated` to a memory's tags, idempotently. Returns whether the
/// tag was (or, under `--dry-run`, would be) added.
///
/// This is the ONLY statement the importer runs against `memories`, and it is an
/// append: the memory keeps its content, its tags, and its existence.
fn tag_memory(conn: &Connection, memory_id: &str, tags_json: &str, dry_run: bool) -> Result<bool> {
    let mut tags: Vec<String> = serde_json::from_str(tags_json).unwrap_or_default();
    if tags.iter().any(|t| t == MIGRATED_TAG) {
        return Ok(false);
    }
    if dry_run {
        return Ok(true);
    }
    tags.push(MIGRATED_TAG.to_string());
    conn.execute(
        "UPDATE memories SET tags = ?1 WHERE id = ?2",
        rusqlite::params![serde_json::to_string(&tags)?, memory_id],
    )?;
    Ok(true)
}

// ── wiring ──────────────────────────────────────────────────────────────────

/// The org to import into: the one named, or the only one there is. Two orgs and
/// no `--org-id` is an error, never a guess — the artifacts would land in the
/// wrong tenant and every read is org-scoped.
fn resolve_org(conn: &Connection, requested: Option<&str>) -> Result<String> {
    if let Some(org_id) = requested {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM organizations WHERE id = ?1)",
            [org_id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(anyhow!("no such org: {org_id}"));
        }
        return Ok(org_id.to_string());
    }

    let ids: Vec<String> = conn
        .prepare("SELECT id FROM organizations ORDER BY created_at LIMIT 2")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    match ids.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err(anyhow!("no organizations in this database — nothing to import into")),
        _ => Err(anyhow!("more than one organization — pass --org-id to choose")),
    }
}

/// The user the filesystem revisions are attributed to. `created_by` is an FK, so
/// it must be a real user; an admin of the org is the closest thing to "the
/// operator who ran the import".
fn resolve_user(conn: &Connection, org_id: &str) -> Result<String> {
    conn.query_row(
        "SELECT id FROM users WHERE org_id = ?1
         ORDER BY CASE WHEN role = 'admin' THEN 0 ELSE 1 END, created_at
         LIMIT 1",
        [org_id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| anyhow!("org {org_id} has no users — revisions need a created_by"))
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let root = PathBuf::from(&args.root);
    eprintln!("→ Opening DB at {}", args.db);
    let conn = connect(&args.db)?;
    migrations::run_all(&conn)?;

    let org_id = resolve_org(&conn, args.org_id.as_deref())?;
    let user_id = resolve_user(&conn, &org_id)?;
    eprintln!(
        "→ Importing into org {org_id} as user {user_id}, project {}, root {}",
        args.project,
        root.display()
    );
    if args.dry_run {
        eprintln!("→ DRY RUN — nothing will be written.");
    }

    // Memories first, so the filesystem (newer, reviewable) lands on top of them
    // as the latest revision (5.16 / design §5).
    let mut stats = import_legacy_memories(&conn, &org_id, &args.project, args.dry_run)?;
    stats.merge(&import_filesystem(
        &conn,
        &org_id,
        &user_id,
        &args.project,
        &root,
        args.dry_run,
    )?);

    let verb = if args.dry_run { "would import" } else { "imported" };
    eprintln!(
        "✓ {verb}: {} changes, {} artifacts, {} revisions, {} memories tagged {MIGRATED_TAG}, {} skipped.",
        stats.changes_created,
        stats.artifacts_created,
        stats.revisions_created,
        stats.memories_tagged,
        stats.skipped
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexusmind::db::{connection::connect, migrations, queries};
    use std::fs;

    // ── fixtures ────────────────────────────────────────────────────────────

    /// In-memory DB with one org and one user — the importer needs a real
    /// `created_by`, since `sdd_changes.created_by` is an FK onto `users`.
    fn setup() -> (Connection, String, String) {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status)
             VALUES ('u1', 'org1', 'admin@acme.com', 'Admin', 'admin', 'active')",
            [],
        )
        .unwrap();
        (conn, "org1".to_string(), "u1".to_string())
    }

    /// A throwaway repo root. Not a git repo — `git_head` must tolerate that.
    fn temp_root(label: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("nm-import-sdd-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("openspec/changes")).unwrap();
        root
    }

    fn write_file(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn artifact_content(
        conn: &Connection,
        org: &str,
        project: &str,
        change: &str,
        kind: &str,
        capability: &str,
    ) -> Option<String> {
        let change = queries::get_sdd_change_by_name(conn, org, project, change).unwrap()?;
        let artifact = change
            .artifacts
            .iter()
            .find(|a| a.kind == kind && a.capability == capability)?;
        conn.query_row(
            "SELECT content FROM sdd_artifact_revisions WHERE artifact_id = ?1
             ORDER BY revision DESC LIMIT 1",
            [&artifact.id],
            |r| r.get(0),
        )
        .ok()
    }

    fn revision_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM sdd_artifact_revisions", [], |r| r.get(0))
            .unwrap()
    }

    // ── 5.3 kind_for_path ───────────────────────────────────────────────────

    #[test]
    fn kind_for_path_maps_every_artifact_filename() {
        let cases: [(&str, SddArtifactKind); 8] = [
            ("proposal.md", SddArtifactKind::Proposal),
            ("design.md", SddArtifactKind::Design),
            ("tasks.md", SddArtifactKind::Tasks),
            ("exploration.md", SddArtifactKind::Exploration),
            ("apply-progress.md", SddArtifactKind::ApplyProgress),
            ("verify-report.md", SddArtifactKind::VerifyReport),
            ("archive-report.md", SddArtifactKind::ArchiveReport),
            ("state.yaml", SddArtifactKind::State),
        ];
        for (file, expected) in cases {
            assert_eq!(
                kind_for_path(Path::new(file)),
                Some((expected, String::new())),
                "{file} must map to {expected} with an empty capability"
            );
        }

        assert_eq!(
            kind_for_path(Path::new("specs/sdd-artifact-store/spec.md")),
            Some((SddArtifactKind::Spec, "sdd-artifact-store".to_string())),
            "specs/{{capability}}/spec.md carries the capability"
        );

        assert_eq!(
            kind_for_path(Path::new("README.md")),
            None,
            "a file the convention does not name is not an artifact"
        );
    }

    #[test]
    fn kind_for_path_accepts_the_explore_md_spelling_used_on_disk() {
        // The convention names `exploration.md`; every change folder in this repo
        // actually writes `explore.md`. Both must import.
        assert_eq!(
            kind_for_path(Path::new("explore.md")),
            Some((SddArtifactKind::Exploration, String::new()))
        );
    }

    #[test]
    fn kind_for_path_maps_a_top_level_spec_md_to_a_capability_less_spec() {
        // The older changes (backend-completeness, policy-engine, …) put the spec at
        // the change root instead of under specs/{capability}/. Ignoring them would
        // drop six real spec documents on the floor.
        assert_eq!(
            kind_for_path(Path::new("spec.md")),
            Some((SddArtifactKind::Spec, String::new()))
        );
    }

    // ── 5.9 infer_phase ─────────────────────────────────────────────────────

    #[test]
    fn infer_phase_picks_the_furthest_kind_present() {
        use SddArtifactKind::*;
        assert_eq!(infer_phase(&[Proposal, Design]), SddPhase::Design);
        assert_eq!(infer_phase(&[Proposal, Spec, Design, Tasks]), SddPhase::Tasks);
        assert_eq!(infer_phase(&[Proposal, Design, Tasks, VerifyReport]), SddPhase::Verify);
        assert_eq!(infer_phase(&[Proposal]), SddPhase::Propose);
        assert_eq!(infer_phase(&[Exploration]), SddPhase::Explore);
        assert_eq!(
            infer_phase(&[ApplyProgress, Proposal]),
            SddPhase::Apply,
            "the order of the inventory must not matter — rank does"
        );
    }

    #[test]
    fn infer_phase_ignores_state_yaml_and_defaults_to_propose() {
        assert_eq!(
            infer_phase(&[SddArtifactKind::State]),
            SddPhase::Propose,
            "state.yaml is bookkeeping, not a phase output"
        );
        assert_eq!(infer_phase(&[]), SddPhase::Propose);
    }

    // ── 5.7 archive folder names ────────────────────────────────────────────

    #[test]
    fn strip_date_prefix_removes_only_a_leading_iso_date() {
        assert_eq!(strip_date_prefix("2026-05-01-old-change"), "old-change");
        assert_eq!(
            strip_date_prefix("2026-07-08-harness-format-variants"),
            "harness-format-variants"
        );
        assert_eq!(strip_date_prefix("team-tasks"), "team-tasks");
        assert_eq!(
            strip_date_prefix("2026-team-tasks"),
            "2026-team-tasks",
            "a bare year is not a date prefix"
        );
    }

    // ── 5.5 filesystem import ───────────────────────────────────────────────

    #[test]
    fn import_filesystem_creates_change_and_artifacts_from_a_temp_tree() {
        let (conn, org, user) = setup();
        let root = temp_root("fs");
        write_file(&root, "openspec/changes/demo/proposal.md", "# Proposal");
        write_file(&root, "openspec/changes/demo/design.md", "# Design");
        write_file(&root, "openspec/changes/demo/specs/cap-a/spec.md", "# Spec A");

        let stats = import_filesystem(&conn, &org, &user, "nexus-mind", &root, false).unwrap();
        assert_eq!(stats.changes_created, 1);
        assert_eq!(stats.artifacts_created, 3);
        assert_eq!(stats.revisions_created, 3);

        let change = queries::get_sdd_change_by_name(&conn, &org, "nexus-mind", "demo")
            .unwrap()
            .expect("the change folder must become an sdd_changes row");
        assert_eq!(change.artifacts.len(), 3, "one artifact per file");
        assert_eq!(change.status, "active");

        for artifact in &change.artifacts {
            assert_eq!(artifact.latest_revision, 1, "a fresh import is revision 1");
        }

        let spec = change
            .artifacts
            .iter()
            .find(|a| a.kind == "spec")
            .expect("specs/cap-a/spec.md must import as kind=spec");
        assert_eq!(spec.capability, "cap-a", "the capability comes from the folder name");
        assert_eq!(
            spec.path.as_deref(),
            Some("openspec/changes/demo/specs/cap-a/spec.md"),
            "path is repo-relative"
        );

        let (source, git_path): (String, Option<String>) = conn
            .query_row(
                "SELECT source, git_path FROM sdd_artifact_revisions WHERE artifact_id = ?1",
                [&spec.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(source, "import");
        assert_eq!(git_path.as_deref(), Some("openspec/changes/demo/specs/cap-a/spec.md"));

        assert_eq!(
            artifact_content(&conn, &org, "nexus-mind", "demo", "proposal", "").as_deref(),
            Some("# Proposal"),
            "the revision carries the file's content verbatim"
        );

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn import_filesystem_skips_files_the_convention_does_not_name() {
        let (conn, org, user) = setup();
        let root = temp_root("skip");
        write_file(&root, "openspec/changes/demo/proposal.md", "# Proposal");
        write_file(&root, "openspec/changes/demo/MIGRATION_NOTE.md", "not an artifact");

        let stats = import_filesystem(&conn, &org, &user, "nexus-mind", &root, false).unwrap();
        assert_eq!(stats.artifacts_created, 1);
        assert_eq!(stats.skipped, 1, "the unrecognized file is counted, not imported");
    }

    // ── 5.7 archive folders ─────────────────────────────────────────────────

    #[test]
    fn import_filesystem_strips_date_prefix_and_marks_archive_changes() {
        let (conn, org, user) = setup();
        let root = temp_root("archive");
        write_file(
            &root,
            "openspec/changes/archive/2026-05-01-old-change/proposal.md",
            "# Old",
        );

        import_filesystem(&conn, &org, &user, "nexus-mind", &root, false).unwrap();

        assert!(
            queries::get_sdd_change_by_name(&conn, &org, "nexus-mind", "2026-05-01-old-change")
                .unwrap()
                .is_none(),
            "the dated folder name must not survive as the change name"
        );
        let change = queries::get_sdd_change_by_name(&conn, &org, "nexus-mind", "old-change")
            .unwrap()
            .expect("the archive folder imports under its stripped name");
        assert_eq!(change.status, "archived");
        assert_eq!(change.phase, "archive");

        // `status` and `archived_at` must AGREE. `list_sdd_changes` filters on
        // `archived_at IS NULL`, not on `status`, so setting only `status` would leave
        // every imported archive folder in the admin's default list forever — two fields
        // representing one fact, disagreeing.
        assert!(
            change.archived_at.is_some(),
            "an archived change must have archived_at set, or it still shows in the default list"
        );
        let default_list =
            queries::list_sdd_changes(&conn, &org, &nexusmind::models::types::SddChangeFilters::default())
                .unwrap();
        assert!(
            !default_list.iter().any(|c| c.name == "old-change"),
            "an archived change must NOT appear in the default list"
        );
        let with_archived = queries::list_sdd_changes(
            &conn,
            &org,
            &nexusmind::models::types::SddChangeFilters { include_archived: true, ..Default::default() },
        )
        .unwrap();
        assert!(
            with_archived.iter().any(|c| c.name == "old-change"),
            "…but it must appear on request"
        );

        // A8 — archiving does not withdraw the change: it stays a valid link target and
        // its artifacts stay retrievable.
        assert!(
            queries::sdd_change_exists(&conn, &org, "old-change").unwrap(),
            "an archived change remains a legitimate link_task_spec target"
        );
        let hydrated = queries::get_sdd_change(&conn, &org, &change.id).unwrap().unwrap();
        assert_eq!(hydrated.artifacts.len(), 1, "its artifacts survive");

        assert!(
            queries::get_sdd_change_by_name(&conn, &org, "nexus-mind", "archive")
                .unwrap()
                .is_none(),
            "the archive/ folder itself is a container, never a change"
        );

        fs::remove_dir_all(&root).ok();
    }

    // ── 5.10 phase inference on active changes ──────────────────────────────

    #[test]
    fn import_filesystem_infers_the_phase_of_active_changes_from_their_inventory() {
        let (conn, org, user) = setup();
        let root = temp_root("phase");
        write_file(&root, "openspec/changes/early/proposal.md", "# P");
        write_file(&root, "openspec/changes/far/proposal.md", "# P");
        write_file(&root, "openspec/changes/far/design.md", "# D");
        write_file(&root, "openspec/changes/far/tasks.md", "# T");
        write_file(&root, "openspec/changes/far/apply-progress.md", "# A");

        import_filesystem(&conn, &org, &user, "nexus-mind", &root, false).unwrap();

        let early = queries::get_sdd_change_by_name(&conn, &org, "nexus-mind", "early")
            .unwrap()
            .unwrap();
        assert_eq!(early.phase, "propose");
        let far = queries::get_sdd_change_by_name(&conn, &org, "nexus-mind", "far")
            .unwrap()
            .unwrap();
        assert_eq!(far.phase, "apply", "the furthest kind present wins");
        assert_eq!(far.status, "active");

        fs::remove_dir_all(&root).ok();
    }

    // ── 5.11 git provenance ─────────────────────────────────────────────────

    #[test]
    fn import_filesystem_sets_git_commit_when_available() {
        let (conn, org, user) = setup();
        let root = temp_root("git");
        write_file(&root, "openspec/changes/demo/proposal.md", "# P");

        // A real (tiny) repo — git_head must read its HEAD.
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&root)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        };
        if !git(&["init", "-q"]) {
            fs::remove_dir_all(&root).ok();
            return; // git is not installed — the other half of this test covers that.
        }
        git(&["config", "user.email", "t@t.t"]);
        git(&["config", "user.name", "T"]);
        git(&["add", "-A"]);
        git(&["commit", "-qm", "init"]);

        let head = git_head(&root).expect("git_head must read HEAD of a real repo");
        assert_eq!(head.len(), 40, "a full sha, not an abbreviation: {head}");

        import_filesystem(&conn, &org, &user, "nexus-mind", &root, false).unwrap();
        let commit: Option<String> = conn
            .query_row("SELECT git_commit FROM sdd_artifact_revisions LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(commit.as_deref(), Some(head.as_str()));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn import_filesystem_succeeds_without_git() {
        let (conn, org, user) = setup();
        let root = temp_root("nogit"); // never `git init`ed
        write_file(&root, "openspec/changes/demo/proposal.md", "# P");

        assert_eq!(git_head(&root), None, "a non-repo yields None, not a panic");

        let stats = import_filesystem(&conn, &org, &user, "nexus-mind", &root, false).unwrap();
        assert_eq!(stats.revisions_created, 1, "the import still succeeds");
        let commit: Option<String> = conn
            .query_row("SELECT git_commit FROM sdd_artifact_revisions LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(commit, None);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn import_filesystem_tolerates_a_root_with_no_openspec_tree() {
        let (conn, org, user) = setup();
        let root = std::env::temp_dir().join(format!("nm-import-sdd-bare-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();

        let stats = import_filesystem(&conn, &org, &user, "nexus-mind", &root, false).unwrap();
        assert_eq!(stats, ImportStats::default(), "nothing to do, and no error");
        assert_eq!(revision_count(&conn), 0);

        fs::remove_dir_all(&root).ok();
    }

    // ── 5.13 legacy memories ────────────────────────────────────────────────

    /// `created_at` is explicit: the whole memory-then-file ordering rests on it.
    fn seed_memory(conn: &Connection, id: &str, topic_key: &str, content: &str, created_at: &str) {
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, project, tool, content, tags, created_at,
                                   title, type, scope, topic_key, revision_count)
             VALUES (?1, 'org1', 'u1', 'nexus-mind', 'claude-code', ?2, '[\"sdd\"]', ?3,
                     'legacy', 'architecture', 'project', ?4, 1)",
            rusqlite::params![id, content, created_at, topic_key],
        )
        .unwrap();
    }

    fn tags_of(conn: &Connection, memory_id: &str) -> Vec<String> {
        let raw: String = conn
            .query_row("SELECT tags FROM memories WHERE id = ?1", [memory_id], |r| r.get(0))
            .unwrap();
        serde_json::from_str(&raw).unwrap_or_default()
    }

    #[test]
    fn parse_sdd_topic_key_reads_change_and_kind_and_rejects_the_rest() {
        assert_eq!(
            parse_sdd_topic_key("sdd/demo/design"),
            Some(("demo".to_string(), SddArtifactKind::Design))
        );
        assert_eq!(
            parse_sdd_topic_key("sdd/demo/apply-progress"),
            Some(("demo".to_string(), SddArtifactKind::ApplyProgress))
        );
        assert_eq!(
            parse_sdd_topic_key("sdd/demo/explore"),
            Some(("demo".to_string(), SddArtifactKind::Exploration)),
            "the skills wrote `explore`, the enum spells it `exploration`"
        );
        assert_eq!(parse_sdd_topic_key("sdd/demo/not-a-kind"), None);
        assert_eq!(parse_sdd_topic_key("sdd/demo"), None);
        assert_eq!(parse_sdd_topic_key("architecture/auth-model"), None);
    }

    #[test]
    fn import_legacy_memories_converts_sdd_topic_keys_to_artifacts() {
        let (conn, org, _user) = setup();
        seed_memory(&conn, "m1", "sdd/demo/design", "legacy design body", "2026-01-01T00:00:00Z");

        let stats = import_legacy_memories(&conn, &org, "nexus-mind", false).unwrap();
        assert_eq!(stats.changes_created, 1);
        assert_eq!(stats.artifacts_created, 1);
        assert_eq!(stats.revisions_created, 1);
        assert_eq!(stats.memories_tagged, 1);

        assert_eq!(
            artifact_content(&conn, &org, "nexus-mind", "demo", "design", "").as_deref(),
            Some("legacy design body"),
            "the artifact carries the memory's content"
        );
        let source: String = conn
            .query_row("SELECT source FROM sdd_artifact_revisions LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(source, "import");
    }

    #[test]
    fn import_legacy_memories_skips_a_topic_key_with_an_unknown_artifact_token() {
        let (conn, org, _user) = setup();
        seed_memory(&conn, "m1", "sdd/demo/gibberish", "?", "2026-01-01T00:00:00Z");

        // Skipped and counted — never a panic, and never a guessed kind.
        let stats = import_legacy_memories(&conn, &org, "nexus-mind", false).unwrap();
        assert_eq!(stats.revisions_created, 0);
        assert_eq!(stats.skipped, 1);
        assert_eq!(revision_count(&conn), 0);
        assert!(
            queries::get_sdd_change_by_name(&conn, &org, "nexus-mind", "demo").unwrap().is_none(),
            "an unimportable memory must not leave a change behind"
        );
    }

    // ── 5.19 the memories survive, tagged ───────────────────────────────────

    #[test]
    fn import_tags_legacy_memories_sdd_migrated_and_never_deletes_them() {
        let (conn, org, _user) = setup();
        seed_memory(&conn, "m1", "sdd/demo/design", "d", "2026-01-01T00:00:00Z");
        seed_memory(&conn, "m2", "sdd/demo/proposal", "p", "2026-01-02T00:00:00Z");
        let before: i64 =
            conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0)).unwrap();

        import_legacy_memories(&conn, &org, "nexus-mind", false).unwrap();

        let after: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0)).unwrap();
        assert_eq!(after, before, "the importer must not remove a single memory");
        for id in ["m1", "m2"] {
            assert!(
                tags_of(&conn, id).contains(&MIGRATED_TAG.to_string()),
                "{id} must be tagged {MIGRATED_TAG}"
            );
            assert!(tags_of(&conn, id).contains(&"sdd".to_string()), "existing tags survive");
        }

        // Idempotent tagging: a second pass must not duplicate the tag.
        import_legacy_memories(&conn, &org, "nexus-mind", false).unwrap();
        let tags = tags_of(&conn, "m1");
        assert_eq!(
            tags.iter().filter(|t| *t == MIGRATED_TAG).count(),
            1,
            "the tag is appended once, not once per run: {tags:?}"
        );
    }

    /// 5.20 — source-scan invariant. The importer moves the legacy memories; it is
    /// not allowed to remove them. The needles are assembled at runtime because
    /// `include_str!` pulls in this very test: a literal would match itself.
    #[test]
    fn importer_never_removes_a_memory() {
        let src = include_str!("import_sdd.rs");
        let verb: String = ["DEL", "ETE"].concat();
        for forbidden in [format!("{verb} FROM"), format!("{verb} ")] {
            assert!(
                !src.contains(&forbidden),
                "the importer must leave every legacy memory in place — found `{forbidden}`. \
                 Retiring them is an explicit user decision, taken in the admin."
            );
        }
    }

    // ── 5.15 ordering: the memory first, so the file wins ───────────────────

    #[test]
    fn import_orders_memory_before_filesystem_so_the_file_wins_as_the_latest_revision() {
        let (conn, org, user) = setup();
        let root = temp_root("both");
        write_file(&root, "openspec/changes/demo/design.md", "FILE version");
        seed_memory(&conn, "m1", "sdd/demo/design", "MEMORY version", "2026-01-01T00:00:00Z");

        // The order main() runs them in.
        import_legacy_memories(&conn, &org, "nexus-mind", false).unwrap();
        import_filesystem(&conn, &org, &user, "nexus-mind", &root, false).unwrap();

        let change = queries::get_sdd_change_by_name(&conn, &org, "nexus-mind", "demo")
            .unwrap()
            .unwrap();
        assert_eq!(change.artifacts.len(), 1, "both sources feed ONE artifact");
        let artifact = &change.artifacts[0];
        assert_eq!(artifact.latest_revision, 2);

        let revs: Vec<(i64, String)> = conn
            .prepare(
                "SELECT revision, content FROM sdd_artifact_revisions
                 WHERE artifact_id = ?1 ORDER BY revision",
            )
            .unwrap()
            .query_map([&artifact.id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            revs,
            vec![
                (1, "MEMORY version".to_string()),
                (2, "FILE version".to_string())
            ],
            "the memory is history; the file is the head"
        );

        assert_eq!(
            artifact_content(&conn, &org, "nexus-mind", "demo", "design", "").as_deref(),
            Some("FILE version"),
            "a read of the artifact returns the filesystem's content"
        );

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn import_legacy_memories_replays_the_memories_oldest_first() {
        let (conn, org, _user) = setup();
        // Two revisions of the SAME artifact, seeded newest-row-first to prove the
        // ORDER BY does the work rather than the insertion order.
        seed_memory(&conn, "m-new", "sdd/demo/design", "v2", "2026-02-01T00:00:00Z");
        seed_memory(&conn, "m-old", "sdd/demo/design", "v1", "2026-01-01T00:00:00Z");

        import_legacy_memories(&conn, &org, "nexus-mind", false).unwrap();

        let contents: Vec<String> = conn
            .prepare(
                "SELECT content FROM sdd_artifact_revisions
                 ORDER BY revision",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(contents, vec!["v1".to_string(), "v2".to_string()], "history reads forward");
    }

    // ── 5.17 idempotency ────────────────────────────────────────────────────

    #[test]
    fn import_is_idempotent_on_second_run() {
        let (conn, org, user) = setup();
        let root = temp_root("idem");
        write_file(&root, "openspec/changes/demo/proposal.md", "# P");
        write_file(&root, "openspec/changes/demo/design.md", "# D");
        write_file(&root, "openspec/changes/demo/specs/cap-a/spec.md", "# S");
        write_file(&root, "openspec/changes/archive/2026-05-01-gone/proposal.md", "# Old");
        seed_memory(&conn, "m1", "sdd/demo/tasks", "legacy tasks", "2026-01-01T00:00:00Z");

        let first = {
            let mut s = import_legacy_memories(&conn, &org, "nexus-mind", false).unwrap();
            s.merge(&import_filesystem(&conn, &org, &user, "nexus-mind", &root, false).unwrap());
            s
        };
        assert!(first.revisions_created > 0);
        let after_first = revision_count(&conn);

        let second = {
            let mut s = import_legacy_memories(&conn, &org, "nexus-mind", false).unwrap();
            s.merge(&import_filesystem(&conn, &org, &user, "nexus-mind", &root, false).unwrap());
            s
        };
        assert_eq!(
            second.revisions_created, 0,
            "a second run must create ZERO revisions — upsert_sdd_artifact de-dups by content hash"
        );
        assert_eq!(second.changes_created, 0);
        assert_eq!(second.artifacts_created, 0);
        assert_eq!(revision_count(&conn), after_first, "the revision table is unchanged");

        fs::remove_dir_all(&root).ok();
    }

    // ── 5.21 dry run ────────────────────────────────────────────────────────

    #[test]
    fn import_dry_run_writes_nothing() {
        let (conn, org, user) = setup();
        let root = temp_root("dry");
        write_file(&root, "openspec/changes/demo/proposal.md", "# P");
        write_file(&root, "openspec/changes/demo/design.md", "# D");
        seed_memory(&conn, "m1", "sdd/demo/tasks", "legacy tasks", "2026-01-01T00:00:00Z");

        let mut dry = import_legacy_memories(&conn, &org, "nexus-mind", true).unwrap();
        dry.merge(&import_filesystem(&conn, &org, &user, "nexus-mind", &root, true).unwrap());

        assert_eq!(dry.revisions_created, 3, "it reports what it would write");
        assert_eq!(dry.artifacts_created, 3);
        assert_eq!(dry.memories_tagged, 1);

        let changes: i64 =
            conn.query_row("SELECT COUNT(*) FROM sdd_changes", [], |r| r.get(0)).unwrap();
        assert_eq!(changes, 0, "--dry-run leaves the DB at zero sdd_changes rows");
        assert_eq!(revision_count(&conn), 0);
        assert!(
            !tags_of(&conn, "m1").contains(&MIGRATED_TAG.to_string()),
            "--dry-run does not tag either"
        );

        // And a real run afterwards still does the full job.
        let mut wet = import_legacy_memories(&conn, &org, "nexus-mind", false).unwrap();
        wet.merge(&import_filesystem(&conn, &org, &user, "nexus-mind", &root, false).unwrap());
        assert_eq!(wet.revisions_created, dry.revisions_created);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn dry_run_after_a_real_run_reports_no_work_left() {
        let (conn, org, user) = setup();
        let root = temp_root("dry2");
        write_file(&root, "openspec/changes/demo/proposal.md", "# P");

        import_filesystem(&conn, &org, &user, "nexus-mind", &root, false).unwrap();
        let dry = import_filesystem(&conn, &org, &user, "nexus-mind", &root, true).unwrap();

        assert_eq!(dry.revisions_created, 0, "the dry run agrees with the real one");
        assert_eq!(dry.changes_created, 0);
        assert_eq!(dry.skipped, 1, "the unchanged artifact is a no-op");

        fs::remove_dir_all(&root).ok();
    }
}
