//! The UI specialist's execution checks (factory F4, ADR 0dfbde60): the changed
//! frontend package installs, builds (which type-checks), lints and passes its
//! tests, and the routes the change touches are screenshotted for the review.
//!
//! Everything that runs repository code runs in ONE sandbox commands pod, never
//! in the worker, as for the tests specialist (`automation::specialist_tests`,
//! whose lockfile detection and `sh -c` positional-argument pattern this
//! reuses). The worker plans the pod (package, scripts, server, fixtures,
//! routes, shots) and ships a small Node script (`specialist_ui_shoot.js`) and
//! its configuration with `CommandsSpec::files`:
//!
//! 1. install per lockfile; `typecheck` and `build` scripts (a build pass
//!    leaves a marker); `lint`; `test` (or the package's runner). Missing
//!    scripts are skipped, and the skip is an advisory finding;
//! 2. with the marker, the script serves the build (`vite preview` or `next
//!    start`), opens each planned page in the image's bundled Chromium with
//!    every API call answered from `.factory/ui-fixtures.json` and every other
//!    off-origin request blocked, and prints each screenshot as a framed base64
//!    line on stdout, the only channel out of a commands pod.
//!
//! The worker decodes and bounds those images (the pod is untrusted) and hands
//! them to the review step in a scratch directory of its checkout.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::specialist_tests::{self as tests, PackageManager, IN_DIR};
use crate::automation::merge_gate::ChangedFile;
use crate::factory::mutation::SourceTree;
use crate::factory::sandbox_exec::CommandRun;
use crate::factory::specialists::{self, CheckOutcome, MAX_UI_ROUTES};

/// Where a package keeps its fixture file, relative to the package.
pub const FIXTURE_FILE: &str = ".factory/ui-fixtures.json";
/// Largest fixture file read.
const MAX_FIXTURE_BYTES: usize = 1_048_576;
const MAX_API_BASES: usize = 8;
const MAX_FIXTURES: usize = 200;
const MAX_FIXTURE_JSON_BYTES: usize = 256 * 1024;
const MAX_ROUTE_HINTS: usize = 100;
const MAX_LOCAL_STORAGE: usize = 20;

/// Most images per change: enough for three routes at two viewports, small
/// enough for one review step to open every one of them.
pub const MAX_SHOTS: usize = 6;
/// Largest image the worker accepts; the script falls back to JPEG to fit.
pub const MAX_IMAGE_BYTES: usize = 850_000;
/// The screenshot command's stdout: every image base64-encoded plus the result.
const SHOTS_STDOUT: usize = 8 * 1_048_576;
/// Tallest page captured, in CSS pixels, so a long list stays one readable image.
const MAX_CAPTURE_HEIGHT: u32 = 2_400;
/// The script's own budget, under the pod's per-command limit so it always
/// reports what it has.
const SHOTS_BUDGET_MS: u64 = 170_000;
const SERVE_TIMEOUT_MS: u64 = 60_000;
/// The preview server's port inside the pod (the pod is the run's alone).
const PORT: u16 = 4791;

const BUILT_MARKER: &str = "/tmp/nm-ui/built";
const SCRIPT_PATH: &str = "/tmp/nm-ui/shoot.js";
const CONFIG_PATH: &str = "/tmp/nm-ui/config.json";
const SERVER_LOG: &str = "/tmp/nm-ui/server.log";
const SCRIPT: &str = include_str!("specialist_ui_shoot.js");
/// The sandbox image installs Playwright globally (npm's prefix is
/// `/usr/local` at build time; a commands pod's `HOME` is `/tmp`, so `npm root
/// -g` there would name another prefix) and its browsers in `/ms-playwright`.
const NODE_PATH: &str = "NODE_PATH=/usr/local/lib/node_modules:/usr/lib/node_modules";
const BROWSERS_PATH: &str = "PLAYWRIGHT_BROWSERS_PATH=/ms-playwright";

/// Runs a package script and leaves a marker when it passes.
const RUN_MARKED: &str = r#"marker=$1; cd "$2" || exit 96; shift 2; "$@" && : > "$marker""#;
/// Runs only after the build passed (exit 99 otherwise).
const AFTER_BUILD: &str = r#"marker=$1; cd "$2" || exit 96; shift 2; [ -e "$marker" ] || exit 99; exec "$@""#;

// ------------------------------------------------------------------ fixtures

/// `.factory/ui-fixtures.json`, as documented in `docs/factory/ui-fixtures.md`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UiFixtures {
    #[serde(default = "version_one")]
    pub version: u32,
    #[serde(default, skip_serializing)]
    pub description: Option<String>,
    /// Where the app's API lives: a same-origin path prefix (`/v1/`) or an
    /// absolute URL prefix (`https://api.example.com/v1/`).
    #[serde(deserialize_with = "one_or_many")]
    pub api_base: Vec<String>,
    #[serde(default)]
    pub fixtures: Vec<Fixture>,
    /// Source glob (relative to the package) to the route that shows it.
    #[serde(default)]
    pub route_hints: BTreeMap<String, String>,
    /// Routes screenshotted when the change names none.
    #[serde(default = "root_route")]
    pub default_routes: Vec<String>,
    #[serde(default = "light_only")]
    pub themes: Vec<String>,
    #[serde(default)]
    pub theme_storage: Option<ThemeStorage>,
    #[serde(default)]
    pub local_storage: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    #[serde(default, skip_serializing)]
    pub description: Option<String>,
    #[serde(default = "get")]
    pub method: String,
    /// The URL path; a trailing `*` makes it a prefix.
    pub path: String,
    /// Query parameters that must be present with these values.
    #[serde(default)]
    pub query: BTreeMap<String, String>,
    #[serde(default = "ok")]
    pub status: u16,
    #[serde(default = "empty_object")]
    pub json: Value,
}

/// A theme the app reads from `localStorage` rather than `prefers-color-scheme`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThemeStorage {
    pub key: String,
    pub light: String,
    pub dark: String,
}

fn version_one() -> u32 {
    1
}
fn root_route() -> Vec<String> {
    vec!["/".into()]
}
fn light_only() -> Vec<String> {
    vec!["light".into()]
}
fn get() -> String {
    "GET".into()
}
fn ok() -> u16 {
    200
}
fn empty_object() -> Value {
    json!({})
}

fn one_or_many<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(value) => vec![value],
        OneOrMany::Many(values) => values,
    })
}

const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

