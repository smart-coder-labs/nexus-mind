//! One-shot, idempotent importer: backfills `openspec/changes/**` and the legacy
//! `sdd/*` memories into the SDD artifact store (design.md §5).
//!
//! # Two sources that live in two different places
//!
//! The importer READS the `openspec/` tree from disk and WRITES to the artifact
//! store. Those two halves have never sat on the same machine:
//!
//! * a developer's checkout has `openspec/`, and no production database;
//! * the Fly.io container has the database, and no checkout — it is a slim runtime
//!   image, there is no tree to walk.
//!
//! So the filesystem half must be able to push over HTTP, and the memory half —
//! whose source rows already live in the production database — must be able to run
//! DB-direct inside the container. Each half runs where its source is:
//!
//! ```text
//! # from a checkout: the tree is here, the database is not.
//! import-sdd --api-url https://api.nexusmind.dev --api-key nm_… --skip-memories
//!
//! # inside the container (fly ssh console): the database is here, the tree is not.
//! import-sdd --db /data/nexusmind.db --skip-filesystem
//!
//! # all-in-one, for a local dev database.
//! import-sdd --db ./data/nexusmind.db [--dry-run]
//! ```
//!
//! Safe to re-run: every write goes through `queries::upsert_sdd_artifact` — over
//! HTTP too, since `PUT /v1/sdd/artifacts` is that same call — which is idempotent
//! by content hash. A second run creates ZERO revisions, and the importer owns no
//! idempotency logic of its own.
//!
//! Two sources, imported in this order:
//!   1. the legacy `sdd/{change}/{artifact}` memories (older) — become revision 1,
//!   2. the filesystem (newer, reviewable) — becomes revision 2 and therefore wins.
//!
//! The filesystem half walks BOTH of openspec's trees:
//!   * `openspec/changes/*/` — the in-flight changes, as artifacts of a change;
//!   * `openspec/specs/*/spec.md` — the LIVING SPECIFICATIONS, as `sdd_specs`. These
//!     are not artifacts of anything: a main spec belongs to the project and outlives
//!     the changes that amend it. `--skip-specs` leaves that subtree alone.
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
    PatchChangeRequest, SaveArtifactRequest, SaveSpecRequest, SddArtifact, SddArtifactDetail,
    SddArtifactKind, SddChange, SddPhase, SddSpec, SddSpecDetail,
};
use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;

/// Provenance stamped on every revision this binary writes.
///
/// Both sinks honour it. The DB sink stamps it directly; the API sink sends it in
/// the body and `put_artifact_handler` records it (an unrecognized value is a 422,
/// so the column stays a closed set). An imported revision therefore says so on
/// either path — it is not silently relabelled as agent-authored.
const SOURCE: &str = "import";

/// Tag applied to every legacy memory carried into the artifact store.
const MIGRATED_TAG: &str = "sdd-migrated";

#[derive(Parser, Default)]
#[command(
    about = "Import openspec/changes/** and legacy sdd/* memories into the SDD artifact store"
)]
struct Args {
    /// No default: a default would make "neither --db nor --api-url" unrepresentable,
    /// and that combination is precisely the one that must be refused.
    #[arg(
        long = "db",
        alias = "db-path",
        env = "DB_PATH",
        help = "SQLite file to write to. Required for the memory half."
    )]
    db: Option<String>,

    #[arg(
        long,
        env = "NEXUSMIND_BASE_URL",
        help = "Backend base url. When set, the FILESYSTEM half pushes over HTTP instead of \
                touching a database."
    )]
    api_url: Option<String>,

    // `hide_env_values` is load-bearing, not cosmetic. Without it clap prints the
    // RESOLVED VALUE of the env var in `--help`:
    //
    //     --api-key <API_KEY>   [env: NEXUSMIND_API_KEY=nm_445e…]
    //
    // So merely asking the tool for help displays a live credential — on a shared
    // terminal, in a CI log, in a screen recording, in a pasted transcript. Asking for
    // help must never be a way to read a secret. (A `//` comment, not `///`: a doc
    // comment would become the help text and print this explanation to every user.)
    #[arg(
        long,
        env = "NEXUSMIND_API_KEY",
        hide_env_values = true,
        help = "API key for --api-url"
    )]
    api_key: Option<String>,

    #[arg(long, help = "Do not walk openspec/ — import the legacy memories only")]
    skip_filesystem: bool,

    #[arg(long, help = "Do not import the legacy sdd/* memories — walk openspec/ only")]
    skip_memories: bool,

    #[arg(
        long,
        help = "Do not walk openspec/specs/ — import the changes tree and the memories only"
    )]
    skip_specs: bool,

    #[arg(long, help = "Org to import into. Omit to resolve the single org.")]
    org_id: Option<String>,

    #[arg(long, default_value = "nexus-mind", help = "Project name for the imported changes")]
    project: String,

    #[arg(long, default_value = ".", help = "Repo root — the folder containing openspec/")]
    root: String,

    #[arg(long, help = "Report what would be imported without writing anything")]
    dry_run: bool,
}

// ── what this invocation is actually going to do ────────────────────────────

/// Where the filesystem half pushes to, when it pushes over HTTP.
#[derive(Debug, PartialEq, Eq)]
struct ApiTarget {
    base_url: String,
    api_key: String,
}

/// The two halves, resolved from the flags — each with its own destination, which
/// is the whole point: they can no longer be forced to run in the same place.
#[derive(Debug, PartialEq, Eq)]
struct Plan {
    /// Present only when a database is actually needed: for the memory half, or for
    /// a filesystem half that is not going over HTTP.
    db: Option<String>,
    api: Option<ApiTarget>,
    filesystem: bool,
    memories: bool,
    /// `openspec/specs/*/spec.md` — the living specifications. Part of the filesystem
    /// half (it needs the same tree and the same sink), so `--skip-filesystem` takes
    /// it out too; `--skip-specs` takes out only this subtree.
    specs: bool,
}

/// Rejects the impossible combinations up front, with a sentence naming the way
/// out — never a panic, and never a half-run that dies on its first write.
fn plan(args: &Args) -> Result<Plan> {
    let filesystem = !args.skip_filesystem;
    let memories = !args.skip_memories;

    if !filesystem && !memories {
        return Err(anyhow!(
            "--skip-filesystem and --skip-memories together leave nothing to import"
        ));
    }

    if args.db.is_none() && args.api_url.is_none() {
        return Err(anyhow!(
            "no destination: pass --db to write to a SQLite file, or --api-url (with --api-key) \
             to push over the HTTP API.\n\
             The filesystem half needs an openspec/ tree and the database half needs the database; \
             they are rarely on the same machine, which is why each half can be run on its own."
        ));
    }

    if memories && args.db.is_none() {
        return Err(anyhow!(
            "the legacy sdd/* memories live IN the database, so importing them needs --db — \
             there is no API to read them through.\n\
             Run that half where the database is (fly ssh console, --db /data/nexusmind.db), \
             or pass --skip-memories."
        ));
    }

    let api = match (&args.api_url, &args.api_key) {
        (Some(url), Some(key)) => Some(ApiTarget {
            base_url: url.trim_end_matches('/').to_string(),
            api_key: key.clone(),
        }),
        (Some(_), None) => {
            return Err(anyhow!(
                "--api-url needs a key: pass --api-key, or set NEXUSMIND_API_KEY"
            ))
        }
        (None, _) => None,
    };

    Ok(Plan {
        db: args.db.clone(),
        api,
        filesystem,
        memories,
        specs: filesystem && !args.skip_specs,
    })
}

// ── the write sink ──────────────────────────────────────────────────────────

/// Where the filesystem import writes. Discovery (`scan_change_dir`,
/// `kind_for_path`, `infer_phase`, …) is identical either way — only the
/// destination moves, which is what lets the half that needs the openspec tree run
/// on the machine that HAS the openspec tree.
pub enum Sink<'a> {
    /// Straight to SQLite. Only runnable where the database file is.
    Db { conn: &'a Connection, org_id: &'a str, user_id: &'a str },
    /// Over HTTP. Runnable from a checkout against a remote backend. The server owns
    /// the org (it comes from the API key), the author, and — crucially — the
    /// idempotency, so nothing about re-runnability is lost in the move.
    Api { client: reqwest::blocking::Client, base_url: String, api_key: String },
}

/// The body of `PUT /v1/sdd/artifacts`. The endpoint answers **200 always, never
/// 201**: "created" is not a property of the status code but of `created_revision`.
#[derive(Debug, Deserialize)]
struct SavedArtifact {
    artifact: SddArtifact,
    created_revision: bool,
}

