use super::*;
use std::fs;
use std::path::Path;
use std::process::Command;

const VALID: &[u8] =
    include_bytes!("../../../../schemas/fixtures/nexusmind-config/v1/valid/multi-project.yaml");

fn snapshot() -> ConfigSnapshot {
    ConfigSnapshot::from_bytes(VALID, PathBuf::from("/repo"), ".nexusmind.yaml".into()).unwrap()
}

#[test]
fn parses_version_one_and_resolves_effective_profile() {
    let snap = snapshot();
    assert_eq!(snap.config.repository.id, "ecommerce-platform");
    assert_eq!(snap.config.projects.len(), 3);
    let caps = snap.config.effective_capabilities("readonly").unwrap();
    assert!(caps.contains("memory.read"));
    assert!(!caps.contains("memory.write"));
    assert!(!caps.contains("task.write"));
}

#[test]
fn rejects_unknown_version_with_stable_code() {
    let bytes = include_bytes!(
        "../../../../schemas/fixtures/nexusmind-config/v1/invalid/unknown-version.yaml"
    );
    assert_eq!(parse(bytes).unwrap_err().code, "CONFIG_UNSUPPORTED_VERSION");
}

#[test]
fn snapshot_hashes_exact_bytes_and_attestation_hides_root() {
    let snap = snapshot();
    let changed = ConfigSnapshot::from_bytes(
        &[VALID, b"\n"].concat(),
        PathBuf::from("/Users/private/repo"),
        ".nexusmind.yaml".into(),
    )
    .unwrap();
    assert_ne!(snap.sha256, changed.sha256);
    let json = serde_json::to_string(&changed.attestation()).unwrap();
    assert!(!json.contains("/Users/private"));
    assert!(json.contains("ecommerce-platform"));
}

#[test]
fn routes_more_specific_projects_and_root_project() {
    let resolver = ProjectResolver::compile(snapshot()).unwrap();
    let payment = resolver
        .resolve(Path::new("services/payments/docs/adr.md"))
        .unwrap();
    let store = resolver
        .resolve(Path::new("apps/storefront/README.md"))
        .unwrap();
    let root = resolver.resolve(Path::new("docs/platform.md")).unwrap();
    assert!(
        matches!(payment, ResolutionStatus::Resolved(ResolvedProject { ref alias, .. }) if alias == "payments")
    );
    assert!(
        matches!(store, ResolutionStatus::Resolved(ResolvedProject { ref alias, .. }) if alias == "storefront")
    );
    assert!(
        matches!(root, ResolutionStatus::Resolved(ResolvedProject { ref alias, .. }) if alias == "platform")
    );
}

#[test]
fn equal_specificity_across_projects_is_ambiguous() {
    let bytes = br#"
version: 1
repository: { id: sample }
projects:
  one: { project_id: p1, paths: ["src/**"] }
  two: { project_id: p2, paths: ["src/**"] }
"#;
    let resolver = ProjectResolver::compile(
        ConfigSnapshot::from_bytes(bytes, PathBuf::from("/repo"), ".nexusmind.yaml".into())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        resolver.resolve(Path::new("src/lib.rs")).unwrap_err().code,
        "ROUTING_AMBIGUOUS"
    );
}

#[test]
fn no_match_without_default_is_unmapped() {
    let bytes = br#"
version: 1
repository: { id: sample }
projects:
  one: { project_id: p1, paths: ["src/**"] }
"#;
    let resolver = ProjectResolver::compile(
        ConfigSnapshot::from_bytes(bytes, PathBuf::from("/repo"), ".nexusmind.yaml".into())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        resolver.resolve(Path::new("docs/a.md")).unwrap(),
        ResolutionStatus::Unmapped
    );
}

#[test]
fn discovers_from_nested_path_and_stops_at_git_root() {
    let temp = tempfile::tempdir().unwrap();
    Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(temp.path())
        .status()
        .unwrap();
    fs::write(temp.path().join(".nexusmind.yaml"), VALID).unwrap();
    let nested = temp.path().join("services/payments");
    fs::create_dir_all(&nested).unwrap();
    let snap = load(ConfigSelection::DiscoverFrom(nested), true)
        .unwrap()
        .unwrap();
    assert_eq!(snap.relative_path, ".nexusmind.yaml");
}

#[test]
fn explicit_config_outside_root_is_rejected() {
    let repo = tempfile::tempdir().unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    let err = load(
        ConfigSelection::Explicit {
            config: outside.path().into(),
            repository_root: repo.path().into(),
        },
        true,
    )
    .unwrap_err();
    assert_eq!(err.code, "CONFIG_OUTSIDE_REPOSITORY");
}

#[test]
fn rejects_secret_fields_without_echoing_values() {
    let bytes =
        b"version: 1\napi_key: super-secret-value\nrepository: { id: sample }\nprojects: {}\n";
    let err = parse(bytes).unwrap_err();
    assert_eq!(err.code, "CONFIG_SECRET_FIELD");
    assert!(!err.to_string().contains("super-secret-value"));
}