fn plain(text: &str, max: usize) -> bool {
    !text.is_empty() && text.len() <= max && !text.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Parses and checks a fixture file. Every problem is named, so a maintainer can
/// fix the file from the advisory alone.
pub fn parse_fixtures(raw: &str) -> Result<UiFixtures, String> {
    if raw.len() > MAX_FIXTURE_BYTES {
        return Err(format!("larger than {MAX_FIXTURE_BYTES} bytes"));
    }
    let mut fixtures: UiFixtures = serde_json::from_str(raw).map_err(|error| format!("not valid: {error}"))?;
    if fixtures.version != 1 {
        return Err(format!("unknown version {}", fixtures.version));
    }
    if fixtures.api_base.is_empty() || fixtures.api_base.len() > MAX_API_BASES {
        return Err(format!("`api_base` needs 1 to {MAX_API_BASES} entries"));
    }
    for base in &fixtures.api_base {
        let absolute = base
            .strip_prefix("https://")
            .or_else(|| base.strip_prefix("http://"))
            .is_some_and(|rest| rest.split('/').next().is_some_and(|host| !host.is_empty()));
        if !plain(base, 200) || !(absolute || (base.starts_with('/') && !base.starts_with("//"))) {
            return Err(format!("`api_base` entry `{base}` is neither a path prefix (`/v1/`) nor an http(s) URL"));
        }
    }
    if fixtures.fixtures.len() > MAX_FIXTURES {
        return Err(format!("more than {MAX_FIXTURES} fixtures"));
    }
    for fixture in &mut fixtures.fixtures {
        fixture.method = fixture.method.to_ascii_uppercase();
        let path = fixture.path.as_str();
        if !METHODS.contains(&fixture.method.as_str()) {
            return Err(format!("fixture `{path}`: unknown method `{}`", fixture.method));
        }
        let star_inside = path.trim_end_matches('*').contains('*') || path.ends_with("**");
        if !plain(path, 300) || !path.starts_with('/') || star_inside || path.contains('?') {
            return Err(format!("fixture path `{path}` must be a URL path (no query), with `*` only at its end"));
        }
        if !(100..=599).contains(&fixture.status) {
            return Err(format!("fixture `{path}`: status {} out of range", fixture.status));
        }
        if serde_json::to_string(&fixture.json).map_or(0, |text| text.len()) > MAX_FIXTURE_JSON_BYTES {
            return Err(format!("fixture `{path}`: `json` larger than {MAX_FIXTURE_JSON_BYTES} bytes"));
        }
    }
    if fixtures.route_hints.len() > MAX_ROUTE_HINTS {
        return Err(format!("more than {MAX_ROUTE_HINTS} route hints"));
    }
    for (pattern, route) in &fixtures.route_hints {
        if pattern.starts_with('/') || pattern.contains("..") || globset::Glob::new(pattern).is_err() {
            return Err(format!("route hint `{pattern}` is not a glob relative to the package"));
        }
        if !specialists::valid_route(route) {
            return Err(format!("route hint `{pattern}`: `{route}` is not an app route"));
        }
    }
    if fixtures.default_routes.len() > MAX_UI_ROUTES || !fixtures.default_routes.iter().all(|r| specialists::valid_route(r)) {
        return Err(format!("`default_routes` needs at most {MAX_UI_ROUTES} app routes"));
    }
    let themes: HashSet<&str> = fixtures.themes.iter().map(String::as_str).collect();
    let known = fixtures.themes.iter().all(|theme| theme == "light" || theme == "dark");
    if !known || themes.len() != fixtures.themes.len() || !themes.contains("light") {
        return Err("`themes` must be [\"light\"] or [\"light\", \"dark\"]".into());
    }
    // Light first: when the image cap bites, every route keeps its light shots.
    fixtures.themes.sort_by_key(|theme| theme != "light");
    if let Some(storage) = &fixtures.theme_storage {
        if storage.key.is_empty() || [&storage.key, &storage.light, &storage.dark].iter().any(|v| v.len() > 100) {
            return Err("`theme_storage` needs a key and values of at most 100 characters".into());
        }
    }
    if fixtures.local_storage.len() > MAX_LOCAL_STORAGE
        || fixtures.local_storage.iter().any(|(key, value)| key.is_empty() || key.len() > 200 || value.len() > 2_000)
    {
        return Err(format!("`local_storage` takes at most {MAX_LOCAL_STORAGE} short entries"));
    }
    Ok(fixtures)
}

/// The routes to screenshot: the routes the change names (the plan, or an eval
/// task) first, then the routes whose `route_hints` glob matches a changed
/// file; the fixture's `default_routes` when there are none. At most
/// [`MAX_UI_ROUTES`].
pub fn select_routes(fixtures: &UiFixtures, package_dir: &str, changed: &[String], named: &[String]) -> Vec<String> {
    let mut routes: Vec<String> = Vec::new();
    let add = |route: &str, routes: &mut Vec<String>| {
        if specialists::valid_route(route) && !routes.iter().any(|known| known == route) && routes.len() < MAX_UI_ROUTES {
            routes.push(route.to_string());
        }
    };
    for route in named {
        add(route, &mut routes);
    }
    let relative: Vec<&str> = changed
        .iter()
        .filter_map(|path| if package_dir.is_empty() { Some(path.as_str()) } else { path.strip_prefix(&format!("{package_dir}/")) })
        .collect();
    for (pattern, route) in &fixtures.route_hints {
        let Ok(glob) = globset::GlobBuilder::new(pattern).literal_separator(true).build() else { continue };
        let matcher = glob.compile_matcher();
        if relative.iter().any(|path| matcher.is_match(path)) {
            add(route, &mut routes);
        }
    }
    if routes.is_empty() {
        for route in &fixtures.default_routes {
            add(route, &mut routes);
        }
    }
    routes
}

// --------------------------------------------------------------------- shots

/// The two viewports every route is captured at.
pub const VIEWPORTS: &[(&str, u32, u32)] = &[("mobile", 390, 844), ("desktop", 1280, 800)];

/// One planned screenshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Shot {
    /// File stem: `r<route index>-<viewport>-<theme>`.
    pub name: String,
    pub route: String,
    pub viewport: &'static str,
    pub width: u32,
    pub height: u32,
    pub theme: String,
}

/// Every route at both viewports in the light theme, then in the dark one,
/// stopping at [`MAX_SHOTS`]: the light shots of every route come first.
pub fn plan_shots(routes: &[String], themes: &[String]) -> Vec<Shot> {
    let mut shots = Vec::new();
    for theme in themes {
        for (index, route) in routes.iter().enumerate() {
            for (viewport, width, height) in VIEWPORTS {
                if shots.len() < MAX_SHOTS {
                    shots.push(Shot {
                        name: format!("r{index}-{viewport}-{theme}"),
                        route: route.clone(),
                        viewport,
                        width: *width,
                        height: *height,
                        theme: theme.clone(),
                    });
                }
            }
        }
    }
    shots
}

/// A screenshot brought back from the pod, checked.
#[derive(Clone, PartialEq, Eq)]
pub struct Screenshot {
    /// File name with its extension (`r0-mobile-light.png`).
    pub file: String,
    pub route: String,
    pub viewport: &'static str,
    pub theme: String,
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for Screenshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Screenshot({}, {} bytes)", self.file, self.bytes.len())
    }
}

impl Screenshot {
    /// How the review step finds and reads this image.
    pub fn describe(&self) -> String {
        let (label, width, height) = VIEWPORTS
            .iter()
            .find(|(name, _, _)| *name == self.viewport)
            .map(|(name, w, h)| (if *name == "mobile" { "phone" } else { "desktop" }, *w, *h))
            .unwrap_or(("unknown", 0, 0));
        format!("`{REVIEW_DIR}/{}` (route `{}`, {label} {width}x{height}, {} theme)", self.file, self.route, self.theme)
    }
}

/// The script's result line.
#[derive(Debug, Default, Deserialize)]
struct ScriptResult {
    #[serde(default)]
    served: bool,
    #[serde(default)]
    serve_detail: Option<String>,
    #[serde(default)]
    fatal: Option<String>,
    #[serde(default)]
    pages: Vec<PageResult>,
    #[serde(default)]
    skipped: Vec<Skipped>,
}