/// The body of `PUT /v1/sdd/specs`. Like the artifact endpoint, it answers **200
/// always** — "created" is `created_revision`, not the status code.
#[derive(Debug, Deserialize)]
struct SavedSpec {
    spec: SddSpec,
    created_revision: bool,
}

/// One save, in the terms the importer counts in.
struct SaveOutcome {
    created_revision: bool,
    latest_revision: i64,
}

impl<'a> Sink<'a> {
    pub fn db(conn: &'a Connection, org_id: &'a str, user_id: &'a str) -> Self {
        Sink::Db { conn, org_id, user_id }
    }

    pub fn api(base_url: &str, api_key: &str) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        Ok(Sink::Api {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
        })
    }

    /// Saves one artifact. Idempotency belongs to the store on both paths: the DB
    /// sink calls `upsert_sdd_artifact`, and the API sink calls the endpoint that
    /// calls `upsert_sdd_artifact`. Same de-duplication, same content hash.
    fn save(&self, req: &SaveArtifactRequest) -> Result<SaveOutcome> {
        match self {
            Sink::Db { conn, org_id, user_id } => {
                let (artifact, created_revision) =
                    queries::upsert_sdd_artifact(conn, org_id, user_id, req, SOURCE)?;
                Ok(SaveOutcome { created_revision, latest_revision: artifact.latest_revision })
            }
            Sink::Api { client, base_url, api_key } => {
                let response = send_throttled(
                    client
                        .put(format!("{base_url}/v1/sdd/artifacts"))
                        .bearer_auth(api_key)
                        .header(reqwest::header::ACCEPT, "application/json")
                        .json(req),
                )?;
                let response = api_ok(
                    response,
                    &format!("PUT /v1/sdd/artifacts ({}/{})", req.change_name, req.kind),
                )?;
                let saved: SavedArtifact = response.json()?;
                Ok(SaveOutcome {
                    created_revision: saved.created_revision,
                    latest_revision: saved.artifact.latest_revision,
                })
            }
        }
    }

    /// Content hash of the latest revision, or `None` when there is no such artifact.
    /// This is how `--dry-run` predicts what a real save would decide, without writing.
    fn latest_hash(
        &self,
        project: &str,
        change_name: &str,
        kind: &str,
        capability: &str,
    ) -> Result<Option<String>> {
        match self {
            Sink::Db { conn, org_id, .. } => {
                db_latest_hash(conn, org_id, project, change_name, kind, capability)
            }
            Sink::Api { client, base_url, api_key } => {
                let response = client
                    .get(format!("{base_url}/v1/sdd/artifacts"))
                    .bearer_auth(api_key)
                    .header(reqwest::header::ACCEPT, "application/json")
                    .query(&[
                        ("project", project),
                        ("change_name", change_name),
                        ("kind", kind),
                        ("capability", capability),
                    ])
                    .send()?;

                // A kind with no artifact is a 404, and a 404 is the ANSWER here — "nothing
                // saved yet" — not a failure.
                if response.status() == reqwest::StatusCode::NOT_FOUND {
                    return Ok(None);
                }

                // `SddArtifactDetail` is serde-FLATTENED: the artifact's own fields sit
                // inline next to change_name/content/content_hash. Deserializing into a type
                // that nests them under an `artifact` key parses fine and silently yields
                // None for every one of them — so this uses the server's own type.
                let detail: SddArtifactDetail =
                    api_ok(response, "GET /v1/sdd/artifacts")?.json()?;
                Ok(detail.content_hash)
            }
        }
    }

    /// Saves one living specification. Same story as `save`: idempotency belongs to the
    /// store on both paths — the DB sink calls `upsert_sdd_spec`, and the API sink calls
    /// `PUT /v1/sdd/specs`, which calls `upsert_sdd_spec`. Same content hash, same
    /// de-duplication, so a second run creates zero revisions on either sink.
    fn save_spec(&self, req: &SaveSpecRequest) -> Result<SaveOutcome> {
        match self {
            Sink::Db { conn, org_id, user_id } => {
                let (spec, created_revision) =
                    queries::upsert_sdd_spec(conn, org_id, user_id, req, SOURCE)?;
                Ok(SaveOutcome { created_revision, latest_revision: spec.latest_revision })
            }
            Sink::Api { client, base_url, api_key } => {
                let response = send_throttled(
                    client
                        .put(format!("{base_url}/v1/sdd/specs"))
                        .bearer_auth(api_key)
                        .header(reqwest::header::ACCEPT, "application/json")
                        .json(req),
                )?;
                let response =
                    api_ok(response, &format!("PUT /v1/sdd/specs ({})", req.capability))?;
                let saved: SavedSpec = response.json()?;
                Ok(SaveOutcome {
                    created_revision: saved.created_revision,
                    latest_revision: saved.spec.latest_revision,
                })
            }
        }
    }

    /// Content hash of the spec's latest revision, or `None` when there is no such
    /// spec. This is how `--dry-run` predicts what a real save would decide.
    fn latest_spec_hash(&self, project: &str, capability: &str) -> Result<Option<String>> {
        match self {
            Sink::Db { conn, org_id, .. } => db_latest_spec_hash(conn, org_id, project, capability),
            Sink::Api { client, base_url, api_key } => {
                let response = client
                    .get(format!("{base_url}/v1/sdd/specs"))
                    .bearer_auth(api_key)
                    .header(reqwest::header::ACCEPT, "application/json")
                    .query(&[("project", project), ("capability", capability)])
                    .send()?;

                // A capability with no spec is a 404, and the 404 is the ANSWER —
                // "nothing saved yet" — not a failure.
                if response.status() == reqwest::StatusCode::NOT_FOUND {
                    return Ok(None);
                }

                // `SddSpecDetail` is serde-FLATTENED, exactly like `SddArtifactDetail`: the
                // spec's own fields sit inline next to content/content_hash. Use the
                // server's own type rather than a look-alike that would silently parse to
                // None for every field.
                let detail: SddSpecDetail = api_ok(response, "GET /v1/sdd/specs")?.json()?;
                Ok(detail.content_hash)
            }
        }
    }

    /// The change, by name. Over HTTP this is a list-and-filter — there is no by-name
    /// read — and `include_archived` is not optional: an archived change is hidden
    /// from the default list, and a re-import must still find the one it archived on
    /// the previous run in order to be a no-op rather than a duplicate.
    fn find_change(&self, project: &str, name: &str) -> Result<Option<SddChange>> {
        match self {
            Sink::Db { conn, org_id, .. } => {
                Ok(queries::get_sdd_change_by_name(conn, org_id, project, name)?)
            }
            Sink::Api { client, base_url, api_key } => {
                let response = send_throttled(
                    client
                        .get(format!("{base_url}/v1/sdd/changes"))
                        .bearer_auth(api_key)
                        .header(reqwest::header::ACCEPT, "application/json")
                        .query(&[("project", project), ("include_archived", "true")]),
                )?;
                let changes: Vec<SddChange> =
                    api_ok(response, "GET /v1/sdd/changes")?.json()?;
                Ok(changes.into_iter().find(|c| c.name == name))
            }
        }
    }

    /// Sets status and phase. The body carries ONLY the patchable fields:
    /// `PatchChangeRequest` is `deny_unknown_fields`, so a body that also named the
    /// change it is patching (`project`, `name`) would be a 422, not a courtesy.
    fn patch_change(&self, id: &str, patch: &PatchChangeRequest) -> Result<()> {
        match self {
            Sink::Db { conn, org_id, .. } => {
                queries::patch_sdd_change(conn, org_id, id, patch)?;
                Ok(())
            }
            Sink::Api { client, base_url, api_key } => {
                let response = send_throttled(
                    client
                        .patch(format!("{base_url}/v1/sdd/changes/{id}"))
                        .bearer_auth(api_key)
                        .header(reqwest::header::ACCEPT, "application/json")
                        .json(patch),
                )?;
                api_ok(response, "PATCH /v1/sdd/changes/:id")?;
                Ok(())
            }
        }
    }

    /// Soft-archives the change: sets `archived_at`, which is what `list_sdd_changes`
    /// actually filters on. `status = 'archived'` alone would leave every imported
    /// archive folder in the admin's default list forever — two fields stating one
    /// fact, disagreeing — so both are always set.
    ///
    /// Already-archived is success. The store's UPDATE is guarded on
    /// `archived_at IS NULL`, so a second attempt matches no row and the HTTP handler
    /// turns that into a 404; a re-import must survive its own previous run.
    fn archive_change(&self, id: &str) -> Result<()> {
        match self {
            Sink::Db { conn, org_id, .. } => {
                queries::archive_sdd_change(conn, org_id, id)?;
                Ok(())
            }
            Sink::Api { client, base_url, api_key } => {
                let response = send_throttled(
                    client
                        .delete(format!("{base_url}/v1/sdd/changes/{id}"))
                        .bearer_auth(api_key)
                        .header(reqwest::header::ACCEPT, "application/json"),
                )?;
                if response.status() == reqwest::StatusCode::NOT_FOUND {
                    return Ok(());
                }
                api_ok(response, "archive /v1/sdd/changes/:id")?;
                Ok(())
            }
        }
    }
}

