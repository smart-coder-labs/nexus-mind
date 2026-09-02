use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// A throwaway directory tree for one test. Unique per test without a tempfile
/// dependency, and cleaned up on drop so a failing test leaves nothing behind.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "nmtui-monorepo-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    /// Writes a file, creating parent directories. `rel` is `/`-separated.
    fn file(&self, rel: &str, contents: &str) -> &Self {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
        self
    }

    fn root(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn project(id: &str, name: &str) -> Project {
    Project {
        id: id.into(),
        name: name.into(),
        client_id: None,
        archived_at: None,
    }
}

// ── Slugs ────────────────────────────────────────────────────────────────────

/// The slug rules must accept exactly what the runner's `valid_slug` accepts,
/// or a name the TUI slugifies here is rejected by the config the runner reads.
#[test]
fn slugify_matches_the_runners_slug_shape() {
    assert_eq!(slugify("web").as_deref(), Some("web"));
    assert_eq!(slugify("My App").as_deref(), Some("my-app"));
    assert_eq!(slugify("@acme/ui-kit").as_deref(), Some("acme-ui-kit"));
    assert_eq!(slugify("  spaced  ").as_deref(), Some("spaced"));
    assert_eq!(slugify("__leading").as_deref(), Some("leading"));
    assert_eq!(slugify("trailing--").as_deref(), Some("trailing"));
    assert_eq!(slugify("a__b").as_deref(), Some("a-b"), "no doubled dashes");
    assert_eq!(slugify("").as_deref(), None);
    assert_eq!(slugify("///").as_deref(), None, "no slug-safe characters");
    assert!(slugify(&"x".repeat(200)).unwrap().len() <= 64);
}

// ── Detection ────────────────────────────────────────────────────────────────

#[test]
fn a_single_project_repo_detects_nothing() {
    let s = Scratch::new();
    s.file("package.json", r#"{"name":"solo"}"#)
        .file("src/index.ts", "");
    assert!(
        detect(s.root()).is_empty(),
        "no workspace and no apps/packages means the single-project flow"
    );
}

#[test]
fn pnpm_workspace_globs_are_expanded_to_manifest_bearing_dirs() {
    let s = Scratch::new();
    s.file(
        "pnpm-workspace.yaml",
        "packages:\n  - 'apps/*'\n  - 'packages/*'\n  - '!packages/ignored'\n",
    )
    .file("apps/web/package.json", "{}")
    .file("apps/api/package.json", "{}")
    .file("packages/ui/package.json", "{}")
    .file("packages/empty/README.md", ""); // no manifest → not a package

    let found: Vec<String> = detect(s.root()).into_iter().map(|d| d.rel_dir).collect();
    assert_eq!(found, vec!["apps/api", "apps/web", "packages/ui"]);
}

#[test]
fn npm_workspaces_array_and_object_forms_both_parse() {
    let arr = Scratch::new();
    arr.file("package.json", r#"{"workspaces":["apps/*"]}"#)
        .file("apps/web/package.json", "{}");
    assert_eq!(detect(arr.root()).len(), 1);

    let obj = Scratch::new();
    obj.file("package.json", r#"{"workspaces":{"packages":["apps/*"]}}"#)
        .file("apps/web/package.json", "{}");
    assert_eq!(detect(obj.root()).len(), 1);
}

#[test]
fn a_cargo_workspace_lists_its_members_even_across_lines() {
    let s = Scratch::new();
    s.file(
        "Cargo.toml",
        "[workspace]\nmembers = [\n  \"crates/core\",\n  \"crates/cli\",\n]\n",
    )
    .file("crates/core/Cargo.toml", "")
    .file("crates/cli/Cargo.toml", "");
    let found: Vec<String> = detect(s.root()).into_iter().map(|d| d.rel_dir).collect();
    assert_eq!(found, vec!["crates/cli", "crates/core"]);
}

#[test]
fn the_apps_packages_convention_is_the_fallback_when_nothing_is_declared() {
    let s = Scratch::new();
    s.file("apps/web/package.json", "{}")
        .file("packages/ui/package.json", "{}")
        .file("packages/notes/README.md", ""); // no manifest → skipped
    let found: Vec<String> = detect(s.root()).into_iter().map(|d| d.rel_dir).collect();
    assert_eq!(found, vec!["apps/web", "packages/ui"]);
}

/// A declared workspace is taken at its word: the convention scan must not add
/// packages the repo deliberately left out of its workspace.
#[test]
fn a_declared_workspace_suppresses_the_convention_fallback() {
    let s = Scratch::new();
    s.file("pnpm-workspace.yaml", "packages:\n  - 'apps/*'\n")
        .file("apps/web/package.json", "{}")
        .file("packages/legacy/package.json", "{}"); // present but not in workspace
    let found: Vec<String> = detect(s.root()).into_iter().map(|d| d.rel_dir).collect();
    assert_eq!(found, vec!["apps/web"], "packages/legacy stays out");
}

#[test]
fn node_modules_and_dotdirs_are_never_treated_as_packages() {
    let s = Scratch::new();
    s.file("apps/web/package.json", "{}")
        .file("apps/node_modules/dep/package.json", "{}")
        .file("apps/.cache/package.json", "{}");
    let found: Vec<String> = detect(s.root()).into_iter().map(|d| d.rel_dir).collect();
    assert_eq!(found, vec!["apps/web"]);
}

#[test]
fn colliding_package_names_get_distinct_aliases() {
    let s = Scratch::new();
    s.file("apps/ui/package.json", "{}")
        .file("packages/ui/package.json", "{}");
    let aliases: Vec<String> = detect(s.root()).into_iter().map(|d| d.alias).collect();
    assert_eq!(aliases, vec!["ui", "ui-2"], "the second `ui` is disambiguated");
}

#[test]
fn the_route_glob_is_scan_root_relative() {
    let d = Detected {
        alias: "web".into(),
        name: "web".into(),
        rel_dir: "apps/web".into(),
        via: "test",
    };
    assert_eq!(d.route_glob(), "apps/web/**");
}

// ── Planning ─────────────────────────────────────────────────────────────────

fn detected(rel: &str) -> Detected {
    let name = rel.rsplit('/').next().unwrap().to_string();
    Detected {
        alias: slugify(&name).unwrap(),
        name,
        rel_dir: rel.into(),
        via: "test",
    }
}

#[test]
fn a_name_that_matches_a_live_project_defaults_to_selecting_it() {
    let rows = build_plan(
        vec![detected("apps/web")],
        &[project("p_web", "web"), project("p_api", "api")],
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].matched.as_ref().unwrap().id, "p_web");
    assert_eq!(rows[0].action, Action::Select("p_web".into()));
}

#[test]
fn matching_ignores_case_and_surrounding_space() {
    let rows = build_plan(vec![detected("apps/Web")], &[project("p_web", "  web ")]);
    assert_eq!(rows[0].action, Action::Select("p_web".into()));
}

#[test]
fn an_unmatched_name_defaults_to_create() {
    let rows = build_plan(vec![detected("apps/new")], &[project("p_web", "web")]);
    assert_eq!(rows[0].action, Action::Create);
    assert!(rows[0].matched.is_none());
}

/// An archived project is a poor migration target: it is shown as no match, so
/// the row defaults to creating a fresh project rather than reviving a dead one.
#[test]
fn an_archived_project_is_not_matched() {
    let mut archived = project("p_old", "web");
    archived.archived_at = Some("2026-01-01T00:00:00Z".into());
    let rows = build_plan(vec![detected("apps/web")], &[archived]);
    assert_eq!(rows[0].action, Action::Create);
}

// ── The config file ──────────────────────────────────────────────────────────

fn resolved_row(rel: &str, action: Action, resolved: Option<&str>) -> PlanRow {
    PlanRow {
        detected: detected(rel),
        matched: None,
        action,
        resolved_project_id: resolved.map(str::to_string),
    }
}

#[test]
fn the_config_carries_only_resolved_non_skipped_rows() {
    let rows = vec![
        resolved_row("apps/web", Action::Create, Some("p_web")),
        resolved_row("apps/api", Action::Skip, None),
        resolved_row("packages/ui", Action::Select("p_ui".into()), Some("p_ui")),
        resolved_row("packages/pending", Action::Create, None), // never resolved
    ];
    let config = build_config("Acme Monorepo", Some("client_acme"), &rows);
    let aliases: Vec<&String> = config.projects.keys().collect();
    assert_eq!(aliases, vec!["ui", "web"], "skipped and unresolved are dropped");
    assert_eq!(config.repository.id, "acme-monorepo", "the repo id is slugged");
    assert_eq!(config.projects["web"].project_id, "p_web");
    assert_eq!(config.projects["web"].paths, vec!["apps/web/**"]);
    assert_eq!(config.projects["web"].client_id.as_deref(), Some("client_acme"));
}

#[test]
fn a_run_without_a_client_writes_no_client_id() {
    let rows = vec![resolved_row("apps/web", Action::Create, Some("p_web"))];
    let config = build_config("repo", None, &rows);
    assert_eq!(config.projects["web"].client_id, None);
    let yaml = to_yaml(&config).unwrap();
    assert!(!yaml.contains("client_id"), "an internal run names no client: {yaml}");
}

/// The bytes written must parse back into the same structure — the guard that
/// keeps this minimal mirror serializing what the runner can read.
#[test]
fn the_config_round_trips_through_yaml() {
    let rows = vec![
        resolved_row("apps/web", Action::Create, Some("p_web")),
        resolved_row("packages/ui", Action::Select("p_ui".into()), Some("p_ui")),
    ];
    let config = build_config("acme", Some("client_acme"), &rows);
    let yaml = to_yaml(&config).unwrap();
    let parsed: RepoConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(parsed, config);
    // Shape the runner requires: a version, a repository id, a non-empty
    // projects map keyed by alias.
    assert!(yaml.contains("version: 1"));
    assert!(yaml.contains("id: acme"));
    assert!(yaml.contains("apps/web/**"));
}

#[test]
fn read_existing_returns_none_for_a_missing_or_unreadable_file() {
    let s = Scratch::new();
    assert!(read_existing(s.root()).is_none(), "no file");
    s.file(".nexusmind.yaml", "this: [is not, valid: config");
    assert!(read_existing(s.root()).is_none(), "unparseable is treated as absent");
}

#[test]
fn read_existing_parses_a_config_the_tui_itself_wrote() {
    let s = Scratch::new();
    let rows = vec![resolved_row("apps/web", Action::Create, Some("p_web"))];
    let yaml = to_yaml(&build_config("acme", None, &rows)).unwrap();
    s.file(".nexusmind.yaml", &yaml);
    let back = read_existing(s.root()).unwrap();
    assert_eq!(back.projects["web"].project_id, "p_web");
}