#[derive(Debug, Deserialize)]
struct PageResult {
    name: String,
    route: String,
    status: String,
    #[serde(default)]
    final_path: Option<String>,
    #[serde(default)]
    errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Skipped {
    name: String,
    reason: String,
}

const SHOT_PREFIX: &str = "NMUI-SHOT ";
const RESULT_PREFIX: &str = "NMUI-RESULT ";

fn image_kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else {
        None
    }
}

/// Parses the screenshot command's stdout. The pod is untrusted: only planned
/// names, real PNG or JPEG content under the size cap, and at most
/// [`MAX_SHOTS`] images are kept; the result is the last result line.
fn decode_output(stdout: &[u8], shots: &[Shot]) -> (Vec<Screenshot>, Option<ScriptResult>) {
    use base64::Engine;
    let mut images: Vec<Screenshot> = Vec::new();
    let mut result = None;
    for line in stdout.split(|b| *b == b'\n') {
        let Ok(line) = std::str::from_utf8(line) else { continue };
        let line = line.trim_end_matches('\r');
        if let Some(json) = line.strip_prefix(RESULT_PREFIX) {
            result = serde_json::from_str::<ScriptResult>(json).ok();
            continue;
        }
        let Some((file, encoded)) = line.strip_prefix(SHOT_PREFIX).and_then(|rest| rest.split_once(' ')) else {
            continue;
        };
        let Some((stem, extension)) = file.rsplit_once('.') else { continue };
        let Some(shot) = shots.iter().find(|shot| shot.name == stem) else { continue };
        if images.len() >= MAX_SHOTS || images.iter().any(|image| image.file.starts_with(&format!("{stem}."))) {
            continue;
        }
        if encoded.len() > MAX_IMAGE_BYTES / 3 * 4 + 4 {
            continue;
        }
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) else { continue };
        if bytes.len() > MAX_IMAGE_BYTES || image_kind(&bytes) != Some(extension) {
            continue;
        }
        images.push(Screenshot {
            file: format!("{stem}.{extension}"),
            route: shot.route.clone(),
            viewport: shot.viewport,
            theme: shot.theme.clone(),
            bytes,
        });
    }
    (images, result)
}

// ---------------------------------------------------------------------- plan

/// How the app is served after its build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Server {
    Vite,
    Next,
}

fn depends(package_json: &Value, name: &str) -> bool {
    ["dependencies", "devDependencies"]
        .iter()
        .any(|key| package_json.get(key).and_then(|deps| deps.get(name)).is_some())
}

/// The server a package's build is served with: Next.js when it depends on
/// `next`, else Vite when it depends on `vite`.
pub fn server_of(package_json: &Value) -> Option<Server> {
    if depends(package_json, "next") {
        Some(Server::Next)
    } else if depends(package_json, "vite") {
        Some(Server::Vite)
    } else {
        None
    }
}

impl Server {
    fn argv(self, manager: PackageManager) -> Vec<String> {
        let mut argv: Vec<String> = manager.exec().iter().map(|s| s.to_string()).collect();
        let port = PORT.to_string();
        match self {
            Server::Vite => argv.extend(["vite", "preview", "--port", &port, "--strictPort", "--host", "127.0.0.1"].map(String::from)),
            Server::Next => argv.extend(["next", "start", "-p", &port, "-H", "127.0.0.1"].map(String::from)),
        }
        argv
    }
}