/// How many times a throttled request is re-sent before the 429 is reported as the
/// failure it then is.
const MAX_THROTTLE_RETRIES: usize = 8;

/// Sends a request, waiting out the server's rate limiter.
///
/// Importing this repo's tree is ~116 requests and the free tier's bucket is 100 per
/// minute, so a full run WILL be throttled part of the way through — the first real
/// run against a live backend died at 429 with a partial import behind it. The
/// server names the number of seconds to wait in `Retry-After`; a one-shot backfill
/// waits, then finishes the job.
///
/// Every request the importer makes is idempotent (`PUT` de-dups by content hash,
/// the rest are reads, a status patch, and an archive that tolerates being already
/// archived), so re-sending one is always safe.
fn send_throttled(request: reqwest::blocking::RequestBuilder) -> Result<reqwest::blocking::Response> {
    let mut attempt = 0usize;
    loop {
        let this_try = request
            .try_clone()
            .ok_or_else(|| anyhow!("request cannot be retried (non-replayable body)"))?;
        let response = this_try.send()?;

        if response.status() != reqwest::StatusCode::TOO_MANY_REQUESTS
            || attempt >= MAX_THROTTLE_RETRIES
        {
            // Out of retries: hand the 429 to `api_ok`, which reports it verbatim.
            return Ok(response);
        }

        attempt += 1;
        let wait = retry_after_secs(&response).unwrap_or(1 << attempt.min(6)).clamp(1, 120);
        eprintln!("… rate limited by the server — waiting {wait}s (retry {attempt})");
        std::thread::sleep(std::time::Duration::from_secs(wait));
    }
}

/// The `Retry-After` the limiter sends with every 429, in seconds.
fn retry_after_secs(response: &reqwest::blocking::Response) -> Option<u64> {
    response.headers().get(reqwest::header::RETRY_AFTER)?.to_str().ok()?.trim().parse().ok()
}

/// Turns any non-2xx into an error carrying the status AND the server's own body.
/// An artifact over the 1 MiB limit must reach the operator as
/// `422 … artifact_too_large`, not as a panic and not as a bare "request failed".
fn api_ok(
    response: reqwest::blocking::Response,
    what: &str,
) -> Result<reqwest::blocking::Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().unwrap_or_default();
    Err(anyhow!("{what} failed: HTTP {} — {}", status.as_u16(), body.trim()))
}

/// What the importer did — or, under `--dry-run`, would do.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImportStats {
    pub changes_created: usize,
    pub artifacts_created: usize,
    /// Living specifications created — `openspec/specs/{capability}/spec.md`.
    pub specs_created: usize,
    /// Revisions across BOTH trees: an artifact revision and a spec revision are the
    /// same kind of event (a document changed) and are counted the same way.
    pub revisions_created: usize,
    pub memories_tagged: usize,
    pub skipped: usize,
}

impl ImportStats {
    pub fn merge(&mut self, other: &ImportStats) {
        self.changes_created += other.changes_created;
        self.artifacts_created += other.artifacts_created;
        self.specs_created += other.specs_created;
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
/// Every write goes through `queries::upsert_sdd_artifact` — directly on a `Sink::Db`,
/// and via `PUT /v1/sdd/artifacts` (which is that same call) on a `Sink::Api`. So a
/// second run is a no-op on either sink: the importer deliberately owns no insert
/// path, and no idempotency logic, of its own.
pub fn import_filesystem(
    sink: &Sink<'_>,
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

        let exists = sink.find_change(project, &change.name)?.is_some();
        count_change(exists, &change.name, &mut ledger, &mut stats);

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
            let outcome = save_artifact(sink, &req, dry_run, &mut ledger)?;
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
        let stored = sink
            .find_change(project, &change.name)?
            .ok_or_else(|| anyhow!("change {} vanished mid-import", change.name))?;
        sink.patch_change(
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
            sink.archive_change(&stored.id)?;
        }
    }

    Ok(stats)
}

/// One `openspec/specs/{capability}/spec.md` on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSpec {
    pub capability: String,
    pub abs_path: PathBuf,
    /// Repo-relative — becomes `git_path` and `sdd_specs.path`.
    pub rel_path: String,
}

/// Walks `{root}/openspec/specs/*/spec.md`.
///
/// ONLY `spec.md` is a living specification. A capability folder may hold design
/// notes or scratch files next to it, and they are not the contract — the convention
/// names exactly one file, so exactly one file is imported. A capability directory
/// without a `spec.md` yields nothing rather than an empty spec.
pub fn discover_specs(root: &Path) -> Result<Vec<DiscoveredSpec>> {
    let specs_root = root.join("openspec").join("specs");
    if !specs_root.is_dir() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for entry in std::fs::read_dir(&specs_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let capability = entry.file_name().to_string_lossy().to_string();
        let spec_md = entry.path().join("spec.md");
        if !spec_md.is_file() {
            continue;
        }
        out.push(DiscoveredSpec {
            rel_path: format!("openspec/specs/{capability}/spec.md"),
            capability,
            abs_path: spec_md,
        });
    }
    out.sort_by(|a, b| a.capability.cmp(&b.capability));
    Ok(out)
}

/// The first markdown H1 (`# …`), which is the spec's title by convention. `None`
/// when the document does not open with one — a missing title is not an error, and
/// inventing one from the filename would be a worse answer than no answer.
pub fn spec_title(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let rest = line.strip_prefix("# ")?;
        let title = rest.trim();
        (!title.is_empty()).then(|| title.to_string())
    })
}

/// Imports `openspec/specs/*/spec.md` — the LIVING SPECIFICATIONS.
///
/// The other tree. `import_filesystem` walks the in-flight changes; this walks the
/// contract those changes are amending. It runs over the same `Sink`, so it works
/// database-direct and over HTTP alike, and every write goes through
/// `upsert_sdd_spec` (directly, or via `PUT /v1/sdd/specs`, which IS that call), so
/// a second run creates zero revisions on either sink.
///
/// `merged_from_change_name` is deliberately NOT set here. The filesystem does not
/// record which change last merged into a spec — only git history does — and
/// inventing a provenance would be worse than admitting there is none. The agents
/// that run `sdd-archive` supply it on the live path, where it is actually known.
pub fn import_specs(
    sink: &Sink<'_>,
    project: &str,
    root: &Path,
    dry_run: bool,
) -> Result<ImportStats> {
    let mut stats = ImportStats::default();
    let mut ledger = DryLedger::default();
    let git_commit = git_head(root);

    for spec in discover_specs(root)? {
        let content = std::fs::read_to_string(&spec.abs_path)?;
        let req = SaveSpecRequest {
            project: project.to_string(),
            capability: spec.capability.clone(),
            title: spec_title(&content),
            content,
            path: Some(spec.rel_path.clone()),
            merged_from_change_name: None,
            git_commit: git_commit.clone(),
            source: Some(SOURCE.to_string()),
        };
        stats.merge(&save_spec(sink, &req, dry_run, &mut ledger)?);
    }

    Ok(stats)
}

/// The spec half's write path — whichever sink it is aimed at.
fn save_spec(
    sink: &Sink<'_>,
    req: &SaveSpecRequest,
    dry_run: bool,
    ledger: &mut DryLedger,
) -> Result<ImportStats> {
    let mut stats = ImportStats::default();

    if dry_run {
        let latest = sink.latest_spec_hash(&req.project, &req.capability)?;
        record_dry_spec(latest, req, ledger, &mut stats);
        return Ok(stats);
    }

    let outcome = sink.save_spec(req)?;
    if outcome.created_revision {
        stats.revisions_created += 1;
        if outcome.latest_revision == 1 {
            stats.specs_created += 1;
        }
    } else {
        stats.skipped += 1;
    }
    Ok(stats)
}

