//! Turning a scan path into a monorepo plan.
//!
//! When the operator points a run at a monorepo, each package under it is its
//! own body of knowledge and belongs in its own NexusMind project — not pooled
//! into one. This module answers three questions, in order, and nothing here
//! talks to a network or a terminal so every rule below is testable on a bare
//! directory tree:
//!
//! 1. **Detection** — which sub-projects does this path contain? Read from the
//!    workspace manifests a repo already has (`pnpm-workspace.yaml`, npm/yarn
//!    `workspaces`, a Cargo `[workspace]`, `go.work`), falling back to the
//!    `apps/*` + `packages/*` convention when there is no manifest.
//! 2. **Matching** — which of those already exist as backend projects? Keyed on
//!    the project *name*, which is unique per organization.
//! 3. **Planning** — for each one, create it, route into an existing project, or
//!    skip it. Nothing is created until the operator confirms the whole plan.
//!
//! The confirmed plan becomes a `.nexusmind.yaml` (see [`RepoConfig`]) written
//! at the scan root. The runner already routes a run per project from that file
//! via its own `ProjectResolver`; this module deliberately does not re-implement
//! that routing, only the file it reads. The schema here is a *minimal mirror*
//! of the runner's `RepositoryConfigV1`: the TUI does not depend on the backend
//! crate (see `Cargo.toml`), so the two definitions are kept in sync by the
//! round-trip test below and by the runner rejecting anything it does not
//! understand.

use crate::api::Project;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The directories a fallback scan treats as workspace roots when a repository
/// declares no workspace of its own. Each immediate child that carries a
/// manifest becomes a sub-project. Ordered, but detection de-duplicates by
/// directory so a package matched twice is still one row.
const CONVENTIONAL_WORKSPACE_DIRS: &[&str] = &["apps", "packages", "services", "libs"];

/// The manifests that mark a directory as a real package, for the fallback scan.
const PACKAGE_MANIFESTS: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "go.mod",
    "pyproject.toml",
    "pom.xml",
    "build.gradle",
];

// ── Detection ────────────────────────────────────────────────────────────────

/// A sub-project found under the scan root, before it is matched or planned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    /// A slug unique within the plan, used as the `.nexusmind.yaml` alias.
    pub alias: String,
    /// Human name — the backend project name, and the key matching is done on.
    pub name: String,
    /// The sub-project directory, relative to the scan root (e.g. `apps/web`).
    /// Empty string means the scan root itself (a single-project repo).
    pub rel_dir: String,
    /// How it was found, shown on the plan screen.
    pub via: &'static str,
}

impl Detected {
    /// The `paths` glob this sub-project routes on, relative to the scan root.
    ///
    /// Matched by the runner against each unit's path relative to `--path`, so
    /// it must be scan-root-relative, not repository-root-relative.
    pub fn route_glob(&self) -> String {
        if self.rel_dir.is_empty() {
            "**".to_string()
        } else {
            format!("{}/**", self.rel_dir.trim_end_matches('/'))
        }
    }
}

/// How the scan root is laid out. This decides how the run is *executed*, not
/// just what is detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// One Git repository holding packages. Routing is a `.nexusmind.yaml`
    /// written at its root, and the whole thing migrates in a single run.
    Monorepo,
    /// A plain folder holding independent Git repositories — the shape a
    /// microservice estate takes on disk. There is no repository to host a
    /// routing config (the runner discovers one via `git rev-parse`, which
    /// fails outside a checkout) and no shared history, so each repository is
    /// migrated on its own: one run apiece, with an explicit `--project`.
    RepoCollection,
}

/// Whether `root` or any ancestor holds a `.git`.
///
/// The routing config is discovered from the Git root, so a path outside a
/// checkout cannot use it — which is exactly what separates the two layouts.
pub fn in_git_repo(root: &Path) -> bool {
    let mut cursor = root;
    loop {
        if cursor.join(".git").exists() {
            return true;
        }
        match cursor.parent() {
            Some(parent) => cursor = parent,
            None => return false,
        }
    }
}