fn has_script(package_json: &Value, name: &str) -> bool {
    package_json.pointer(&format!("/scripts/{name}")).and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PodStep {
    Install(String),
    Build(String),
    Lint(String),
    Tests(String),
    Screenshots,
}

/// The commands and files of the checks pod, and what each command is.
#[derive(Debug, Default)]
pub struct UiPlan {
    pub package_dir: String,
    pub commands: Vec<Vec<String>>,
    pub files: Vec<(String, Vec<u8>)>,
    steps: Vec<PodStep>,
    /// Checks whose script the package lacks: advisory findings.
    skipped: Vec<(&'static str, String)>,
    pub shots: Vec<Shot>,
    /// Why there are no screenshots, when there are none (advisory).
    pub no_screenshots: Option<String>,
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

/// Everything the worker decides before the pod. `Err` is a reason the package
/// cannot be built at all, which fails `ui_build`.
pub fn prepare(tree: &dyn SourceTree, changed: &[String], named_routes: &[String]) -> Result<UiPlan, String> {
    let first = changed.first().ok_or("ui_no_change")?;
    let package_dir = tests::ancestors(crate::factory::mutation::parent(first))
        .into_iter()
        .find(|dir| tree.exists(&tests::join(dir, "package.json")))
        .ok_or_else(|| format!("ui_package_not_found: no package.json above {first}"))?;
    let (install_dir, manager) = tests::locate_install(tree, &package_dir)
        .ok_or_else(|| format!("ui_lockfile_not_found: no lockfile for {}", tests::display_dir(&package_dir)))?;
    let package_json = tests::read_json(tree, &tests::join(&package_dir, "package.json"))
        .ok_or_else(|| format!("ui_package_json_unreadable: {}", tests::join(&package_dir, "package.json")))?;
    let mut plan = UiPlan { package_dir: package_dir.clone(), ..Default::default() };
    let dir = tests::pod_path(&package_dir);
    let run_script = |name: &str| -> Vec<String> {
        let mut argv: Vec<String> = manager.run_script().iter().map(|s| s.to_string()).collect();
        argv.push(name.to_string());
        argv
    };
    let in_dir = |argv: Vec<String>| {
        let mut command = strings(&["sh", "-c", IN_DIR, "sh"]);
        command.push(dir.clone());
        command.extend(argv);
        command
    };

    let mut install = strings(&["sh", "-c", IN_DIR, "sh"]);
    install.push(tests::pod_path(&install_dir));
    install.extend(manager.install().iter().map(|s| s.to_string()));
    plan.commands.push(install);
    plan.steps.push(PodStep::Install(format!(
        "install in {} ({})",
        tests::display_dir(&install_dir),
        manager.install().join(" ")
    )));
    if has_script(&package_json, "typecheck") {
        plan.commands.push(in_dir(run_script("typecheck")));
        plan.steps.push(PodStep::Build("typecheck script".into()));
    }
    let builds = has_script(&package_json, "build");
    if builds {
        let mut command = strings(&["sh", "-c", RUN_MARKED, "sh", BUILT_MARKER]);
        command.push(dir.clone());
        command.extend(run_script("build"));
        plan.commands.push(command);
        plan.steps.push(PodStep::Build("build script".into()));
    } else {
        plan.skipped.push(("ui_build", "skipped: no `build` script".into()));
    }
    if has_script(&package_json, "lint") {
        plan.commands.push(in_dir(run_script("lint")));
        plan.steps.push(PodStep::Lint("lint script".into()));
    } else {
        plan.skipped.push(("ui_lint", "skipped: no `lint` script".into()));
    }
    if has_script(&package_json, "test") {
        plan.commands.push(in_dir(run_script("test")));
        plan.steps.push(PodStep::Tests("test script".into()));
    } else if let Some(runner) = tests::runner_of(&package_json) {
        let mut argv: Vec<String> = manager.exec().iter().map(|s| s.to_string()).collect();
        argv.extend(match runner {
            tests::Runner::Vitest => strings(&["vitest", "run"]),
            tests::Runner::Jest => strings(&["jest"]),
        });
        plan.commands.push(in_dir(argv));
        plan.steps.push(PodStep::Tests(format!("{runner:?} (no test script)").to_lowercase()));
    } else {
        plan.skipped.push(("ui_tests", "skipped: no `test` script and no vitest or jest".into()));
    }

    let fixture_path = tests::join(&package_dir, FIXTURE_FILE);
    let fixtures = match tree.read(&fixture_path).map(|raw| parse_fixtures(&raw)) {
        None => {
            plan.no_screenshots =
                Some(format!("not run: no fixture file `{fixture_path}` (see docs/factory/ui-fixtures.md)"));
            return Ok(plan);
        }
        Some(Err(problem)) => {
            plan.no_screenshots = Some(format!("not run: fixture file `{fixture_path}` {problem}"));
            return Ok(plan);
        }
        Some(Ok(fixtures)) => fixtures,
    };
    let Some(server) = server_of(&package_json) else {
        plan.no_screenshots = Some("not run: unsupported app (only Vite and Next.js builds are served)".into());
        return Ok(plan);
    };
    if !builds {
        plan.no_screenshots = Some("not run: no `build` script, so there is nothing to serve".into());
        return Ok(plan);
    }
    let routes = select_routes(&fixtures, &package_dir, changed, named_routes);
    plan.shots = plan_shots(&routes, &fixtures.themes);
    if plan.shots.is_empty() {
        plan.no_screenshots = Some("not run: no route named, hinted or defaulted".into());
        return Ok(plan);
    }
    let config = json!({
        "port": PORT,
        "server": server.argv(manager),
        "server_log": SERVER_LOG,
        "fixtures": fixtures,
        "shots": plan.shots,
        "budget_ms": SHOTS_BUDGET_MS,
        "serve_timeout_ms": SERVE_TIMEOUT_MS,
        "max_image_bytes": MAX_IMAGE_BYTES,
        "max_height": MAX_CAPTURE_HEIGHT,
    });
    plan.files.push((SCRIPT_PATH.to_string(), SCRIPT.as_bytes().to_vec()));
    plan.files.push((CONFIG_PATH.to_string(), config.to_string().into_bytes()));
    let mut command = strings(&["sh", "-c", AFTER_BUILD, "sh", BUILT_MARKER]);
    command.push(dir);
    command.extend(strings(&["env", NODE_PATH, BROWSERS_PATH, "node", SCRIPT_PATH, CONFIG_PATH]));
    plan.commands.push(command);
    plan.steps.push(PodStep::Screenshots);
    Ok(plan)
}

// ----------------------------------------------------------------- interpret

const MAX_DETAILS: usize = 20;

fn outcome(name: &'static str, passed: bool, advisory: bool, details: Vec<String>) -> CheckOutcome {
    CheckOutcome { name, passed, advisory, details: details.into_iter().take(MAX_DETAILS).collect() }
}

/// The checks a change whose package cannot be planned reports.
pub fn not_runnable(reason: String) -> Vec<CheckOutcome> {
    vec![outcome("ui_build", false, false, vec![reason])]
}

fn failure(what: &str, run: &CommandRun) -> Option<String> {
    match run.exit_code {
        Some(0) => None,
        Some(127) => Some(format!("{what}: command not found (exit 127)")),
        Some(code) => Some(format!("{what} exited {code}: {}", tests::tail(run))),
        None => Some(format!("{what}: no exit status")),
    }
}

/// Turns the pod's runs into `ui_build`, `ui_lint`, `ui_tests` and
/// `ui_screenshots`, and the checked images.
pub fn interpret(plan: &UiPlan, runs: &[CommandRun]) -> (Vec<CheckOutcome>, Vec<Screenshot>) {
    let mut build = Vec::new();
    let mut lint = Vec::new();
    let mut tests_failures = Vec::new();
    let mut shots_run = None;
    for (step, run) in plan.steps.iter().zip(runs) {
        match step {
            PodStep::Install(what) | PodStep::Build(what) => build.extend(failure(what, run)),
            PodStep::Lint(what) => lint.extend(failure(what, run)),
            PodStep::Tests(what) => tests_failures.extend(failure(what, run)),
            PodStep::Screenshots => shots_run = Some(run),
        }
    }
    let skipped = |name: &str| plan.skipped.iter().find(|(check, _)| *check == name).map(|(_, why)| why.clone());
    let script_check = |name: &'static str, failures: Vec<String>| match (failures.is_empty(), skipped(name)) {
        (true, Some(why)) => outcome(name, false, true, vec![why]),
        (passed, _) => outcome(name, passed, false, failures),
    };
    let mut checks = vec![
        script_check("ui_build", build.clone()),
        script_check("ui_lint", lint),
        script_check("ui_tests", tests_failures),
    ];
    let (screenshots, images) = match (&plan.no_screenshots, shots_run) {
        (Some(why), _) => (outcome("ui_screenshots", false, true, vec![why.clone()]), Vec::new()),
        (None, None) => (outcome("ui_screenshots", false, false, vec!["the screenshot command did not run".into()]), Vec::new()),
        (None, Some(run)) => judge_screenshots(plan, run, !build.is_empty()),
    };
    checks.push(screenshots);
    (checks, images)
}

/// ADVISORY: nothing to judge the change by (the app could not be served, a
/// route could not be loaded or redirected, an image is missing). BLOCKING:
/// the change broke a page (an uncaught error, a blank body), or the script
/// itself failed after the fixtures existed.
fn judge_screenshots(plan: &UiPlan, run: &CommandRun, build_failed: bool) -> (CheckOutcome, Vec<Screenshot>) {
    if run.exit_code == Some(99) || build_failed {
        return (outcome("ui_screenshots", false, true, vec!["not run: the build did not pass".into()]), Vec::new());
    }
    let (images, result) = decode_output(&run.stdout, &plan.shots);
    let crashed = |detail: String| (outcome("ui_screenshots", false, false, vec![detail]), Vec::new());
    let Some(result) = result.filter(|_| run.exit_code == Some(0) && !run.stdout_truncated) else {
        let code = run.exit_code.map_or("none".to_string(), |code| code.to_string());
        let stderr = String::from_utf8_lossy(&run.stderr);
        let tail: String = stderr.trim().chars().rev().take(400).collect::<Vec<_>>().into_iter().rev().collect();
        return crashed(format!("the screenshot script failed (exit {code}): {}", tail.replace('\n', " | ")));
    };
    if let Some(fatal) = result.fatal {
        return crashed(format!("the screenshot script failed: {fatal}"));
    }
    if !result.served {
        let detail = result.serve_detail.unwrap_or_default();
        return (outcome("ui_screenshots", false, true, vec![format!("the app could not be served: {detail}")]), Vec::new());
    }
    let mut blocking = Vec::new();
    let mut advisory = Vec::new();
    let label = |name: &str| {
        plan.shots
            .iter()
            .find(|shot| shot.name == name)
            .map_or(name.to_string(), |shot| format!("{} ({}, {})", shot.route, shot.viewport, shot.theme))
    };
    for page in &result.pages {
        let errors = page.errors.join("; ");
        match page.status.as_str() {
            "ok" => {}
            "page_error" => blocking.push(format!("{}: uncaught error: {errors}", label(&page.name))),
            "blank" => blocking.push(format!("{}: the page rendered a blank body", label(&page.name))),
            "http_error" => blocking.push(format!("{}: the app answered {errors}", label(&page.name))),
            _ => advisory.push(format!("{}: could not be loaded: {errors}", label(&page.name))),
        }
        if let Some(path) = page.final_path.as_deref() {
            let requested = page.route.split('?').next().unwrap_or(&page.route);
            if path != requested {
                advisory.push(format!("{}: redirected to {path}", label(&page.name)));
            }
        }
    }
    for skipped in &result.skipped {
        advisory.push(format!("{}: skipped ({})", label(&skipped.name), skipped.reason));
    }
    for shot in &plan.shots {
        let reported = result.pages.iter().any(|page| page.name == shot.name)
            || result.skipped.iter().any(|skipped| skipped.name == shot.name);
        let returned = images.iter().any(|image| image.file.starts_with(&format!("{}.", shot.name)));
        if !reported {
            advisory.push(format!("{}: not captured", label(&shot.name)));
        } else if !returned && result.pages.iter().any(|page| page.name == shot.name && !["unreachable", "http_error"].contains(&page.status.as_str())) {
            advisory.push(format!("{}: no valid image returned", label(&shot.name)));
        }
    }
    advisory.dedup();
    let check = if !blocking.is_empty() {
        blocking.extend(advisory);
        outcome("ui_screenshots", false, false, blocking)
    } else if !advisory.is_empty() {
        outcome("ui_screenshots", false, true, advisory)
    } else {
        outcome("ui_screenshots", true, false, Vec::new())
    };
    (check, images)
}

// -------------------------------------------------------------------- review

/// The scratch directory the screenshots are written to for a review or eval
/// judge step. It is written only after the change's files and diff were taken
/// and removed as soon as the step ends, so it never reaches the checks, the
/// diff or a pull request; a stale copy (or one the agent made) is replaced.
pub const REVIEW_DIR: &str = ".nm-ui-review";

/// The screenshots in a checkout's scratch directory for one step; dropping it
/// removes them, whatever way the step ends.
pub(crate) struct ReviewShots {
    dir: Option<PathBuf>,
    pub descriptions: Vec<String>,
}

impl ReviewShots {
    pub fn none() -> Self {
        Self { dir: None, descriptions: Vec::new() }
    }

    pub fn write(workdir: &Path, screenshots: &[Screenshot]) -> std::io::Result<Self> {
        if screenshots.is_empty() {
            return Ok(Self::none());
        }
        let dir = workdir.join(REVIEW_DIR);
        if std::fs::symlink_metadata(&dir).is_ok() {
            if dir.is_dir() && !dir.is_symlink() {
                std::fs::remove_dir_all(&dir)?;
            } else {
                std::fs::remove_file(&dir)?;
            }
        }
        std::fs::create_dir(&dir)?;
        let shots = Self { dir: Some(dir.clone()), descriptions: screenshots.iter().map(Screenshot::describe).collect() };
        for screenshot in screenshots {
            std::fs::write(dir.join(&screenshot.file), &screenshot.bytes)?;
        }
        Ok(shots)
    }
}

impl Drop for ReviewShots {
    fn drop(&mut self) {
        if let Some(dir) = self.dir.take() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

// ----------------------------------------------------------------------- run

/// Runs the UI checks for a change that passed the path checks. `Err` is a
/// tenant-visible code: the checks could not run (no sandbox, a pod failure).
pub(crate) async fn run_checks(
    env: &super::specialist_run::StepEnv<'_>,
    changed: &[ChangedFile],
    named_routes: &[String],
) -> Result<(Vec<CheckOutcome>, Vec<Screenshot>), String> {
    // Fail closed: the worker never runs repository code.
    if !env.sandboxed {
        return Err("ui_specialist_requires_sandbox".into());
    }
    let mut paths: Vec<String> = changed
        .iter()
        .filter(|file| file.status != "removed" && file.filename != "PENDING.md")
        .map(|file| file.filename.clone())
        .collect();
    paths.sort();
    let root = env.workdir.to_path_buf();
    let named = named_routes.to_vec();
    let planned = tokio::task::spawn_blocking(move || {
        let tree = tests::DiskTree::new(&root).map_err(|_| "checkout_unreadable".to_string())?;
        prepare(&tree, &paths, &named)
    })
    .await
    .map_err(|_| "specialist_checks_failed_to_run".to_string())?;
    let plan = match planned {
        Ok(plan) => plan,
        Err(reason) => return Ok((not_runnable(reason), Vec::new())),
    };
    let receipts = std::sync::Mutex::new(Vec::new());
    let slot = format!("{}-ui", env.slot);
    let run = super::sandboxed::SandboxedRun {
        store: env.store,
        org_id: env.org_id,
        run_id: env.run_id,
        attempt_id: env.attempt_id,
        retry: 0,
        workdir: env.workdir,
        secret_values: env.secret_values,
        seq_base: 0,
        wall_time: Duration::ZERO,
        verification: &[],
        receipts: &receipts,
        qa: None,
        slot: &slot,
        writes: None,
    };
    // corepack must never wait on a download prompt.
    let corepack = [("COREPACK_ENABLE_DOWNLOAD_PROMPT".to_string(), "0".to_string())];
    let spec = super::sandboxed::CommandsSpec {
        commands: &plan.commands,
        extra_env: &corepack,
        timeout_secs: crate::factory::verification::COMMAND_TIMEOUT_SECS,
        reproduce_failures: false,
        hosts: &[],
        label: "u",
        max_stdout: SHOTS_STDOUT,
        files: plan.files.clone(),
        proxy_file: None,
    };
    let runs = super::sandboxed::run_commands_sandboxed(&run, &spec)
        .await
        .map_err(|error| super::sandboxed::failure_code(env.run_id, &error))?;
    if runs.len() != plan.commands.len() {
        return Err("ui_checks_incomplete".into());
    }
    Ok(interpret(&plan, &runs))
}

#[cfg(test)]
mod tests_ui {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Tree(HashMap<String, String>);

    impl Tree {
        fn with(mut self, path: &str, content: &str) -> Self {
            self.0.insert(path.into(), content.into());
            self
        }
    }

    impl SourceTree for Tree {
        fn read(&self, path: &str) -> Option<String> {
            self.0.get(path).cloned()
        }
        fn exists(&self, path: &str) -> bool {
            self.0.contains_key(path)
        }
    }

    fn paths(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    const ADMIN_PACKAGE: &str = r#"{"scripts":{"dev":"vite","build":"tsc -b && vite build","preview":"vite preview","test":"vitest run"},"devDependencies":{"vite":"5","vitest":"2"}}"#;
    const FIXTURES: &str = r#"{
        "api_base": "/v1/",
        "fixtures": [
            {"path": "/v1/admin/auth/me", "json": {"user": {"name": "Ada Example"}}},
            {"method": "post", "path": "/v1/tasks/*", "status": 201}
        ],
        "route_hints": {"src/pages/Tasks.tsx": "/tasks", "src/components/**": "/", "src/pages/Users*.tsx": "/users"},
        "themes": ["dark", "light"]
    }"#;

    fn admin() -> Tree {
        Tree::default()
            .with("apps/admin/package.json", ADMIN_PACKAGE)
            .with("apps/admin/package-lock.json", "{}")
            .with("apps/admin/pnpm-lock.yaml", "")
            .with("apps/admin/.factory/ui-fixtures.json", FIXTURES)
    }

    #[test]
    fn fixture_files_parse_with_defaults_and_name_every_problem() {
        let fixtures = parse_fixtures(FIXTURES).unwrap();
        assert_eq!(fixtures.api_base, ["/v1/"]);
        assert_eq!(fixtures.fixtures[0].method, "GET");
        assert_eq!((fixtures.fixtures[1].method.as_str(), fixtures.fixtures[1].status), ("POST", 201));
        assert_eq!(fixtures.fixtures[1].json, json!({}));
        assert_eq!(fixtures.default_routes, ["/"]);
        assert_eq!(fixtures.themes, ["light", "dark"], "light always comes first");
        let minimal = parse_fixtures(r#"{"api_base": ["/api/", "https://api.example.com/v2/"]}"#).unwrap();
        assert_eq!((minimal.themes.as_slice(), minimal.fixtures.len()), (&["light".to_string()][..], 0));
        for (raw, problem) in [
            ("{", "not valid"),
            (r#"{"fixtures": []}"#, "not valid"),
            (r#"{"api_base": "/v1/", "fixturez": []}"#, "not valid"),
            (r#"{"version": 2, "api_base": "/v1/"}"#, "unknown version"),
            (r#"{"api_base": []}"#, "`api_base` needs"),
            (r#"{"api_base": "v1"}"#, "neither a path prefix"),
            (r#"{"api_base": "//evil.example/"}"#, "neither a path prefix"),
            (r#"{"api_base": "ftp://x/"}"#, "neither a path prefix"),
            (r#"{"api_base": "/v1/", "fixtures": [{"path": "v1/x"}]}"#, "must be a URL path"),
            (r#"{"api_base": "/v1/", "fixtures": [{"path": "/v1/*/x"}]}"#, "must be a URL path"),
            (r#"{"api_base": "/v1/", "fixtures": [{"path": "/v1/x?a=1"}]}"#, "must be a URL path"),
            (r#"{"api_base": "/v1/", "fixtures": [{"path": "/v1/x", "method": "FETCH"}]}"#, "unknown method"),
            (r#"{"api_base": "/v1/", "fixtures": [{"path": "/v1/x", "status": 99}]}"#, "out of range"),
            (r#"{"api_base": "/v1/", "route_hints": {"/abs/*.tsx": "/"}}"#, "not a glob relative"),
            (r#"{"api_base": "/v1/", "route_hints": {"src/a.tsx": "tasks"}}"#, "not an app route"),
            (r#"{"api_base": "/v1/", "default_routes": ["/a", "/b", "/c", "/d"]}"#, "`default_routes`"),
            (r#"{"api_base": "/v1/", "themes": ["dark"]}"#, "`themes`"),
            (r#"{"api_base": "/v1/", "themes": ["light", "sepia"]}"#, "`themes`"),
            (r#"{"api_base": "/v1/", "themes": ["light", "light"]}"#, "`themes`"),
            (r#"{"api_base": "/v1/", "theme_storage": {"key": "", "light": "l", "dark": "d"}}"#, "`theme_storage`"),
        ] {
            let error = parse_fixtures(raw).unwrap_err();
            assert!(error.contains(problem), "{raw}: {error}");
        }
        let huge = format!(r#"{{"api_base": "/v1/", "description": "{}"}}"#, "x".repeat(MAX_FIXTURE_BYTES));
        assert!(parse_fixtures(&huge).unwrap_err().contains("larger than"));
    }

    #[test]
    fn routes_come_from_the_plan_then_the_hints_then_the_defaults_capped_at_three() {
        let fixtures = parse_fixtures(FIXTURES).unwrap();
        let changed = paths(&["apps/admin/src/pages/Tasks.tsx"]);
        assert_eq!(select_routes(&fixtures, "apps/admin", &changed, &[]), ["/tasks"]);
        let named = paths(&["/memories", "/tasks", "not-a-route", "//evil.example"]);
        assert_eq!(select_routes(&fixtures, "apps/admin", &changed, &named), ["/memories", "/tasks"]);
        // `**` crosses directories, `*` does not.
        let nested = paths(&["apps/admin/src/components/ui/Badge.tsx", "apps/admin/src/pages/UsersTable.tsx", "apps/admin/src/pages/a/Users.tsx"]);
        assert_eq!(select_routes(&fixtures, "apps/admin", &nested, &[]), ["/", "/users"]);
        let many = paths(&["/a", "/b", "/c", "/d"]);
        assert_eq!(select_routes(&fixtures, "apps/admin", &changed, &many), ["/a", "/b", "/c"]);
        // Nothing named or hinted: the defaults.
        assert_eq!(select_routes(&fixtures, "apps/admin", &paths(&["apps/admin/src/lib/x.ts"]), &[]), ["/"]);
        let none = UiFixtures { default_routes: vec![], ..fixtures };
        assert!(select_routes(&none, "apps/admin", &paths(&["apps/admin/src/lib/x.ts"]), &[]).is_empty());
    }

    #[test]
    fn shots_cover_light_first_and_stop_at_six() {
        let light = plan_shots(&paths(&["/", "/tasks"]), &paths(&["light"]));
        let names: Vec<&str> = light.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["r0-mobile-light", "r0-desktop-light", "r1-mobile-light", "r1-desktop-light"]);
        assert_eq!((light[0].width, light[0].height, light[1].width), (390, 844, 1280));
        let both = plan_shots(&paths(&["/", "/tasks"]), &paths(&["light", "dark"]));
        assert_eq!(both.len(), MAX_SHOTS);
        assert_eq!(both[4].name, "r0-mobile-dark");
        let three = plan_shots(&paths(&["/", "/a", "/b"]), &paths(&["light", "dark"]));
        assert!(three.iter().all(|shot| shot.theme == "light"), "every route keeps its light shots");
    }

    #[test]
    fn the_pod_installs_builds_lints_tests_then_screenshots_after_the_build() {
        let changed = paths(&["apps/admin/src/pages/Tasks.tsx"]);
        let plan = prepare(&admin(), &changed, &[]).unwrap();
        // install (npm wins over pnpm), build, test, screenshots; no lint script.
        assert_eq!(plan.commands.len(), 4);
        assert_eq!(&plan.commands[0][4..], ["/workspace/apps/admin", "npm", "ci", "--no-audit", "--no-fund"]);
        assert_eq!(&plan.commands[1][4..], [BUILT_MARKER, "/workspace/apps/admin", "npm", "run", "build"]);
        assert_eq!(&plan.commands[2][4..], ["/workspace/apps/admin", "npm", "run", "test"]);
        let shots = &plan.commands[3];
        assert_eq!(shots[2], AFTER_BUILD);
        assert_eq!(&shots[4..6], [BUILT_MARKER, "/workspace/apps/admin"]);
        assert_eq!(&shots[6..], ["env", NODE_PATH, BROWSERS_PATH, "node", SCRIPT_PATH, CONFIG_PATH]);
        // Nothing from the repository is spliced into a script.
        assert!(plan.commands.iter().all(|argv| !argv[2].contains("apps/admin")));
        let config: Value = serde_json::from_slice(&plan.files.iter().find(|(p, _)| p == CONFIG_PATH).unwrap().1).unwrap();
        assert_eq!(config["server"], json!(["npx", "--no", "vite", "preview", "--port", "4791", "--strictPort", "--host", "127.0.0.1"]));
        assert_eq!(config["fixtures"]["fixtures"][1]["method"], "POST");
        assert_eq!(config["shots"].as_array().unwrap().len(), 4, "/tasks at two viewports, light and dark");
        assert!(plan.files.iter().any(|(p, c)| p == SCRIPT_PATH && c.starts_with(b"'use strict'")));
        assert_eq!(plan.skipped, [("ui_lint", "skipped: no `lint` script".to_string())]);
    }

    #[test]
    fn scripts_servers_and_fixtures_decide_what_runs() {
        let pnpm_next = Tree::default()
            .with("web/package.json", r#"{"scripts":{"build":"next build","lint":"next lint","typecheck":"tsc"},"dependencies":{"next":"15"},"devDependencies":{"jest":"29"}}"#)
            .with("pnpm-lock.yaml", "")
            .with("web/.factory/ui-fixtures.json", r#"{"api_base":"https://api.example.com/"}"#);
        let plan = prepare(&pnpm_next, &paths(&["web/src/app/page.tsx"]), &[]).unwrap();
        let tails: Vec<Vec<String>> = plan.commands.iter().map(|c| c[4..].to_vec()).collect();
        assert_eq!(tails[0], ["/workspace", "corepack", "pnpm", "install", "--frozen-lockfile"]);
        assert_eq!(tails[1], ["/workspace/web", "corepack", "pnpm", "run", "typecheck"]);
        assert_eq!(tails[3], ["/workspace/web", "corepack", "pnpm", "run", "lint"]);
        assert_eq!(tails[4], ["/workspace/web", "corepack", "pnpm", "exec", "jest"], "the runner without a test script");
        let config: Value = serde_json::from_slice(&plan.files[1].1).unwrap();
        assert_eq!(config["server"], json!(["corepack", "pnpm", "exec", "next", "start", "-p", "4791", "-H", "127.0.0.1"]));
        assert_eq!(config["shots"][0]["route"], "/", "the default route");

        let no_fixture = Tree::default().with("package.json", ADMIN_PACKAGE).with("yarn.lock", "");
        let plan = prepare(&no_fixture, &paths(&["src/App.tsx"]), &[]).unwrap();
        assert!(plan.no_screenshots.as_deref().unwrap().contains("no fixture file `.factory/ui-fixtures.json`"));
        assert!(plan.files.is_empty() && plan.commands.len() == 3);

        let broken = admin().with("apps/admin/.factory/ui-fixtures.json", r#"{"api_base": "v1"}"#);
        let plan = prepare(&broken, &paths(&["apps/admin/src/App.tsx"]), &[]).unwrap();
        assert!(plan.no_screenshots.as_deref().unwrap().contains("neither a path prefix"));

        let webpack = admin().with("apps/admin/package.json", r#"{"scripts":{"build":"webpack"},"devDependencies":{"webpack":"5"}}"#);
        let plan = prepare(&webpack, &paths(&["apps/admin/src/App.tsx"]), &[]).unwrap();
        assert!(plan.no_screenshots.as_deref().unwrap().contains("unsupported app"));
        assert_eq!(plan.skipped.len(), 2, "no lint, no test script or runner");

        let unbuilt = admin().with("apps/admin/package.json", r#"{"devDependencies":{"vite":"5"}}"#);
        let plan = prepare(&unbuilt, &paths(&["apps/admin/src/App.tsx"]), &[]).unwrap();
        assert!(plan.no_screenshots.as_deref().unwrap().contains("no `build` script"));

        assert!(prepare(&Tree::default(), &paths(&["src/a.tsx"]), &[]).unwrap_err().starts_with("ui_package_not_found"));
        let unlocked = Tree::default().with("package.json", ADMIN_PACKAGE);
        assert!(prepare(&unlocked, &paths(&["src/a.tsx"]), &[]).unwrap_err().starts_with("ui_lockfile_not_found"));
        assert!(not_runnable("ui_lockfile_not_found: x".into()).iter().all(|c| !c.passed && !c.advisory));
    }

    fn run(exit_code: Option<i32>) -> CommandRun {
        CommandRun { exit_code, stdout: b"error TS2322".to_vec(), ..Default::default() }
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest-of-image";

    fn shot_line(file: &str, bytes: &[u8]) -> String {
        use base64::Engine;
        format!("{SHOT_PREFIX}{file} {}\n", base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    fn shots_run(result: Value, images: &[(&str, &[u8])]) -> CommandRun {
        let mut stdout = String::from("vite preview noise\n");
        for (file, bytes) in images {
            stdout.push_str(&shot_line(file, bytes));
        }
        stdout.push_str(&format!("{RESULT_PREFIX}{result}\n"));
        CommandRun { exit_code: Some(0), stdout: stdout.into_bytes(), ..Default::default() }
    }

    fn by_name<'a>(checks: &'a [CheckOutcome], name: &str) -> &'a CheckOutcome {
        checks.iter().find(|c| c.name == name).unwrap()
    }

    fn admin_plan() -> UiPlan {
        let tree = admin().with("apps/admin/.factory/ui-fixtures.json", r#"{"api_base":"/v1/","route_hints":{"src/pages/Tasks.tsx":"/tasks"}}"#);
        prepare(&tree, &paths(&["apps/admin/src/pages/Tasks.tsx"]), &[]).unwrap()
    }

    fn page(name: &str, status: &str) -> Value {
        json!({"name": name, "route": "/tasks", "status": status, "final_path": "/tasks", "errors": if status == "page_error" { json!(["x is undefined"]) } else { json!([]) }})
    }

    #[test]
    fn screenshots_are_framed_decoded_and_bounded() {
        let plan = admin_plan();
        let ok = json!({"served": true, "pages": [page("r0-mobile-light", "ok"), page("r0-desktop-light", "ok")]});
        let jpeg: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 1, 2];
        let runs = [run(Some(0)), run(Some(0)), run(Some(0)), shots_run(ok.clone(), &[("r0-mobile-light.png", PNG), ("r0-desktop-light.jpg", jpeg)])];
        let (checks, images) = interpret(&plan, &runs);
        assert!(by_name(&checks, "ui_screenshots").passed, "{checks:?}");
        assert_eq!(images.len(), 2);
        assert_eq!((images[0].file.as_str(), images[0].route.as_str(), images[0].viewport), ("r0-mobile-light.png", "/tasks", "mobile"));
        assert_eq!(images[0].bytes, PNG);
        assert_eq!(images[0].describe(), "`.nm-ui-review/r0-mobile-light.png` (route `/tasks`, phone 390x844, light theme)");

        // Unplanned names, wrong content, a mismatched extension, duplicates and
        // oversized images are dropped.
        let big = [PNG, &vec![0u8; MAX_IMAGE_BYTES]].concat();
        let hostile = [
            ("../../etc/passwd.png", PNG),
            ("r9-mobile-light.png", PNG),
            ("r0-mobile-light.png", b"<svg>".as_slice()),
            ("r0-mobile-light.jpg", PNG),
            ("r0-desktop-light.png", big.as_slice()),
            ("r0-mobile-light.png", PNG),
            ("r0-mobile-light.png", PNG),
        ];
        let (images, _) = decode_output(&shots_run(ok, &hostile).stdout, &plan.shots);
        assert_eq!(images.len(), 1, "{images:?}");
        assert_eq!(images[0].file, "r0-mobile-light.png");
    }

    #[test]
    fn advisory_versus_blocking_screenshot_findings() {
        let plan = admin_plan();
        let builds = || vec![run(Some(0)), run(Some(0)), run(Some(0))];
        let check_of = |shots: CommandRun| {
            let mut runs = builds();
            runs.push(shots);
            let (checks, _) = interpret(&plan, &runs);
            by_name(&checks, "ui_screenshots").clone()
        };
        let both = [("r0-mobile-light.png", PNG), ("r0-desktop-light.png", PNG)];
        // A page that throws or renders nothing blocks.
        let thrown = check_of(shots_run(json!({"served": true, "pages": [page("r0-mobile-light", "page_error"), page("r0-desktop-light", "ok")]}), &both));
        assert!(!thrown.passed && !thrown.advisory);
        assert_eq!(thrown.details[0], "/tasks (mobile, light): uncaught error: x is undefined");
        let blank = check_of(shots_run(json!({"served": true, "pages": [page("r0-mobile-light", "blank"), page("r0-desktop-light", "ok")]}), &both));
        assert!(!blank.passed && !blank.advisory);
        // The app answering an HTTP error blocks: the change may have broken the route.
        let errored = check_of(shots_run(json!({"served": true, "pages": [page("r0-mobile-light", "http_error"), page("r0-desktop-light", "ok")]}), &both[1..]));
        assert!(!errored.passed && !errored.advisory, "{errored:?}");
        assert!(errored.details.iter().all(|d| !d.contains("no valid image")), "{errored:?}");
        // The script failing is blocking; so is a fatal (no browser).
        let crashed = check_of(CommandRun { exit_code: Some(1), stderr: b"TypeError: boom".to_vec(), ..Default::default() });
        assert!(!crashed.passed && !crashed.advisory && crashed.details[0].contains("TypeError: boom"), "{crashed:?}");
        let fatal = check_of(shots_run(json!({"fatal": "playwright_unavailable: Cannot find module"}), &[]));
        assert!(!fatal.passed && !fatal.advisory);
        let truncated = CommandRun { stdout_truncated: true, ..shots_run(json!({"served": true}), &[]) };
        assert!(!check_of(truncated).advisory);
        // Not served, unreachable, redirected or missing images: advisory.
        let unserved = check_of(shots_run(json!({"served": false, "serve_detail": "port in use"}), &[]));
        assert!(unserved.advisory && unserved.details[0].contains("port in use"));
        let unreachable = check_of(shots_run(json!({"served": true, "pages": [page("r0-mobile-light", "unreachable"), page("r0-desktop-light", "ok")]}), &both[1..]));
        assert!(!unreachable.passed && unreachable.advisory, "{unreachable:?}");
        let mut redirected = page("r0-mobile-light", "ok");
        redirected["final_path"] = json!("/login");
        let login = check_of(shots_run(json!({"served": true, "pages": [redirected, page("r0-desktop-light", "ok")]}), &both));
        assert!(login.advisory && login.details == ["/tasks (mobile, light): redirected to /login"]);
        let missing = check_of(shots_run(json!({"served": true, "pages": [page("r0-mobile-light", "ok"), page("r0-desktop-light", "ok")]}), &both[..1]));
        assert!(missing.advisory && missing.details == ["/tasks (desktop, light): no valid image returned"]);
        let skipped = check_of(shots_run(json!({"served": true, "pages": [page("r0-mobile-light", "ok")], "skipped": [{"name": "r0-desktop-light", "reason": "time budget spent"}]}), &both[..1]));
        assert!(skipped.advisory && skipped.details[0].contains("time budget spent"));
        let report = specialists::CheckReport::from_checks(vec![skipped]);
        assert!(report.passed, "an advisory never fails the report");
    }

    #[test]
    fn build_lint_and_test_failures_block_and_missing_scripts_are_advisory() {
        let plan = admin_plan();
        let (checks, images) = interpret(&plan, &[run(Some(0)), run(Some(2)), run(Some(0)), run(Some(99))]);
        let build = by_name(&checks, "ui_build");
        assert!(!build.passed && !build.advisory && build.details[0].starts_with("build script exited 2: error TS2322"));
        assert_eq!(by_name(&checks, "ui_screenshots").details, ["not run: the build did not pass"]);
        assert!(by_name(&checks, "ui_screenshots").advisory && images.is_empty());
        let lint = by_name(&checks, "ui_lint");
        assert!(lint.advisory && lint.details == ["skipped: no `lint` script"]);
        let (checks, _) = interpret(&plan, &[run(Some(0)), run(Some(0)), run(Some(1)), run(Some(99))]);
        assert!(!by_name(&checks, "ui_tests").passed && !by_name(&checks, "ui_tests").advisory);
        let report = specialists::CheckReport::from_checks(checks);
        assert!(!report.passed);
        let no_fixture = UiPlan { no_screenshots: Some("not run: no fixture file".into()), ..admin_plan() };
        let (checks, _) = interpret(&no_fixture, &[run(Some(0)), run(Some(0)), run(Some(0))]);
        assert!(by_name(&checks, "ui_screenshots").advisory);
        assert!(specialists::CheckReport::from_checks(checks).passed);
    }

    #[test]
    fn review_screenshots_live_only_as_long_as_the_step() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(REVIEW_DIR)).unwrap();
        std::fs::write(dir.path().join(REVIEW_DIR).join("stale.tsx"), "left by the agent").unwrap();
        let shot = Screenshot { file: "r0-mobile-light.png".into(), route: "/".into(), viewport: "mobile", theme: "light".into(), bytes: PNG.to_vec() };
        {
            let written = ReviewShots::write(dir.path(), std::slice::from_ref(&shot)).unwrap();
            assert_eq!(std::fs::read(dir.path().join(REVIEW_DIR).join("r0-mobile-light.png")).unwrap(), PNG);
            assert!(!dir.path().join(REVIEW_DIR).join("stale.tsx").exists(), "a stale copy is replaced");
            assert_eq!(written.descriptions.len(), 1);
        }
        assert!(!dir.path().join(REVIEW_DIR).exists(), "dropping removes the scratch directory");
        let nothing = ReviewShots::write(dir.path(), &[]).unwrap();
        assert!(nothing.descriptions.is_empty() && !dir.path().join(REVIEW_DIR).exists());
    }

    #[test]
    fn the_admin_apps_fixture_file_is_valid() {
        let fixtures = parse_fixtures(include_str!("../../../admin/.factory/ui-fixtures.json")).unwrap();
        assert_eq!(fixtures.themes, ["light"], "the admin has one theme and no switch");
        let boot = fixtures.fixtures.iter().find(|f| f.path == "/v1/admin/auth/me").expect("the session fixture");
        assert_eq!(boot.json["user"]["role"], "admin", "renders as a logged-in admin");
        let changed = paths(&["apps/admin/src/pages/Tasks.tsx", "apps/admin/src/pages/Users.tsx"]);
        assert_eq!(select_routes(&fixtures, "apps/admin", &changed, &[]), ["/tasks", "/users"]);
        // Fake data only: example domains, no real names.
        let raw = include_str!("../../../admin/.factory/ui-fixtures.json");
        let emails: Vec<&str> = raw.split('"').filter(|s| s.contains('@')).collect();
        assert!(!emails.is_empty() && emails.iter().all(|e| e.ends_with("@example.test")), "{emails:?}");
    }

    #[test]
    fn the_server_follows_the_dependencies() {
        assert_eq!(server_of(&json!({"devDependencies": {"vite": "5"}})), Some(Server::Vite));
        assert_eq!(server_of(&json!({"dependencies": {"next": "15", "vite": "5"}})), Some(Server::Next));
        assert_eq!(server_of(&json!({"dependencies": {"react-scripts": "5"}})), None);
    }
}