/// What a real spec save WOULD do, answered with reads only.
fn record_dry_spec(
    latest: Option<String>,
    req: &SaveSpecRequest,
    ledger: &mut DryLedger,
    stats: &mut ImportStats,
) {
    let key = format!("{}\u{1}{}", req.project, req.capability);
    match latest {
        None if ledger.specs.insert(key) => {
            stats.specs_created += 1;
            stats.revisions_created += 1;
        }
        None => stats.revisions_created += 1,
        Some(hash) if hash != sha256_hex(&req.content) => stats.revisions_created += 1,
        Some(_) => stats.skipped += 1,
    }
}

/// Content hash of the latest revision of `(project, capability)`, or `None` when the
/// spec does not exist yet.
fn db_latest_spec_hash(
    conn: &Connection,
    org_id: &str,
    project: &str,
    capability: &str,
) -> Result<Option<String>> {
    let hash = conn
        .query_row(
            "SELECT r.content_hash
             FROM sdd_spec_revisions r
             JOIN sdd_specs s ON s.id = r.spec_id
             WHERE s.org_id = ?1 AND s.project = ?2 AND s.capability = ?3
             ORDER BY r.revision DESC LIMIT 1",
            rusqlite::params![org_id, project, capability],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
    Ok(hash)
}

/// `--dry-run` writes nothing, so the DB cannot tell it what it already "created"
/// earlier in the same pass. This remembers, so two memories of one artifact are
/// reported as one artifact and two revisions — the same as a real run.
#[derive(Default)]
struct DryLedger {
    changes: std::collections::HashSet<String>,
    artifacts: std::collections::HashSet<String>,
    specs: std::collections::HashSet<String>,
}

/// Counts a change as created the first time it is seen and does not yet exist.
/// `exists` is the caller's to answer — a database read on one sink, a list call on
/// the other.
fn count_change(exists: bool, name: &str, ledger: &mut DryLedger, stats: &mut ImportStats) {
    if !exists && ledger.changes.insert(name.to_string()) {
        stats.changes_created += 1;
    }
}

/// 5.18 — idempotency is not the importer's to implement. `upsert_sdd_artifact`
/// compares the content hash against the latest revision and returns
/// `created_revision = false` when they match. `PUT /v1/sdd/artifacts` is that same
/// call behind a socket, so the property is identical on both sinks and a second
/// run writes nothing on either.
fn record_save(outcome: &SaveOutcome, stats: &mut ImportStats) {
    if outcome.created_revision {
        stats.revisions_created += 1;
        if outcome.latest_revision == 1 {
            stats.artifacts_created += 1;
        }
    } else {
        stats.skipped += 1;
    }
}

/// What a real save WOULD do, answered with reads only, so the numbers `--dry-run`
/// reports are the numbers a real run would produce.
fn record_dry(
    latest: Option<String>,
    req: &SaveArtifactRequest,
    ledger: &mut DryLedger,
    stats: &mut ImportStats,
) {
    let capability = req.capability.as_deref().unwrap_or("");
    let key = format!("{}\u{1}{}\u{1}{}", req.change_name, req.kind, capability);
    match latest {
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
}

/// The filesystem half's write path — whichever sink it is aimed at.
fn save_artifact(
    sink: &Sink<'_>,
    req: &SaveArtifactRequest,
    dry_run: bool,
    ledger: &mut DryLedger,
) -> Result<ImportStats> {
    let mut stats = ImportStats::default();

    if dry_run {
        let capability = req.capability.as_deref().unwrap_or("");
        let latest =
            sink.latest_hash(&req.project, &req.change_name, &req.kind, capability)?;
        record_dry(latest, req, ledger, &mut stats);
        return Ok(stats);
    }

    record_save(&sink.save(req)?, &mut stats);
    Ok(stats)
}

/// The memory half's write path. Deliberately NOT a sink: the memories already live
/// in the production database, so this half needs no openspec tree and can run
/// inside the container — and `created_by` is the memory's OWN author, which is
/// better provenance than any operator id an API key could supply.
fn save_memory_artifact(
    conn: &Connection,
    org_id: &str,
    author: &str,
    req: &SaveArtifactRequest,
    dry_run: bool,
    ledger: &mut DryLedger,
) -> Result<ImportStats> {
    let mut stats = ImportStats::default();

    if dry_run {
        let capability = req.capability.as_deref().unwrap_or("");
        let latest =
            db_latest_hash(conn, org_id, &req.project, &req.change_name, &req.kind, capability)?;
        record_dry(latest, req, ledger, &mut stats);
        return Ok(stats);
    }

    let (artifact, created_revision) =
        queries::upsert_sdd_artifact(conn, org_id, author, req, SOURCE)?;
    record_save(
        &SaveOutcome { created_revision, latest_revision: artifact.latest_revision },
        &mut stats,
    );
    Ok(stats)
}

/// Content hash of the latest revision of `(change, kind, capability)`, or `None`
/// when the artifact does not exist yet. Read-only — this is how `--dry-run`
/// predicts what `upsert_sdd_artifact` would decide.
fn db_latest_hash(
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

        let exists =
            queries::get_sdd_change_by_name(conn, org_id, project, &change_name)?.is_some();
        count_change(exists, &change_name, &mut ledger, &mut stats);

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
        let outcome = save_memory_artifact(conn, org_id, &author, &req, dry_run, &mut ledger)?;
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

fn main() {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    // A flag combination that cannot work is a sentence on stderr and a non-zero
    // exit, never a panic and never a stack trace.
    if let Err(e) = run(&args) {
        eprintln!("✗ {e}");
        std::process::exit(1);
    }
}

fn run(args: &Args) -> Result<()> {
    let plan = plan(args)?;
    let root = PathBuf::from(&args.root);

    if args.dry_run {
        eprintln!("→ DRY RUN — nothing will be written.");
    }

    // The database is opened only when a half actually needs one. `--api-url` with
    // `--skip-memories` touches no database at all — which is what lets it run from a
    // checkout, where there is no production database to open.
    let needs_db = plan.memories || (plan.filesystem && plan.api.is_none());
    let db = if needs_db {
        let path = plan
            .db
            .as_deref()
            .ok_or_else(|| anyhow!("--db is required for this combination of flags"))?;
        eprintln!("→ Opening DB at {path}");
        let conn = connect(path)?;
        migrations::run_all(&conn)?;
        let org_id = resolve_org(&conn, args.org_id.as_deref())?;
        let user_id = resolve_user(&conn, &org_id)?;
        eprintln!("→ Org {org_id}, user {user_id}, project {}", args.project);
        Some((conn, org_id, user_id))
    } else {
        None
    };

    let mut stats = ImportStats::default();

    // Memories first, so the filesystem (newer, reviewable) lands on top of them as
    // the latest revision (5.16 / design §5). When the two halves are run as two
    // separate invocations — which is the normal case now — that ordering is the
    // operator's to keep: run the container half before the checkout half.
    if plan.memories {
        let Some((conn, org_id, _)) = db.as_ref() else {
            return Err(anyhow!("the memory import needs --db"));
        };
        eprintln!("→ Importing the legacy sdd/* memories (database-direct)");
        stats.merge(&import_legacy_memories(conn, org_id, &args.project, args.dry_run)?);
    }

    if plan.filesystem {
        let sink = match &plan.api {
            Some(target) => {
                eprintln!("→ Importing {} over the API at {}", root.display(), target.base_url);
                Sink::api(&target.base_url, &target.api_key)?
            }
            None => {
                let Some((conn, org_id, user_id)) = db.as_ref() else {
                    return Err(anyhow!("the filesystem import needs --db or --api-url"));
                };
                eprintln!("→ Importing {} into the database", root.display());
                Sink::db(conn, org_id, user_id)
            }
        };
        stats.merge(&import_filesystem(&sink, &args.project, &root, args.dry_run)?);

        // The changes tree first, THEN the specs tree. The order is not load-bearing —
        // a spec is not an artifact of a change and neither half needs the other — but
        // it keeps the log in the order an operator thinks in: the drafts, then the
        // contract they amend.
        if plan.specs {
            eprintln!("→ Importing the living specifications (openspec/specs/*/spec.md)");
            stats.merge(&import_specs(&sink, &args.project, &root, args.dry_run)?);
        }
    }

    let verb = if args.dry_run { "would import" } else { "imported" };
    eprintln!(
        "✓ {verb}: {} changes, {} artifacts, {} specs, {} revisions, {} memories tagged {MIGRATED_TAG}, {} skipped.",
        stats.changes_created,
        stats.artifacts_created,
        stats.specs_created,
        stats.revisions_created,
        stats.memories_tagged,
        stats.skipped
    );
    Ok(())
}

#[cfg(test)]
mod help_leak_tests {
    use super::Args;
    use clap::CommandFactory;

    /// Asking a tool for help must never be a way to read a secret.
    ///
    /// clap prints the RESOLVED VALUE of an env var in `--help` unless told not to.
    /// With `NEXUSMIND_API_KEY` exported — which this repo's own CLAUDE.md instructs
    /// developers to do — `import-sdd --help` printed the live key to the terminal.
    #[test]
    fn help_never_prints_the_value_of_the_api_key_env_var() {
        let secret = "nm_thisisnotarealkey_0123456789abcdef";
        std::env::set_var("NEXUSMIND_API_KEY", secret);

        let help = Args::command().render_long_help().to_string();

        std::env::remove_var("NEXUSMIND_API_KEY");

        assert!(
            !help.contains(secret),
            "--help leaked the API key. `hide_env_values = true` is missing from the \
             api_key arg.\n\n{help}"
        );
        // The variable NAME is still shown — that is useful and carries nothing secret.
        assert!(help.contains("NEXUSMIND_API_KEY"), "the env var name should still be documented");
    }
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
    pub(super) fn temp_root(label: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("nm-import-sdd-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("openspec/changes")).unwrap();
        root
    }

    pub(super) fn write_file(root: &Path, rel: &str, content: &str) {
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

        let stats = import_filesystem(&Sink::db(&conn, &org, &user), "nexus-mind", &root, false).unwrap();
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

        let stats = import_filesystem(&Sink::db(&conn, &org, &user), "nexus-mind", &root, false).unwrap();
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

        import_filesystem(&Sink::db(&conn, &org, &user), "nexus-mind", &root, false).unwrap();

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

        import_filesystem(&Sink::db(&conn, &org, &user), "nexus-mind", &root, false).unwrap();

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

        import_filesystem(&Sink::db(&conn, &org, &user), "nexus-mind", &root, false).unwrap();
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

        let stats = import_filesystem(&Sink::db(&conn, &org, &user), "nexus-mind", &root, false).unwrap();
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

        let stats = import_filesystem(&Sink::db(&conn, &org, &user), "nexus-mind", &root, false).unwrap();
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
        import_filesystem(&Sink::db(&conn, &org, &user), "nexus-mind", &root, false).unwrap();

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
            s.merge(&import_filesystem(&Sink::db(&conn, &org, &user), "nexus-mind", &root, false).unwrap());
            s
        };
        assert!(first.revisions_created > 0);
        let after_first = revision_count(&conn);

        let second = {
            let mut s = import_legacy_memories(&conn, &org, "nexus-mind", false).unwrap();
            s.merge(&import_filesystem(&Sink::db(&conn, &org, &user), "nexus-mind", &root, false).unwrap());
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
        dry.merge(&import_filesystem(&Sink::db(&conn, &org, &user), "nexus-mind", &root, true).unwrap());

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
        wet.merge(&import_filesystem(&Sink::db(&conn, &org, &user), "nexus-mind", &root, false).unwrap());
        assert_eq!(wet.revisions_created, dry.revisions_created);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn dry_run_after_a_real_run_reports_no_work_left() {
        let (conn, org, user) = setup();
        let root = temp_root("dry2");
        write_file(&root, "openspec/changes/demo/proposal.md", "# P");

        import_filesystem(&Sink::db(&conn, &org, &user), "nexus-mind", &root, false).unwrap();
        let dry = import_filesystem(&Sink::db(&conn, &org, &user), "nexus-mind", &root, true).unwrap();

        assert_eq!(dry.revisions_created, 0, "the dry run agrees with the real one");
        assert_eq!(dry.changes_created, 0);
        assert_eq!(dry.skipped, 1, "the unchanged artifact is a no-op");

        fs::remove_dir_all(&root).ok();
    }
    #[test]
    fn discover_specs_finds_one_spec_per_capability_directory() {
        let root = temp_root("discover-specs");
        write_file(&root, "openspec/specs/harness-library/spec.md", "# Harness Library");
        write_file(&root, "openspec/specs/harness-config-review/spec.md", "# Config Review");
        // Not a contract: a stray file next to one, and a capability folder without a spec.
        write_file(&root, "openspec/specs/harness-library/notes.md", "scratch");
        fs::create_dir_all(root.join("openspec/specs/empty-capability")).unwrap();

        let found = discover_specs(&root).unwrap();
        assert_eq!(found.len(), 2, "only spec.md counts, and only where it exists");
        assert_eq!(found[0].capability, "harness-config-review", "sorted by capability");
        assert_eq!(found[0].rel_path, "openspec/specs/harness-config-review/spec.md");
        assert_eq!(found[1].capability, "harness-library");
    }

    #[test]
    fn discover_specs_tolerates_a_repo_with_no_specs_tree() {
        let root = temp_root("no-specs");
        assert!(discover_specs(&root).unwrap().is_empty(), "a missing tree is empty, not an error");
    }

    #[test]
    fn spec_title_takes_the_first_h1_and_nothing_else() {
        assert_eq!(spec_title("# Harness Library\n\n## Purpose"), Some("Harness Library".into()));
        assert_eq!(spec_title("## Purpose\n\n# Later H1"), Some("Later H1".into()));
        assert_eq!(spec_title("no heading at all"), None, "a missing title is None, not invented");
        assert_eq!(spec_title("#no-space-is-not-an-h1"), None);
    }

    #[test]
    fn import_specs_creates_a_living_spec_per_capability() {
        let (conn, org, user) = setup();
        let root = temp_root("specs");
        write_file(
            &root,
            "openspec/specs/harness-library/spec.md",
            "# Harness Library\n\n## Requirement: the library MUST be versioned",
        );
        write_file(&root, "openspec/specs/policy-engine/spec.md", "# Policy Engine");

        let stats = import_specs(&Sink::db(&conn, &org, &user), "nexus-mind", &root, false).unwrap();
        assert_eq!(stats.specs_created, 2);
        assert_eq!(stats.revisions_created, 2);
        assert_eq!(stats.changes_created, 0, "a spec is NOT an artifact of a change");

        let spec = queries::get_sdd_spec_by_capability(&conn, &org, "nexus-mind", "harness-library")
            .unwrap()
            .expect("openspec/specs/harness-library/spec.md must become an sdd_specs row");
        assert_eq!(spec.spec.latest_revision, 1);
        assert_eq!(spec.spec.title.as_deref(), Some("Harness Library"), "the H1 becomes the title");
        assert_eq!(spec.spec.path.as_deref(), Some("openspec/specs/harness-library/spec.md"));
        assert!(spec.content.unwrap().contains("MUST be versioned"), "the full contract is stored");

        // Provenance: source=import, git_path set, and NO invented merged_from_change_id.
        let rev = queries::get_sdd_spec_revision(&conn, &org, &spec.spec.id, 1).unwrap().unwrap();
        assert_eq!(rev.source, "import");
        assert_eq!(rev.git_path.as_deref(), Some("openspec/specs/harness-library/spec.md"));
        assert_eq!(
            rev.merged_from_change_id, None,
            "the filesystem does not know which change last merged — inventing one would be worse"
        );

        // No change was created as a side effect of importing a contract.
        let changes: i64 =
            conn.query_row("SELECT COUNT(*) FROM sdd_changes", [], |r| r.get(0)).unwrap();
        assert_eq!(changes, 0);
    }

    /// The idempotency contract, on the specs tree: a second run creates ZERO revisions.
    #[test]
    fn import_specs_is_idempotent_a_second_run_creates_no_revision() {
        let (conn, org, user) = setup();
        let root = temp_root("specs-idem");
        write_file(&root, "openspec/specs/cap/spec.md", "# Cap\n\nthe contract");

        let sink = Sink::db(&conn, &org, &user);
        let first = import_specs(&sink, "nexus-mind", &root, false).unwrap();
        assert_eq!(first.revisions_created, 1);

        let second = import_specs(&sink, "nexus-mind", &root, false).unwrap();
        assert_eq!(second.revisions_created, 0, "a re-import must create NO revision");
        assert_eq!(second.specs_created, 0);
        assert_eq!(second.skipped, 1, "…and must say so");

        let revisions: i64 =
            conn.query_row("SELECT COUNT(*) FROM sdd_spec_revisions", [], |r| r.get(0)).unwrap();
        assert_eq!(revisions, 1, "still exactly one revision in the database");

        // An edited contract, however, appends.
        write_file(&root, "openspec/specs/cap/spec.md", "# Cap\n\nthe contract, amended");
        let third = import_specs(&sink, "nexus-mind", &root, false).unwrap();
        assert_eq!(third.revisions_created, 1);
        assert_eq!(third.specs_created, 0, "the same spec, a new revision");
    }

    #[test]
    fn import_specs_dry_run_predicts_the_real_run_and_writes_nothing() {
        let (conn, org, user) = setup();
        let root = temp_root("specs-dry");
        write_file(&root, "openspec/specs/a/spec.md", "# A");
        write_file(&root, "openspec/specs/b/spec.md", "# B");

        let sink = Sink::db(&conn, &org, &user);
        let dry = import_specs(&sink, "nexus-mind", &root, true).unwrap();
        assert_eq!(dry.specs_created, 2);
        assert_eq!(dry.revisions_created, 2);

        let specs: i64 = conn.query_row("SELECT COUNT(*) FROM sdd_specs", [], |r| r.get(0)).unwrap();
        assert_eq!(specs, 0, "--dry-run must write nothing");

        // …and the numbers it predicted are the numbers the real run produces.
        let real = import_specs(&sink, "nexus-mind", &root, false).unwrap();
        assert_eq!(real.specs_created, dry.specs_created);
        assert_eq!(real.revisions_created, dry.revisions_created);
    }

    /// The two trees import independently and do not collide: a change's *delta* spec
    /// (`openspec/changes/x/specs/cap/spec.md`, an artifact) and the *living* spec
    /// (`openspec/specs/cap/spec.md`, an sdd_specs row) may name the same capability and
    /// remain two different documents with two different histories.
    #[test]
    fn a_delta_spec_and_the_living_spec_for_one_capability_are_two_documents() {
        let (conn, org, user) = setup();
        let root = temp_root("both-trees");
        write_file(&root, "openspec/changes/demo/specs/cap/spec.md", "## ADDED: a new requirement");
        write_file(&root, "openspec/specs/cap/spec.md", "# Cap\n\nthe whole contract");

        let sink = Sink::db(&conn, &org, &user);
        import_filesystem(&sink, "nexus-mind", &root, false).unwrap();
        import_specs(&sink, "nexus-mind", &root, false).unwrap();

        let delta = queries::get_sdd_artifact_by_kind(&conn, &org, "nexus-mind", "demo", "spec", Some("cap"))
            .unwrap()
            .expect("the change's delta spec is an artifact of that change");
        assert_eq!(delta.content.as_deref(), Some("## ADDED: a new requirement"));

        let living = queries::get_sdd_spec_by_capability(&conn, &org, "nexus-mind", "cap")
            .unwrap()
            .expect("the living specification is its own entity");
        assert_eq!(living.content.as_deref(), Some("# Cap\n\nthe whole contract"));

        assert_eq!(
            queries::list_sdd_specs(&conn, &org, &Default::default()).unwrap().len(),
            1,
            "the delta spec must NOT have leaked into sdd_specs"
        );
    }

    /// This repo's own tree, imported for real. `harness-library`, `harness-config-review`
    /// and `harness-install-approval` exist on disk right now and the platform has never
    /// seen them — that is the gap this change closes.
    #[test]
    fn import_specs_walks_this_repos_own_openspec_specs_tree() {
        let (conn, org, user) = setup();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

        let stats =
            import_specs(&Sink::db(&conn, &org, &user), "nexus-mind", &root, true).unwrap();
        assert!(
            stats.specs_created >= 3,
            "this repo has at least 3 living specifications on disk, and the importer must \
             find them — it found {}",
            stats.specs_created
        );
    }
}

/// The API sink — the half that makes the importer runnable at all.
///
/// The bug this closes: the importer READ `openspec/` from disk and WROTE straight
/// to the SQLite file. A developer's checkout has the tree and not the production
/// database; the Fly.io container has the database and not the tree. The two halves
/// never coexisted, so the thing could not be run anywhere.
///
/// These tests drive the filesystem half over HTTP against the REAL router — the
/// same one `main.rs` serves — because the properties that matter (idempotency, the
/// 422, the archive) are the SERVER's, and a hand-written fake would only prove the
/// fake agrees with itself. The one exception is the wire-format test, which needs
/// the raw bytes the real router would have already parsed away.
#[cfg(test)]
mod api_sink_tests {
    use super::tests::{temp_root, write_file};
    use super::*;
    use nexusmind::config::Config;
    use nexusmind::db::{connection::connect, migrations, queries};
    use nexusmind::store::sqlite::SqliteStore;
    use std::fs;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::{Arc, Mutex};

    // ── a real backend on a real socket ─────────────────────────────────────

    /// The REAL router, bound to an ephemeral port. Returns its base url, an admin
    /// API key, and a handle on the very store it writes to, so a test can assert
    /// against the server's own database rather than a mock's memory.
    fn spawn_backend() -> (String, String, SqliteStore) {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();

        let (router, store) =
            nexusmind::api::router::build_with_store(conn, Config::parse_from(["import-sdd-test"]));

        let api_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_org, _user, key) =
                queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                    axum::serve(listener, router.into_make_service()).await.unwrap();
                });
        });

        (format!("http://{addr}"), api_key, store)
    }

    /// Every `sdd_artifact_revisions` row the server holds — the ground truth for
    /// "a second run created nothing".
    fn server_revisions(store: &SqliteStore) -> i64 {
        let db = store.conn();
        let conn = db.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM sdd_artifact_revisions", [], |r| r.get(0)).unwrap()
    }

    fn server_spec_revisions(store: &SqliteStore) -> i64 {
        let db = store.conn();
        let conn = db.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM sdd_spec_revisions", [], |r| r.get(0)).unwrap()
    }

    fn server_change(store: &SqliteStore, name: &str) -> Option<nexusmind::models::types::SddChange> {
        let db = store.conn();
        let conn = db.lock().unwrap();
        let org: String =
            conn.query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0)).unwrap();
        queries::get_sdd_change_by_name(&conn, &org, "nexus-mind", name).unwrap()
    }

    // ── a recorder, for the bytes the real router would have parsed away ────

    #[derive(Clone, Debug)]
    struct RawRequest {
        line: String,
        headers: Vec<(String, String)>,
        body: String,
    }

    impl RawRequest {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
        }
        fn json(&self) -> serde_json::Value {
            serde_json::from_str(&self.body).expect("the body must be JSON")
        }
    }

    /// A stand-in backend that records the RAW request line, headers and body, and
    /// answers with a canned response. This is the only way to assert the WIRE
    /// format: the real router parses and normalizes a request before any assertion
    /// could see it, which is exactly what would hide a wrong body shape.
    struct Recorder {
        base_url: String,
        seen: Arc<Mutex<Vec<RawRequest>>>,
    }

    impl Recorder {
        fn start() -> Self {
            Self::start_throttling(0)
        }

        /// The first `throttle` **PUT**s are answered `429 Retry-After: 1` — the
        /// production token bucket in miniature, aimed at the request whose retry
        /// actually matters. A dropped read loses a lookup; a dropped PUT loses an
        /// artifact and leaves a partial import behind.
        fn start_throttling(throttle: usize) -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let base_url = format!("http://{}", listener.local_addr().unwrap());
            let seen: Arc<Mutex<Vec<RawRequest>>> = Arc::new(Mutex::new(Vec::new()));
            let recorded = Arc::clone(&seen);

            std::thread::spawn(move || {
                let mut remaining_throttles = throttle;
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { break };
                    let request = read_request(&mut stream);

                    let response = if request.line.starts_with("PUT") && remaining_throttles > 0 {
                        remaining_throttles -= 1;
                        throttled_response()
                    } else {
                        canned_response(&request.line)
                    };

                    recorded.lock().unwrap().push(request);
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                }
            });

            Recorder { base_url, seen }
        }

        fn requests(&self) -> Vec<RawRequest> {
            self.seen.lock().unwrap().clone()
        }

        fn first(&self, method: &str) -> RawRequest {
            self.requests()
                .into_iter()
                .find(|r| r.line.starts_with(method))
                .unwrap_or_else(|| panic!("the importer never sent a {method}"))
        }
    }

    fn read_request(stream: &mut std::net::TcpStream) -> RawRequest {
        let mut reader = BufReader::new(stream.try_clone().unwrap());

        let mut line = String::new();
        reader.read_line(&mut line).unwrap();

        let mut headers = Vec::new();
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).unwrap();
            if header.trim().is_empty() {
                break;
            }
            let (name, value) = header.split_once(':').expect("a header must have a colon");
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }

        let length: usize = headers
            .iter()
            .find(|(k, _)| k == "content-length")
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0u8; length];
        reader.read_exact(&mut body).unwrap();

        RawRequest {
            line: line.trim_end().to_string(),
            headers,
            body: String::from_utf8(body).unwrap(),
        }
    }

    /// What the production limiter actually sends: a 429 and the number of seconds to
    /// wait. `retry_after_secs` on the free tier's bucket rounds up to 1.
    fn throttled_response() -> String {
        let body = serde_json::json!({ "error": "Rate limit exceeded", "code": "rate_limited" })
            .to_string();
        format!(
            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nRetry-After: 1\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    /// Just enough of a reply for the importer to make progress. `Connection: close`
    /// keeps the socket handling to one request per connection.
    fn canned_response(request_line: &str) -> String {
        let artifact = serde_json::json!({
            "id": "a1", "change_id": "c1", "kind": "design", "capability": "",
            "latest_revision": 1,
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        });
        let change = serde_json::json!({
            "id": "c1", "org_id": "org1", "project": "nexus-mind", "name": "demo",
            "status": "active", "phase": "propose", "created_by": "u1",
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"
        });

        let body = if request_line.starts_with("PUT /v1/sdd/artifacts") {
            serde_json::json!({ "artifact": artifact, "created_revision": true }).to_string()
        } else if request_line.starts_with("GET /v1/sdd/changes") {
            serde_json::json!([change]).to_string()
        } else if request_line.starts_with("PATCH /v1/sdd/changes") {
            change.to_string()
        } else {
            return "HTTP/1.1 204 No Content\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
                .to_string();
        };

        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    // ── 5.22 the wire format ────────────────────────────────────────────────

    #[test]
    fn api_sink_puts_each_artifact_with_the_request_the_server_expects() {
        let recorder = Recorder::start();
        let root = temp_root("api-shape");
        write_file(&root, "openspec/changes/demo/design.md", "# Design");

        let sink = Sink::api(&recorder.base_url, "test-key").unwrap();
        import_filesystem(&sink, "nexus-mind", &root, false).unwrap();

        let put = recorder.first("PUT");
        assert_eq!(
            put.line, "PUT /v1/sdd/artifacts HTTP/1.1",
            "the collection route, not a by-id route: the server keys the artifact off the body"
        );
        assert_eq!(put.header("authorization"), Some("Bearer test-key"));
        assert_eq!(put.header("content-type"), Some("application/json"));

        let body = put.json();
        assert_eq!(body["project"], "nexus-mind");
        assert_eq!(body["change_name"], "demo");
        assert_eq!(body["kind"], "design");
        assert_eq!(body["capability"], "");
        assert_eq!(body["content"], "# Design");
        assert_eq!(
            body["path"], "openspec/changes/demo/design.md",
            "the repo-relative path, so the artifact can be traced back to its file"
        );

        // The status/phase patch carries ONLY the patchable fields. `PatchChangeRequest`
        // is `deny_unknown_fields`: a body that also names the change it is patching —
        // `project`, `name` — is a 422, not a helpful no-op.
        let patch = recorder.first("PATCH");
        let patched = patch.json();
        assert_eq!(patched["status"], "active");
        assert_eq!(patched["phase"], "design");
        for forbidden in ["project", "name", "change_name"] {
            assert!(
                patched.get(forbidden).is_none(),
                "PATCH /v1/sdd/changes/:id denies unknown fields — sending `{forbidden}` would 422"
            );
        }

        fs::remove_dir_all(&root).ok();
    }

    // ── 5.23 idempotency is the SERVER's, and it holds over HTTP ────────────

    #[test]
    fn api_import_creates_zero_revisions_on_a_second_identical_run() {
        let (base_url, api_key, store) = spawn_backend();
        let root = temp_root("api-idem");
        write_file(&root, "openspec/changes/demo/proposal.md", "# P");
        write_file(&root, "openspec/changes/demo/design.md", "# D");
        write_file(&root, "openspec/changes/demo/specs/cap-a/spec.md", "# S");
        write_file(&root, "openspec/changes/archive/2026-05-01-gone/proposal.md", "# Old");

        let sink = Sink::api(&base_url, &api_key).unwrap();

        let first = import_filesystem(&sink, "nexus-mind", &root, false).unwrap();
        assert_eq!(first.changes_created, 2);
        assert_eq!(first.artifacts_created, 4);
        assert_eq!(first.revisions_created, 4);
        let after_first = server_revisions(&store);
        assert_eq!(after_first, 4);

        let second = import_filesystem(&sink, "nexus-mind", &root, false).unwrap();
        assert_eq!(
            second.revisions_created, 0,
            "a second run must create ZERO revisions — the SERVER de-dups by content hash, \
             so idempotency survives the move to HTTP without the importer owning any of it"
        );
        assert_eq!(second.changes_created, 0);
        assert_eq!(second.artifacts_created, 0);
        assert_eq!(second.skipped, 4, "every artifact is an unchanged no-op");
        assert_eq!(server_revisions(&store), after_first, "the revision table is untouched");

        fs::remove_dir_all(&root).ok();
    }

    // ── 5.24 the archive: status AND archived_at ────────────────────────────

    #[test]
    fn api_import_archives_an_archive_folder_with_both_status_and_archived_at() {
        let (base_url, api_key, store) = spawn_backend();
        let root = temp_root("api-archive");
        write_file(&root, "openspec/changes/archive/2026-05-01-old-change/proposal.md", "# Old");

        let sink = Sink::api(&base_url, &api_key).unwrap();
        import_filesystem(&sink, "nexus-mind", &root, false).unwrap();

        let change = server_change(&store, "old-change")
            .expect("the archive folder imports under its date-stripped name");
        assert_eq!(change.status, "archived");
        assert_eq!(change.phase, "archive");

        // `list_sdd_changes` filters on `archived_at IS NULL`, NOT on `status`. Patching
        // the status alone would leave every imported archive folder sitting in the
        // admin's default list forever — so the API sink must patch AND soft-archive.
        assert!(
            change.archived_at.is_some(),
            "archived_at must be set, or the change never leaves the default list"
        );

        // And a re-import survives it: the store's archive UPDATE is guarded on
        // `archived_at IS NULL`, so the second attempt reports no row and the handler
        // answers 404. That is "already archived", not a failure.
        let second = import_filesystem(&sink, "nexus-mind", &root, false)
            .expect("re-archiving an already-archived change must not be an error");
        assert_eq!(second.revisions_created, 0);
        assert!(server_change(&store, "old-change").unwrap().archived_at.is_some());

        fs::remove_dir_all(&root).ok();
    }

    // ── 5.25 the server's 422 arrives as a sentence, not a panic ────────────

    #[test]
    fn api_import_surfaces_an_oversized_artifact_as_the_servers_422() {
        let (base_url, api_key, _store) = spawn_backend();
        let root = temp_root("api-toobig");
        // SDD_MAX_ARTIFACT_BYTES is 1 MiB, and the guard is the store's, not the
        // importer's — the importer's job is to relay the refusal legibly.
        write_file(&root, "openspec/changes/huge/design.md", &"x".repeat(1_048_577));

        let sink = Sink::api(&base_url, &api_key).unwrap();
        let err = import_filesystem(&sink, "nexus-mind", &root, false)
            .expect_err("an oversized artifact must fail the import, not be silently dropped");

        let message = err.to_string();
        assert!(message.contains("422"), "the status must be in the message: {message}");
        assert!(
            message.contains("artifact_too_large"),
            "the SERVER's own reason must survive to the operator: {message}"
        );

        fs::remove_dir_all(&root).ok();
    }

    // ── 5.26 --dry-run over HTTP writes nothing ─────────────────────────────

    #[test]
    fn api_dry_run_reads_but_never_writes() {
        let (base_url, api_key, store) = spawn_backend();
        let root = temp_root("api-dry");
        write_file(&root, "openspec/changes/demo/proposal.md", "# P");
        write_file(&root, "openspec/changes/demo/design.md", "# D");

        let sink = Sink::api(&base_url, &api_key).unwrap();
        let dry = import_filesystem(&sink, "nexus-mind", &root, true).unwrap();

        assert_eq!(dry.artifacts_created, 2, "it reports what a real run would write");
        assert_eq!(dry.revisions_created, 2);
        assert_eq!(server_revisions(&store), 0, "--dry-run must not have written a single row");
        assert!(server_change(&store, "demo").is_none(), "…nor created the change");

        fs::remove_dir_all(&root).ok();
    }

    // ── 5.27 the rate limiter, which a real run WILL hit ────────────────────

    /// Found by running the thing: importing this repo's own `openspec/` tree over the
    /// API is ~116 requests, and the free tier's token bucket holds 100 per minute. The
    /// real run died at `429` two thirds of the way through, having written a partial
    /// import — which is the same "unrunnable in production" bug in a new costume.
    ///
    /// The server says exactly how long to wait. A one-shot backfill's only sane
    /// answer to that is to wait, and then finish the job.
    #[test]
    fn api_sink_waits_out_a_rate_limit_and_completes_the_import() {
        // The first PUT is refused with a 429; the second must be the same PUT again.
        let recorder = Recorder::start_throttling(1);
        let root = temp_root("api-429");
        write_file(&root, "openspec/changes/demo/design.md", "# Design");

        let sink = Sink::api(&recorder.base_url, "test-key").unwrap();
        let stats = import_filesystem(&sink, "nexus-mind", &root, false)
            .expect("a 429 is a 'wait', not a failure — the import must still complete");

        assert_eq!(stats.revisions_created, 1, "the artifact lands after the throttle lifts");

        let throttled: Vec<_> =
            recorder.requests().into_iter().filter(|r| r.line.starts_with("PUT")).collect();
        assert_eq!(
            throttled.len(),
            2,
            "the PUT that was throttled must be RE-SENT, not dropped: {throttled:#?}"
        );
        assert_eq!(
            throttled[0].body, throttled[1].body,
            "the retry must carry the same body — a retry that mutates the request is not a retry"
        );

        fs::remove_dir_all(&root).ok();
    }

    // ── 5.28 the flags: a clear refusal, never a panic ──────────────────────

    #[test]
    fn plan_refuses_when_neither_a_db_nor_an_api_url_is_given() {
        let err = plan(&Args::default()).expect_err("with no destination there is nothing to do");
        let message = err.to_string();
        assert!(message.contains("--db"), "the message must name the flag: {message}");
        assert!(message.contains("--api-url"), "…and the alternative: {message}");
    }

    #[test]
    fn plan_refuses_the_memory_import_without_a_db() {
        // The legacy memories live IN the database. Asking to migrate them while
        // pointing only at an HTTP API is not a thing that can be done.
        let err = plan(&Args {
            api_url: Some("https://api.example.com".into()),
            api_key: Some("k".into()),
            ..Args::default()
        })
        .expect_err("the memory half cannot run without the database it reads from");
        assert!(
            err.to_string().contains("--skip-memories") && err.to_string().contains("--db"),
            "the message must name the way out: {err}"
        );
    }

    #[test]
    fn plan_refuses_an_api_url_with_no_key() {
        let err = plan(&Args {
            api_url: Some("https://api.example.com".into()),
            skip_memories: true,
            ..Args::default()
        })
        .expect_err("an unauthenticated push would 401 on the first artifact");
        assert!(err.to_string().contains("--api-key"), "{err}");
    }

    #[test]
    fn plan_routes_the_filesystem_over_http_and_the_memories_to_the_db() {
        // The one invocation that does both halves: the memories go where they live
        // (the DB), the filesystem goes where it can be read from (a checkout, pushed
        // over the API). This is the whole point of the change.
        let resolved = plan(&Args {
            db: Some("/data/nexusmind.db".into()),
            api_url: Some("https://api.example.com/".into()),
            api_key: Some("nm_key".into()),
            ..Args::default()
        })
        .unwrap();

        assert!(resolved.memories && resolved.filesystem);
        assert_eq!(resolved.db.as_deref(), Some("/data/nexusmind.db"));
        let api = resolved.api.expect("the filesystem half must be routed over HTTP");
        assert_eq!(api.base_url, "https://api.example.com", "the trailing slash is trimmed");
        assert_eq!(api.api_key, "nm_key");
    }

    #[test]
    fn plan_accepts_each_half_on_its_own() {
        // Half 1 — from a developer's checkout, which HAS openspec/, against the remote
        // backend, which HAS the database.
        let filesystem_only = plan(&Args {
            api_url: Some("https://api.example.com".into()),
            api_key: Some("k".into()),
            skip_memories: true,
            ..Args::default()
        })
        .unwrap();
        assert!(filesystem_only.filesystem && !filesystem_only.memories);
        assert!(filesystem_only.db.is_none(), "no database is needed, and none is opened");

        // Half 2 — inside the container, which HAS the database and no checkout.
        let memories_only = plan(&Args {
            db: Some("/data/nexusmind.db".into()),
            skip_filesystem: true,
            ..Args::default()
        })
        .unwrap();
        assert!(memories_only.memories && !memories_only.filesystem);
        assert!(memories_only.api.is_none());
    }

    // ── The living specifications (openspec/specs/*/spec.md) ────────────────

    #[test]
    fn plan_walks_the_specs_tree_by_default_and_skip_specs_takes_it_out() {
        let default = plan(&Args {
            db: Some("/data/nexusmind.db".into()),
            skip_memories: true,
            ..Args::default()
        })
        .unwrap();
        assert!(default.specs, "the specs tree is walked by default — it is the source of truth");

        let skipped = plan(&Args {
            db: Some("/data/nexusmind.db".into()),
            skip_memories: true,
            skip_specs: true,
            ..Args::default()
        })
        .unwrap();
        assert!(!skipped.specs);
        assert!(skipped.filesystem, "--skip-specs leaves the changes tree alone");

        // The specs tree lives UNDER openspec/, so --skip-filesystem takes it out too.
        let no_fs = plan(&Args {
            db: Some("/data/nexusmind.db".into()),
            skip_filesystem: true,
            ..Args::default()
        })
        .unwrap();
        assert!(!no_fs.specs, "--skip-filesystem must not leave the specs half walking a tree");
    }

    /// The specs half runs over the API sink too — against the REAL router — and the
    /// idempotency is the SERVER's on that path as well. `PUT /v1/sdd/specs` IS
    /// `upsert_sdd_spec` behind a socket, so a second run creates zero revisions
    /// without the importer owning any de-duplication of its own.
    #[test]
    fn api_import_specs_creates_zero_revisions_on_a_second_identical_run() {
        let (base_url, api_key, store) = spawn_backend();
        let root = temp_root("api-specs");
        write_file(&root, "openspec/specs/harness-library/spec.md", "# Harness Library\n\nthe contract");
        write_file(&root, "openspec/specs/policy-engine/spec.md", "# Policy Engine");

        let sink = Sink::api(&base_url, &api_key).unwrap();

        let first = import_specs(&sink, "nexus-mind", &root, false).unwrap();
        assert_eq!(first.specs_created, 2);
        assert_eq!(first.revisions_created, 2);
        assert_eq!(server_spec_revisions(&store), 2);

        let second = import_specs(&sink, "nexus-mind", &root, false).unwrap();
        assert_eq!(
            second.revisions_created, 0,
            "a second run must create ZERO revisions — the server de-dups by content hash"
        );
        assert_eq!(second.specs_created, 0);
        assert_eq!(second.skipped, 2);
        assert_eq!(server_spec_revisions(&store), 2, "the revision table is untouched");

        // The provenance the API sink sent is the provenance the server stored: this is
        // the bug the artifact path had to fix (`source` was hard-coded to `agent`), and
        // the specs path must not reintroduce it.
        let db = store.conn();
        let conn = db.lock().unwrap();
        let (source, git_path): (String, Option<String>) = conn
            .query_row(
                "SELECT r.source, r.git_path FROM sdd_spec_revisions r
                 JOIN sdd_specs s ON s.id = r.spec_id
                 WHERE s.capability = 'harness-library'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(source, "import", "the importer's provenance must survive the trip over HTTP");
        assert_eq!(git_path.as_deref(), Some("openspec/specs/harness-library/spec.md"));

        drop(conn);
        fs::remove_dir_all(&root).ok();
    }

    /// An oversized contract reaches the operator as the server's own 422, not as a panic.
    #[test]
    fn api_import_specs_surfaces_an_oversized_spec_as_the_servers_422() {
        let (base_url, api_key, _store) = spawn_backend();
        let root = temp_root("api-specs-big");
        write_file(&root, "openspec/specs/huge/spec.md", &"x".repeat(1_048_577));

        let sink = Sink::api(&base_url, &api_key).unwrap();
        let err = import_specs(&sink, "nexus-mind", &root, false).unwrap_err().to_string();

        assert!(err.contains("422"), "the operator must see the status: {err}");
        assert!(err.contains("spec_too_large"), "…and the server's own code: {err}");

        fs::remove_dir_all(&root).ok();
    }
}
