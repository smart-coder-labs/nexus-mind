use super::*;
use tempfile::TempDir;

fn opts(dir: &TempDir) -> ScanOptions {
    ScanOptions {
        root: dir.path().to_string_lossy().to_string(),
        ..Default::default()
    }
}

fn write(dir: &TempDir, rel: &str, content: &str) {
    let p = dir.path().join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

fn connector() -> SourceCodeConnector {
    SourceCodeConnector::new("testrepo")
}

fn paths(items: &[SourceItem]) -> Vec<String> {
    items
        .iter()
        .map(|i| i.meta["path"].as_str().unwrap().to_string())
        .collect()
}

/// The walker admits only real source files: docs and config/data are not code
/// and must never become code-knowledge units.
#[test]
fn scans_code_files_and_ignores_docs_and_config() {
    let dir = TempDir::new().unwrap();
    write(&dir, "src/app.rs", "pub fn main() { let x = 1; }\n");
    write(&dir, "README.md", "# not code\n");
    write(&dir, "package.json", "{}\n");
    let items = connector().scan(&opts(&dir)).unwrap();
    assert_eq!(paths(&items), vec!["src/app.rs"], "only code becomes a unit");
    assert_eq!(items[0].routing_path.as_deref(), Some("src/app.rs"));
    assert!(items[0].source_identity.starts_with("source-code:testrepo:src/app.rs#file:"));
}

/// A small file is one whole-file unit — the common case the design is built on.
#[test]
fn a_small_file_is_a_single_whole_file_unit() {
    let dir = TempDir::new().unwrap();
    write(&dir, "a.rs", "pub fn a() {}\n");
    let items = connector().scan(&opts(&dir)).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].raw.contains("pub fn a()"));
}

/// A file past the size cap is split into windows, each with its own line
/// anchor and identity so a later edit re-proposes one window, not the file.
#[test]
fn a_large_file_is_split_into_windowed_units() {
    let dir = TempDir::new().unwrap();
    let big = "let value = compute_something_long();\n".repeat(1200); // ~45 KB
    assert!(big.len() > MAX_UNIT_BYTES);
    write(&dir, "big.rs", &big);
    let items = connector().scan(&opts(&dir)).unwrap();
    assert!(items.len() > 1, "a large file yields multiple units");
    assert!(
        items.iter().all(|i| i.routing_path.as_deref() == Some("big.rs")),
        "every window still routes to the same file"
    );
    assert!(
        items.iter().all(|i| i.meta.get("start_line").is_some()),
        "each window carries its start line"
    );
    // Distinct identities so an edit to one window does not touch the others.
    let ids: std::collections::BTreeSet<_> =
        items.iter().map(|i| i.source_identity.clone()).collect();
    assert_eq!(ids.len(), items.len(), "identities are per-window");
}

#[test]
fn includes_and_excludes_narrow_the_scan() {
    let dir = TempDir::new().unwrap();
    write(&dir, "src/app.rs", "pub fn a() {}\n");
    write(&dir, "generated/gen.rs", "pub fn g() {}\n");

    let excluded = connector()
        .scan(&ScanOptions {
            excludes: vec!["generated".into()],
            ..opts(&dir)
        })
        .unwrap();
    assert_eq!(paths(&excluded), vec!["src/app.rs"], "excluded subpath is dropped");

    let included = connector()
        .scan(&ScanOptions {
            includes: vec!["generated".into()],
            ..opts(&dir)
        })
        .unwrap();
    assert_eq!(paths(&included), vec!["generated/gen.rs"], "only the included subpath");
}

#[test]
fn an_empty_file_is_reported_as_excluded_not_a_unit() {
    let dir = TempDir::new().unwrap();
    write(&dir, "empty.rs", "   \n");
    write(&dir, "real.rs", "pub fn r() {}\n");
    let report = connector().scan_report(&opts(&dir)).unwrap();
    assert_eq!(report.units, 1);
    assert!(report.excluded.iter().any(|(p, r)| p == "empty.rs" && r.contains("empty")));
}

#[test]
fn the_prompt_asks_for_conventions_and_a_verbatim_excerpt() {
    let dir = TempDir::new().unwrap();
    write(&dir, "src/app.rs", "pub fn a() {}\n");
    let items = connector().scan(&opts(&dir)).unwrap();
    let prompt = connector().classify_prompt(&items[0]);
    assert!(prompt.contains("src/app.rs"), "names the file");
    assert!(prompt.contains("rust"), "names the language");
    assert!(prompt.contains("convention"), "asks for conventions");
    assert!(prompt.contains("VERBATIM"), "demands a verbatim excerpt");
    assert!(prompt.contains("not the code"), "content is the knowledge, not the code");
}

/// Code carries no deterministic candidate — the knowledge is exactly what needs
/// a model — so `--no-llm` produces nothing rather than a fabricated convention.
#[test]
fn there_is_no_deterministic_fallback() {
    let dir = TempDir::new().unwrap();
    write(&dir, "src/app.rs", "pub fn a() {}\n");
    let items = connector().scan(&opts(&dir)).unwrap();
    assert!(connector().fallback(&items[0]).is_none());
}

#[test]
fn scan_report_counts_files_units_and_bytes() {
    let dir = TempDir::new().unwrap();
    write(&dir, "a.rs", "pub fn a() {}\n");
    write(&dir, "b.rs", "pub fn b() {}\n");
    let report = connector().scan_report(&opts(&dir)).unwrap();
    assert_eq!(report.documents, 2, "two files read");
    assert_eq!(report.units, 2);
    assert!(report.bytes > 0);
}