/// What the scan root actually is, and the projects it contains.
///
/// The two layouts are mutually exclusive by construction: inside a checkout
/// the packages are found by workspace manifests; outside one, the only things
/// that can be migrated are the repositories sitting directly inside.
pub fn survey(root: &Path) -> (Layout, Vec<Detected>) {
    if in_git_repo(root) {
        (Layout::Monorepo, detect(root))
    } else {
        (Layout::RepoCollection, detect_sibling_repos(root))
    }
}

/// Immediate children that are Git repositories of their own.
///
/// Deliberately permissive about what counts as a project: a repository with no
/// package manifest (a docs repo, an infrastructure repo, a mobile app) is
/// still a body of knowledge worth migrating, and excluding it by manifest
/// would silently drop real projects. The operator skips what they do not want
/// on the plan screen; the only things filtered here are scaffolding —
/// `*-worktrees` parents, and the dot-directories `immediate_subdirs` already
/// drops.
pub fn detect_sibling_repos(root: &Path) -> Vec<Detected> {
    let mut dirs: Vec<(String, &'static str)> = Vec::new();
    for name in immediate_subdirs(root) {
        // A worktree parent holds checkouts of a repo already listed on its own.
        if name.ends_with("-worktrees") {
            continue;
        }
        if root.join(&name).join(".git").exists() {
            dirs.push((name, "sibling git repository"));
        }
    }
    into_detected(dirs)
}

/// Every sub-project a scan root contains, de-duplicated and slug-stable.
///
/// An empty result is meaningful: the path is a single project, not a monorepo,
/// and the caller should fall back to today's single-`project` flow.
pub fn detect(root: &Path) -> Vec<Detected> {
    let mut dirs: Vec<(String, &'static str)> = Vec::new();

    if let Some(globs) = pnpm_workspace_globs(root) {
        for g in globs {
            expand_glob(root, &g, "pnpm-workspace.yaml", &mut dirs);
        }
    }
    if let Some(globs) = npm_workspace_globs(root) {
        for g in globs {
            expand_glob(root, &g, "package.json workspaces", &mut dirs);
        }
    }
    for g in cargo_workspace_members(root) {
        expand_glob(root, &g, "Cargo workspace", &mut dirs);
    }
    for g in go_work_uses(root) {
        expand_glob(root, &g, "go.work", &mut dirs);
    }

    // Only fall back to convention when nothing was declared: a repo that lists
    // its workspace means it, and `apps/legacy` that it left out should stay out.
    if dirs.is_empty() {
        for base in CONVENTIONAL_WORKSPACE_DIRS {
            let base_path = root.join(base);
            for sub in immediate_subdirs(&base_path) {
                let rel = format!("{base}/{sub}");
                if has_any_manifest(&root.join(&rel)) {
                    dirs.push((rel, "apps/packages convention"));
                }
            }
        }
    }

    into_detected(dirs)
}

/// A synthetic row for the repository itself — the catch-all project.
///
/// A monorepo's root-level knowledge (top-level guides, `/docs`, ADRs) lives
/// outside every package. Route only by package globs and that knowledge maps
/// nowhere, and the runner aborts the whole run with `ROUTING_UNMAPPED`. This
/// row's `**` glob is the *least* specific of all, so a package's own files
/// still route to the package (the resolver prefers the more specific pattern);
/// only what no package claims falls here. It also becomes the config's
/// `defaults.project`, the final backstop for anything a glob misses.
///
/// Kept out of [`detect`] so detection stays a pure "what packages exist" —
/// this is a routing decision layered on top, added only when packages exist.
pub fn repository_root_row(root: &Path, existing: &[Detected]) -> Detected {
    let name = root
        .canonicalize()
        .ok()
        .as_deref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repository".to_string());
    let taken: std::collections::BTreeSet<&str> =
        existing.iter().map(|d| d.alias.as_str()).collect();
    let mut alias = slugify(&name).unwrap_or_else(|| "repository".to_string());
    if taken.contains(alias.as_str()) {
        alias = format!("{alias}-root");
    }
    Detected {
        alias,
        name,
        rel_dir: String::new(),
        via: "repository root",
    }
}

/// Whether a row is the synthetic repository-root catch-all.
pub fn is_repository_root(row: &Detected) -> bool {
    row.rel_dir.is_empty()
}

/// Turns raw `(rel_dir, via)` hits into de-duplicated, slug-stable rows.
fn into_detected(mut dirs: Vec<(String, &'static str)>) -> Vec<Detected> {
    // A directory can be matched by more than one source (a package listed in
    // both pnpm and the convention scan); the first sighting wins its `via`.
    dirs.sort_by(|a, b| a.0.cmp(&b.0));
    dirs.dedup_by(|a, b| a.0 == b.0);

    let mut used = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for (rel, via) in dirs {
        let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        let mut alias = match slugify(&name) {
            Some(s) => s,
            // A directory whose name has no slug-safe characters at all still
            // deserves a stable alias rather than being dropped silently.
            None => "project".to_string(),
        };
        // Two packages named `ui` in different workspace roots must not collide
        // on the same alias — the second gets a numbered suffix.
        if used.contains(&alias) {
            let mut n = 2;
            while used.contains(&format!("{alias}-{n}")) {
                n += 1;
            }
            alias = format!("{alias}-{n}");
        }
        used.insert(alias.clone());
        out.push(Detected {
            alias,
            name,
            rel_dir: rel,
            via,
        });
    }
    out
}

fn pnpm_workspace_globs(root: &Path) -> Option<Vec<String>> {
    let bytes = std::fs::read(root.join("pnpm-workspace.yaml")).ok()?;
    let value: serde_yaml::Value = serde_yaml::from_slice(&bytes).ok()?;
    let packages = value.get("packages")?.as_sequence()?;
    Some(
        packages
            .iter()
            .filter_map(|v| v.as_str())
            .filter(|s| !s.starts_with('!')) // negations are exclusions, not roots
            .map(str::to_string)
            .collect(),
    )
}

fn npm_workspace_globs(root: &Path) -> Option<Vec<String>> {
    let bytes = std::fs::read(root.join("package.json")).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    // `workspaces` is either an array of globs or `{ "packages": [...] }`.
    let arr = value
        .get("workspaces")
        .and_then(|w| w.as_array().cloned().or_else(|| w.get("packages")?.as_array().cloned()))?;
    Some(
        arr.iter()
            .filter_map(|v| v.as_str())
            .filter(|s| !s.starts_with('!'))
            .map(str::to_string)
            .collect(),
    )
}

/// Cargo `[workspace] members`, extracted without a TOML dependency.
///
/// Deliberately best-effort: the members array can span lines, so this reads
/// from `members` to the next `]` and pulls the quoted strings out. A repo the
/// heuristic misreads still has the `apps/packages` fallback and, above all, the
/// operator confirming the plan before anything runs.
fn cargo_workspace_members(root: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(root.join("Cargo.toml")) else {
        return Vec::new();
    };
    if !text.contains("[workspace]") {
        return Vec::new();
    }
    let after = match text.split_once("members") {
        Some((_, rest)) => rest,
        None => return Vec::new(),
    };
    let Some(start) = after.find('[') else {
        return Vec::new();
    };
    let Some(end) = after[start..].find(']') else {
        return Vec::new();
    };
    quoted_strings(&after[start..start + end])
}

/// `go.work` `use (...)` entries, or single-line `use ./dir` forms.
fn go_work_uses(root: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(root.join("go.work")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        let rest = line.strip_prefix("use").map(str::trim).unwrap_or("");
        for token in rest.split(|c: char| c.is_whitespace() || c == '(' || c == ')') {
            let token = token.trim().trim_matches('"');
            if token.is_empty() {
                continue;
            }
            out.push(token.trim_start_matches("./").to_string());
        }
    }
    out
}

/// Pulls the double-quoted string literals out of a fragment.
fn quoted_strings(fragment: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = fragment.chars();
    while let Some(c) = chars.next() {
        if c == '"' {
            let s: String = chars.by_ref().take_while(|c| *c != '"').collect();
            if !s.is_empty() {
                out.push(s);
            }
        }
    }
    out
}

/// Expands a workspace glob to the concrete directories it names under `root`.
///
/// Only the shapes real manifests use are handled: an exact path, a trailing
/// `*` or `**` ("every immediate child"). A deeper glob is treated as its
/// leading fixed segments plus one wildcard level, which is enough to enumerate
/// packages without pulling in a glob engine the detection does not need.
fn expand_glob(root: &Path, glob: &str, via: &'static str, out: &mut Vec<(String, &'static str)>) {
    let glob = glob.trim_start_matches("./").trim_end_matches('/');
    let segments: Vec<&str> = glob.split('/').collect();
    let wildcard = segments.iter().position(|s| s.contains('*'));
    match wildcard {
        None => {
            // An exact directory. Only a real, manifest-bearing directory counts.
            let full = root.join(glob);
            if full.is_dir() && has_any_manifest(&full) {
                out.push((glob.to_string(), via));
            }
        }
        Some(idx) => {
            let prefix = segments[..idx].join("/");
            let base = if prefix.is_empty() {
                root.to_path_buf()
            } else {
                root.join(&prefix)
            };
            for sub in immediate_subdirs(&base) {
                let rel = if prefix.is_empty() {
                    sub.clone()
                } else {
                    format!("{prefix}/{sub}")
                };
                if has_any_manifest(&root.join(&rel)) {
                    out.push((rel, via));
                }
            }
        }
    }
}

fn immediate_subdirs(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.') && n != "node_modules" && n != "target")
        .collect();
    names.sort();
    names
}

fn has_any_manifest(dir: &Path) -> bool {
    PACKAGE_MANIFESTS.iter().any(|m| dir.join(m).is_file())
}

// ── Slugs ────────────────────────────────────────────────────────────────────

/// Normalizes a name into the runner's slug shape: lowercase ASCII letters,
/// digits and single hyphens, no leading or trailing hyphen, at most 64 chars.
///
/// Kept identical to the backend's `valid_slug` acceptance so a name that
/// slugifies here is one the runner will accept in the config it reads.
pub fn slugify(name: &str) -> Option<String> {
    let mut out = String::new();
    let mut last_dash = false;
    for c in name.chars() {
        let mapped = if c.is_ascii_alphanumeric() {
            c.to_ascii_lowercase()
        } else if matches!(c, ' ' | '_' | '-' | '.' | '/') {
            '-'
        } else {
            continue;
        };
        if mapped == '-' {
            if out.is_empty() || last_dash {
                continue; // no leading dash, no doubled dash
            }
            last_dash = true;
        } else {
            last_dash = false;
        }
        out.push(mapped);
        if out.len() >= 64 {
            break;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

// ── Planning ─────────────────────────────────────────────────────────────────

/// What the operator has decided to do with one detected sub-project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Create a new backend project named after the sub-project.
    Create,
    /// Route into an existing backend project, carried by id.
    Select(String),
    /// Leave this sub-project out of the migration entirely.
    Skip,
}

/// One row of the plan: a detected sub-project, whatever existing project its
/// name matched, and the decision.
#[derive(Debug, Clone)]
pub struct PlanRow {
    pub detected: Detected,
    /// An existing, non-archived backend project whose name matched, if any.
    pub matched: Option<Project>,
    pub action: Action,
    /// The project id this row resolved to once the plan executed — the created
    /// project's id, or the selected one. `None` until execution, or when
    /// skipped.
    pub resolved_project_id: Option<String>,
}

impl PlanRow {
    /// The project id this row currently points at without any creation: the
    /// selected id, or the matched project's id when the action is to select.
    pub fn selected_project_id(&self) -> Option<&str> {
        match &self.action {
            Action::Select(id) => Some(id.as_str()),
            _ => None,
        }
    }
}

/// Builds the initial plan: every detected sub-project defaulting to the safest
/// non-destructive choice its name affords.
///
/// A name that matches exactly one live project defaults to *routing into it* —
/// re-running a migration should feed the same project, not fork a second one.
/// A name with no match defaults to *create*. Nothing here is committed; these
/// are only the pre-selected actions the operator sees first.
pub fn build_plan(detected: Vec<Detected>, existing: &[Project]) -> Vec<PlanRow> {
    detected
        .into_iter()
        .map(|d| {
            let matched = existing
                .iter()
                .find(|p| !p.is_archived() && names_match(&p.name, &d.name))
                .cloned();
            let action = match &matched {
                Some(p) => Action::Select(p.id.clone()),
                None => Action::Create,
            };
            PlanRow {
                detected: d,
                matched,
                action,
                resolved_project_id: None,
            }
        })
        .collect()
}

/// Project names match when they are equal ignoring case and surrounding space.
///
/// Deliberately not slug-based: two names that slug the same (`My App`,
/// `my-app`) are usually distinct projects a human chose to name differently,
/// and silently merging them is the failure this whole gate exists to prevent.
fn names_match(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

// ── The `.nexusmind.yaml` file ───────────────────────────────────────────────

/// A minimal mirror of the runner's `RepositoryConfigV1`.
///
/// Only the fields the TUI writes are present; the runner's schema has more
/// (`agents`, per-project `agent_profile`), all optional, and its
/// `deny_unknown_fields` accepts this subset. Field names and the slug rules on
/// `repository.id` and the aliases must stay identical to the runner's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoConfig {
    pub version: u32,
    pub repository: RepoIdentity,
    #[serde(default, skip_serializing_if = "Defaults::is_empty")]
    pub defaults: Defaults,
    pub projects: BTreeMap<String, ProjectRoute>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoIdentity {
    pub id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Defaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

impl Defaults {
    fn is_empty(&self) -> bool {
        self.project.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectRoute {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
}

/// Assembles the config from a plan whose rows have been resolved to project
/// ids. Skipped rows and rows that never resolved are left out; the caller has
/// already refused to proceed if nothing is left.
pub fn build_config(
    repository_id: &str,
    client_id: Option<&str>,
    rows: &[PlanRow],
) -> RepoConfig {
    let mut projects = BTreeMap::new();
    for row in rows {
        if row.action == Action::Skip {
            continue;
        }
        let Some(project_id) = row.resolved_project_id.as_deref() else {
            continue;
        };
        projects.insert(
            row.detected.alias.clone(),
            ProjectRoute {
                project_id: project_id.to_string(),
                client_id: client_id
                    .filter(|c| !c.trim().is_empty())
                    .map(|c| c.trim().to_string()),
                paths: vec![row.detected.route_glob()],
                exclude: Vec::new(),
            },
        );
    }
    // The repository-root row is the catch-all: anything a package glob misses
    // routes here instead of failing the run as unmapped. Only when it actually
    // made it into the config (resolved, not skipped) can it be the default.
    let default = rows
        .iter()
        .find(|r| {
            is_repository_root(&r.detected)
                && r.action != Action::Skip
                && r.resolved_project_id.is_some()
        })
        .map(|r| r.detected.alias.clone())
        .filter(|alias| projects.contains_key(alias));

    RepoConfig {
        version: 1,
        repository: RepoIdentity {
            id: slugify(repository_id).unwrap_or_else(|| "repository".to_string()),
        },
        defaults: Defaults { project: default },
        projects,
    }
}

/// Serializes a config to the exact bytes written to `.nexusmind.yaml`.
pub fn to_yaml(config: &RepoConfig) -> Result<String, String> {
    serde_yaml::to_string(config).map_err(|e| e.to_string())
}

/// Reads and parses an existing `.nexusmind.yaml` at the scan root, if any.
///
/// `None` covers both "no file" and "a file this minimal mirror cannot read";
/// the caller treats either as "no prior config" and only uses a successful
/// parse to warn that writing will overwrite one, and to seed the plan.
pub fn read_existing(root: &Path) -> Option<RepoConfig> {
    let bytes = std::fs::read(root.join(".nexusmind.yaml")).ok()?;
    serde_yaml::from_slice(&bytes).ok()
}

/// Where the config is written for a given scan root.
pub fn config_path(root: &Path) -> std::path::PathBuf {
    root.join(".nexusmind.yaml")
}

#[cfg(test)]
mod tests;