#[test]
fn rejects_yaml_aliases_and_profile_cycles() {
    let alias = b"version: 1\nrepository: &repo { id: sample }\nprojects: {}\n";
    assert_eq!(parse(alias).unwrap_err().code, "CONFIG_YAML_ALIAS");
    let cycle = br#"
version: 1
repository: { id: sample }
projects:
  sample: { project_id: p1, paths: ["**"] }
agents:
  profiles:
    one: { extends: two, capabilities: [] }
    two: { extends: one, capabilities: [] }
"#;
    assert_eq!(parse(cycle).unwrap_err().code, "CONFIG_PROFILE_CYCLE");
}

#[test]
fn explicit_project_override_wins_and_client_requires_project() {
    let resolver = ProjectResolver::compile(snapshot()).unwrap();
    let resolved = resolver
        .resolve_with_override(
            Path::new("services/payments/a.md"),
            &DestinationOverride {
                project_id: Some("prj-manual".into()),
                client_id: Some("client-manual".into()),
            },
        )
        .unwrap();
    assert!(
        matches!(resolved, ResolutionStatus::Resolved(ResolvedProject { ref project_id, basis: ResolutionBasis::ExplicitOverride, .. }) if project_id == "prj-manual")
    );
    let err = resolver
        .resolve_with_override(
            Path::new("a.md"),
            &DestinationOverride {
                project_id: None,
                client_id: Some("client-only".into()),
            },
        )
        .unwrap_err();
    assert_eq!(err.code, "ROUTING_OVERRIDE_INVALID");
}

#[test]
fn inventory_plan_groups_projects_and_keeps_pathless_items_on_default() {
    let resolver = ProjectResolver::compile(snapshot()).unwrap();
    let plan = resolver
        .plan_paths(
            [
                Some("services/payments/a.md"),
                Some("apps/storefront/a.md"),
                Some("services/payments/b.md"),
                None,
            ],
            &DestinationOverride::default(),
        )
        .unwrap();
    assert_eq!(plan.groups.len(), 3);
    assert!(plan.unmapped_indices.is_empty());
    let payments = plan
        .groups
        .iter()
        .find(|g| g.destination.alias == "payments")
        .unwrap();
    assert_eq!(payments.item_indices, vec![0, 2]);
    let platform = plan
        .groups
        .iter()
        .find(|g| g.destination.alias == "platform")
        .unwrap();
    assert_eq!(platform.item_indices, vec![3]);
}

#[test]
fn inventory_plan_collects_unmapped_without_spending_or_writing() {
    let bytes = br#"
version: 1
repository: { id: sample }
projects:
  one: { project_id: p1, paths: ["src/**"] }
"#;
    let resolver = ProjectResolver::compile(
        ConfigSnapshot::from_bytes(bytes, PathBuf::from("/repo"), ".nexusmind.yaml".into())
            .unwrap(),
    )
    .unwrap();
    let plan = resolver
        .plan_paths(
            [Some("src/a.rs"), Some("docs/a.md"), None],
            &DestinationOverride::default(),
        )
        .unwrap();
    assert_eq!(plan.groups[0].item_indices, vec![0]);
    assert_eq!(plan.unmapped_indices, vec![1, 2]);
}

/// The exact `.nexusmind.yaml` the migrator TUI writes must be accepted by this
/// (stricter) parser and route every path to the right project. Captured
/// verbatim from `migrator-tui`'s `monorepo::to_yaml`, so a drift between the
/// TUI's config shape and what the runner accepts fails here rather than in a
/// live monorepo run.
const TUI_GENERATED: &[u8] = b"version: 1
repository:
  id: acme-monorepo
defaults:
  project: repo
projects:
  repo:
    project_id: proj_root_id
    client_id: client_acme
    paths:
    - '**'
  web:
    project_id: proj_web_id
    client_id: client_acme
    paths:
    - apps/web/**
";

#[test]
fn the_tui_generated_config_parses_and_routes_to_the_right_projects() {
    let snap = ConfigSnapshot::from_bytes(
        TUI_GENERATED,
        PathBuf::from("/repo"),
        ".nexusmind.yaml".into(),
    )
    .expect("the runner must accept the TUI's config");
    assert_eq!(snap.config.repository.id, "acme-monorepo");
    assert_eq!(snap.config.defaults.project.as_deref(), Some("repo"));

    let resolver = ProjectResolver::compile(snap).unwrap();

    // A package file routes to its package (the more specific glob wins).
    let web = resolver
        .resolve(Path::new("apps/web/src/index.ts"))
        .unwrap();
    assert!(
        matches!(web, ResolutionStatus::Resolved(ResolvedProject { ref alias, ref project_id, .. }) if alias == "web" && project_id == "proj_web_id"),
        "package file must route to the package: {web:?}"
    );

    // A root-level doc — the case that used to fail with ROUTING_UNMAPPED —
    // routes to the repository catch-all, not nowhere.
    let doc = resolver.resolve(Path::new("docs/verification.md")).unwrap();
    assert!(
        matches!(doc, ResolutionStatus::Resolved(ResolvedProject { ref alias, ref project_id, .. }) if alias == "repo" && project_id == "proj_root_id"),
        "root-level knowledge must route to the repo catch-all: {doc:?}"
    );

    // Nothing is unmapped, so the run cannot abort with ROUTING_UNMAPPED.
    let plan = resolver
        .plan_paths(
            [Some("docs/verification.md"), Some("apps/web/src/index.ts"), Some("README.md")]
                .into_iter(),
            &DestinationOverride::default(),
        )
        .unwrap();
    assert!(plan.unmapped_indices.is_empty(), "nothing may be unmapped");
}
