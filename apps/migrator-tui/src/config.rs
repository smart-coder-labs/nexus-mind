//! What a migration run is, and what has to be true before it may start.
//!
//! Every safety property the migration system establishes is enforced here, in
//! one place, as data — not scattered across the screens that happen to collect
//! the fields. A screen can only ever *display* what this module decides.

use std::fmt;

/// The connectors, in the order the operator meets them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    RepoDocs,
    ClaudeMemories,
    GitHistory,
    DbSchema,
}

impl Source {
    pub const ALL: [Source; 4] = [
        Source::RepoDocs,
        Source::ClaudeMemories,
        Source::GitHistory,
        Source::DbSchema,
    ];

    /// The `--source` value. Must match `connector_for` in the runner.
    pub fn flag(self) -> &'static str {
        match self {
            Source::RepoDocs => "repo-docs",
            Source::ClaudeMemories => "claude-memories",
            Source::GitHistory => "git-history",
            Source::DbSchema => "db-schema",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Source::RepoDocs => "Repository documents",
            Source::ClaudeMemories => "Claude assets",
            Source::GitHistory => "Git history",
            Source::DbSchema => "Database schema",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Source::RepoDocs => {
                "Markdown under the repo — ADRs, guides, SDD artifacts. Split into sections; \
                 each unchecked checklist item becomes its own task candidate."
            }
            Source::ClaudeMemories => {
                "CLAUDE.md, agents, skills, commands, hooks, output styles and plugins. \
                 Third-party assets under plugins/cache are never read, and neither are \
                 session transcripts."
            }
            Source::GitHistory => {
                "Substantive commits and merged PRs. Trailers are stripped before the body \
                 is measured, so a one-line commit with five trailers still reads as trivial."
            }
            Source::DbSchema => {
                "Tables, columns, constraints and relationships, grouped into areas. \
                 Row data is never read unless you explicitly unlock it below."
            }
        }
    }

    /// Whether `--path` names a directory the operator picks.
    pub fn takes_path(self) -> bool {
        !matches!(self, Source::DbSchema)
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.flag())
    }
}

/// Everything a run needs. One struct so the whole configuration can be shown,
/// validated and turned into a command line without any screen holding state of
/// its own.
#[derive(Debug, Clone)]
pub struct RunConfig {
    pub api_url: String,
    pub api_key: String,
    pub client: String,
    pub project: String,

    pub source: Source,
    pub path: String,
    pub includes: String,
    pub excludes: String,

    // repo-docs
    pub include_sdd: bool,
    // claude-memories
    pub host_scope: bool,
    // git-history
    pub since_commit: String,
    // db-schema
    pub dsn: String,
    pub supabase: bool,
    pub include_data: bool,
    pub tables: String,
    pub sample_limit: String,
    pub redact_pii: bool,
    pub attest: String,

    pub no_llm: bool,
    pub max_tokens: String,
    pub claude_bin: String,
    pub verbose: bool,
    pub retries: String,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            // Deliberately NOT `NEXUSMIND_BASE_URL`. That variable points at
            // production in a working shell, and a migration TUI that inherits
            // it publishes a first experimental run to the live org. The env
            // value is offered as an explicit choice on the connection screen,
            // labelled for what it is.
            api_url: "http://localhost:8080".to_string(),
            api_key: std::env::var("NEXUSMIND_API_KEY").unwrap_or_default(),
            client: String::new(),
            project: String::new(),
            source: Source::RepoDocs,
            path: ".".to_string(),
            includes: String::new(),
            excludes: String::new(),
            include_sdd: false,
            host_scope: false,
            since_commit: String::new(),
            // Read from the environment on purpose: a DSN typed into a field is
            // a DSN that can end up in a screenshot. See `to_args`.
            dsn: std::env::var("NEXUSMIND_SOURCE_DSN").unwrap_or_default(),
            supabase: false,
            include_data: false,
            tables: String::new(),
            sample_limit: String::new(),
            redact_pii: false,
            attest: String::new(),
            no_llm: false,
            max_tokens: String::new(),
            claude_bin: "claude".to_string(),
            verbose: false,
            retries: String::new(),
        }
    }
}

/// A reason the run may not start. Blockers are hard; the UI cannot override
/// them, it can only show them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocker {
    pub field: &'static str,
    pub why: String,
}

/// Something true and worth seeing that does not stop the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub headline: String,
    pub detail: String,
}

/// Appends `--flag value`, or nothing at all when the value is blank.
///
/// A free function rather than a closure so it can be called alongside direct
/// pushes to the same vector.
fn push_flag(args: &mut Vec<String>, flag: &str, value: &str) {
    if !value.trim().is_empty() {
        args.push(flag.to_string());
        args.push(value.trim().to_string());
    }
}

fn split_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// A URL that is not obviously a local backend.
///
/// Deliberately conservative: anything not plainly loopback counts as remote.
/// A false "this is remote" costs the operator one glance; a false "this is
/// local" costs them a production write.
pub fn is_local(api_url: &str) -> bool {
    let host = api_url
        .split("://")
        .nth(1)
        .unwrap_or(api_url)
        .split('/')
        .next()
        .unwrap_or_default();
    let host = host.rsplit_once(':').map_or(host, |(h, _)| h);
    matches!(host, "localhost" | "127.0.0.1" | "0.0.0.0" | "[::1]" | "::1")
}

impl RunConfig {
    /// The command line for `migrate-knowledge`, minus the binary itself.
    ///
    /// # The DSN is not here, and never will be
    ///
    /// `argv` is readable by every process on the machine through `ps`, is
    /// written verbatim into shell history, and is captured by command logging.
    /// A database URL contains a password. It travels in the child's
    /// environment (`env_vars`) and nowhere else — the runner's own `--dsn`
    /// flag exists only so it can refuse it loudly.
    pub fn to_args(&self, dry_run: bool) -> Vec<String> {
        let mut a: Vec<String> = vec![
            "--json".into(),
            "--source".into(),
            self.source.flag().into(),
        ];

        if self.source.takes_path() {
            push_flag(&mut a, "--path", &self.path);
        }
        push_flag(&mut a, "--api-url", &self.api_url);
        push_flag(&mut a, "--api-key", &self.api_key);
        push_flag(&mut a, "--client", &self.client);
        push_flag(&mut a, "--project", &self.project);
        for inc in split_list(&self.includes) {
            push_flag(&mut a, "--include", &inc);
        }
        for exc in split_list(&self.excludes) {
            push_flag(&mut a, "--exclude", &exc);
        }
        push_flag(&mut a, "--since-commit", &self.since_commit);
        push_flag(&mut a, "--max-tokens", &self.max_tokens);
        push_flag(&mut a, "--claude-bin", &self.claude_bin);
        push_flag(&mut a, "--retries", &self.retries);

        match self.source {
            Source::RepoDocs if self.include_sdd => a.push("--include-sdd".into()),
            Source::ClaudeMemories if self.host_scope => a.push("--host-scope".into()),
            Source::DbSchema => {
                if self.supabase {
                    a.push("--supabase".into());
                }
                if self.include_data {
                    a.push("--include-data".into());
                    for t in split_list(&self.tables) {
                        a.push("--tables".into());
                        a.push(t);
                    }
                    push_flag(&mut a, "--sample-limit", &self.sample_limit);
                    if self.redact_pii {
                        a.push("--redact-pii".into());
                    }
                    push_flag(&mut a, "--attest", &self.attest);
                }
            }
            _ => {}
        }

        if self.no_llm {
            a.push("--no-llm".into());
        }
        if self.verbose {
            a.push("--verbose".into());
        }
        if dry_run {
            a.push("--dry-run".into());
        }
        a
    }

    /// What the child process inherits beyond the parent environment.
    pub fn env_vars(&self) -> Vec<(String, String)> {
        let mut env = Vec::new();
        if self.source == Source::DbSchema && !self.dsn.trim().is_empty() {
            env.push((
                "NEXUSMIND_SOURCE_DSN".to_string(),
                self.dsn.trim().to_string(),
            ));
        }
        env
    }

    /// A command line safe to show on screen and paste into a bug report.
    pub fn display_command(&self, dry_run: bool) -> String {
        let redacted: Vec<String> = {
            let args = self.to_args(dry_run);
            let mut out = Vec::with_capacity(args.len());
            let mut mask_next = false;
            for arg in args {
                if mask_next {
                    out.push("«hidden»".to_string());
                    mask_next = false;
                    continue;
                }
                mask_next = arg == "--api-key";
                out.push(arg);
            }
            out
        };
        let mut line = String::new();
        for (k, _) in self.env_vars() {
            line.push_str(&format!("{k}=«from environment» "));
        }
        line.push_str("migrate-knowledge ");
        line.push_str(&shell_words::join(redacted));
        line
    }

    /// Everything that must be true before this run may be launched.
    ///
    /// A dry run has a shorter list: it posts nothing, so it needs no
    /// credentials. That distinction is what makes the preview screen usable
    /// before the operator has an API key in hand.
    pub fn blockers(&self, dry_run: bool) -> Vec<Blocker> {
        let mut b = Vec::new();
        let need = |v: &str| v.trim().is_empty();

        if self.source.takes_path() && need(&self.path) {
            b.push(Blocker {
                field: "path",
                why: "a source path is required".into(),
            });
        }

        if !dry_run {
            if need(&self.api_url) {
                b.push(Blocker {
                    field: "api_url",
                    why: "without a backend URL the run has nowhere to stage".into(),
                });
            }
            if need(&self.api_key) {
                b.push(Blocker {
                    field: "api_key",
                    why: "an API key is required to stage candidates".into(),
                });
            }
        }

        if self.source == Source::DbSchema {
            if need(&self.dsn) {
                b.push(Blocker {
                    field: "dsn",
                    why: "set NEXUSMIND_SOURCE_DSN before starting the TUI; the DSN is \
                          never typed into a field or passed as an argument"
                        .into(),
                });
            }
            // The four cumulative gates on reading client rows. They are checked
            // again server-side; duplicating them here is not redundancy, it is
            // the difference between a refusal the operator understands and an
            // opaque error after a long scan.
            if self.include_data {
                if split_list(&self.tables).is_empty() {
                    b.push(Blocker {
                        field: "tables",
                        why: "row sampling requires an explicit table allowlist — there is \
                              no 'all tables' option, by design"
                            .into(),
                    });
                }
                match self.sample_limit.trim().parse::<u32>() {
                    Ok(n) if n > 0 => {}
                    _ => b.push(Blocker {
                        field: "sample_limit",
                        why: "row sampling requires a bounded, positive row limit".into(),
                    }),
                }
                if !self.redact_pii {
                    b.push(Blocker {
                        field: "redact_pii",
                        why: "PII redaction runs in-process before a sample leaves the \
                              machine, and cannot be skipped when sampling rows"
                            .into(),
                    });
                }
                if self.attest.trim().len() < 8 {
                    b.push(Blocker {
                        field: "attest",
                        why: "record who authorised reading client rows, and under what \
                              agreement — this is written onto the run"
                            .into(),
                    });
                }
            }
        }

        if let Some(raw) = Some(self.max_tokens.trim()).filter(|s| !s.is_empty()) {
            if raw.parse::<i64>().map(|n| n <= 0).unwrap_or(true) {
                b.push(Blocker {
                    field: "max_tokens",
                    why: "the token budget must be a positive number".into(),
                });
            }
        }

        if let Some(raw) = Some(self.retries.trim()).filter(|s| !s.is_empty()) {
            if raw.parse::<u32>().map(|n| n == 0).unwrap_or(true) {
                b.push(Blocker {
                    field: "retries",
                    why: "the retry count must be a positive number".into(),
                });
            }
        }
        b
    }

    pub fn warnings(&self, dry_run: bool) -> Vec<Warning> {
        let mut w = Vec::new();
        if !dry_run && !is_local(&self.api_url) {
            w.push(Warning {
                headline: format!("{} is not a local backend", self.api_url),
                detail: "Candidates staged here land in a shared org. Review the client and \
                         project before starting."
                    .into(),
            });
        }
        if !dry_run && self.client.trim().is_empty() {
            w.push(Warning {
                headline: "No client selected".into(),
                detail: "The run will be org-scoped. For consultancy work that is almost \
                         always wrong — a client's knowledge should carry its client id."
                    .into(),
            });
        }
        if self.include_data {
            w.push(Warning {
                headline: "Row sampling is unlocked".into(),
                detail: "Real client rows will be read, redacted locally, and staged for \
                         review. Your attestation is recorded on the run."
                    .into(),
            });
        }
        if !self.no_llm && self.max_tokens.trim().is_empty() {
            w.push(Warning {
                headline: "No token budget".into(),
                detail: "Classification will run until the source is exhausted. Set a budget \
                         to make the first run of an unfamiliar source cheap."
                    .into(),
            });
        }
        w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db_config() -> RunConfig {
        RunConfig {
            source: Source::DbSchema,
            dsn: "postgres://ro:secret@db.internal/app".into(),
            api_url: "http://localhost:8080".into(),
            api_key: "nm_x".into(),
            ..Default::default()
        }
    }

    /// The single most important property in this file.
    #[test]
    fn the_dsn_never_reaches_the_command_line() {
        let cfg = db_config();
        let args = cfg.to_args(false).join(" ");
        assert!(
            !args.contains("secret") && !args.contains("postgres://"),
            "the DSN leaked into argv: {args}"
        );
        assert!(!args.contains("--dsn"), "the runner refuses --dsn anyway");
        assert_eq!(
            cfg.env_vars(),
            vec![(
                "NEXUSMIND_SOURCE_DSN".to_string(),
                "postgres://ro:secret@db.internal/app".to_string()
            )],
            "it travels in the environment instead"
        );
    }

    #[test]
    fn the_api_key_is_masked_in_the_displayed_command() {
        let cfg = RunConfig {
            api_key: "nm_000000000000000000".into(),
            ..Default::default()
        };
        let shown = cfg.display_command(true);
        assert!(!shown.contains("nm_000000000000000000"), "{shown}");
        assert!(shown.contains("--api-key «hidden»"), "{shown}");
    }

    /// The production URL in the operator's shell must not become the default.
    #[test]
    fn the_default_backend_is_local() {
        assert_eq!(RunConfig::default().api_url, "http://localhost:8080");
        assert!(is_local("http://localhost:8080"));
        assert!(is_local("http://127.0.0.1:9999/"));
        assert!(!is_local("https://api.nexusmind.smartcoderlabs.com"));
        assert!(
            !is_local("https://localhost.evil.com"),
            "a suffix match would be a hole"
        );
    }

    #[test]
    fn row_sampling_needs_all_four_gates_before_it_may_run() {
        let mut cfg = db_config();
        cfg.include_data = true;
        let fields: Vec<&str> = cfg.blockers(false).iter().map(|b| b.field).collect();
        assert_eq!(
            fields,
            vec!["tables", "sample_limit", "redact_pii", "attest"],
            "all four gates report at once, so the operator sees the whole cost"
        );

        cfg.tables = "invoices, customers".into();
        cfg.sample_limit = "50".into();
        cfg.redact_pii = true;
        cfg.attest = "MSA-2026-014, approved by legal".into();
        assert!(cfg.blockers(false).is_empty());

        let args = cfg.to_args(false);
        assert!(args.contains(&"--include-data".to_string()));
        assert_eq!(
            args.iter().filter(|a| *a == "--tables").count(),
            2,
            "each allowlisted table is its own flag"
        );
        assert!(args.contains(&"--redact-pii".to_string()));
    }

    #[test]
    fn removing_any_single_gate_blocks_the_run_again() {
        let base = {
            let mut c = db_config();
            c.include_data = true;
            c.tables = "invoices".into();
            c.sample_limit = "50".into();
            c.redact_pii = true;
            c.attest = "MSA-2026-014".into();
            c
        };
        assert!(base.blockers(false).is_empty(), "baseline must be clean");

        /// One gate removed from an otherwise valid sampling configuration.
        type DropGate = (&'static str, fn(&mut RunConfig));
        let mutations: Vec<DropGate> = vec![
            ("tables", |c| c.tables.clear()),
            ("sample_limit", |c| c.sample_limit.clear()),
            ("redact_pii", |c| c.redact_pii = false),
            ("attest", |c| c.attest.clear()),
        ];
        for (field, mutate) in mutations {
            let mut c = base.clone();
            mutate(&mut c);
            let blocked: Vec<&str> = c.blockers(false).iter().map(|b| b.field).collect();
            assert_eq!(blocked, vec![field], "dropping {field} must block the run");
        }
    }

    /// A zero limit is not a bounded limit; it reads as "no limit" to a reader
    /// and would be an unbounded sample if it ever reached the connector.
    #[test]
    fn a_zero_sample_limit_is_refused() {
        let mut cfg = db_config();
        cfg.include_data = true;
        cfg.tables = "invoices".into();
        cfg.redact_pii = true;
        cfg.attest = "MSA-2026-014".into();
        for bad in ["0", "-1", "many", ""] {
            cfg.sample_limit = bad.into();
            let fields: Vec<&str> = cfg.blockers(false).iter().map(|b| b.field).collect();
            assert!(fields.contains(&"sample_limit"), "{bad:?} slipped through");
        }
    }

    /// Empty means unset — a blank retry count is not itself a blocker — but
    /// anything present must be a genuine positive number.
    #[test]
    fn retries_is_optional_but_must_be_a_positive_number_when_set() {
        let mut cfg = RunConfig::default();
        assert!(cfg.blockers(true).is_empty(), "empty retries must not block");
        assert!(!cfg.to_args(true).contains(&"--retries".to_string()));

        for bad in ["0", "-1", "many"] {
            cfg.retries = bad.into();
            let fields: Vec<&str> = cfg.blockers(true).iter().map(|b| b.field).collect();
            assert!(fields.contains(&"retries"), "{bad:?} slipped through");
        }

        cfg.retries = "3".into();
        assert!(cfg.blockers(true).is_empty());
        let args = cfg.to_args(true);
        let idx = args.iter().position(|a| a == "--retries").expect("--retries missing");
        assert_eq!(args[idx + 1], "3");
    }

    /// A dry run posts nothing, so it must not demand credentials — otherwise
    /// nobody can preview a source before being onboarded.
    #[test]
    fn a_dry_run_needs_no_credentials_but_a_real_run_does() {
        let cfg = RunConfig {
            api_key: String::new(),
            api_url: String::new(),
            ..Default::default()
        };
        assert!(cfg.blockers(true).is_empty());
        let fields: Vec<&str> = cfg.blockers(false).iter().map(|b| b.field).collect();
        assert_eq!(fields, vec!["api_url", "api_key"]);
    }

    #[test]
    fn include_and_exclude_are_split_into_repeated_flags() {
        let cfg = RunConfig {
            includes: "docs/adr, docs/guides".into(),
            excludes: " node_modules ,, vendor".into(),
            ..Default::default()
        };
        let args = cfg.to_args(true);
        assert_eq!(args.iter().filter(|a| *a == "--include").count(), 2);
        assert_eq!(
            args.iter().filter(|a| *a == "--exclude").count(),
            2,
            "empty segments are dropped, not passed as empty flags"
        );
        assert!(args.contains(&"docs/guides".to_string()));
    }

    /// Options belonging to another connector must not leak into this one's
    /// command line — `--host-scope` on a repo-docs run is a runner error the
    /// operator would have to decode.
    #[test]
    fn per_connector_options_do_not_leak_across_sources() {
        let cfg = RunConfig {
            source: Source::RepoDocs,
            include_sdd: true,
            host_scope: true,
            supabase: true,
            ..Default::default()
        };
        let args = cfg.to_args(true);
        assert!(args.contains(&"--include-sdd".to_string()));
        assert!(!args.contains(&"--host-scope".to_string()));
        assert!(!args.contains(&"--supabase".to_string()));
    }

    #[test]
    fn db_schema_sends_no_path_because_it_has_none() {
        let args = db_config().to_args(true);
        assert!(!args.contains(&"--path".to_string()));
    }

    #[test]
    fn verbose_emits_the_flag_only_when_on() {
        let mut cfg = RunConfig::default();
        assert!(!cfg.to_args(true).contains(&"--verbose".to_string()));

        cfg.verbose = true;
        assert!(cfg.to_args(true).contains(&"--verbose".to_string()));
    }

    #[test]
    fn a_remote_backend_and_a_missing_client_are_warned_about_not_blocked() {
        let cfg = RunConfig {
            api_url: "https://api.nexusmind.smartcoderlabs.com".into(),
            api_key: "nm_x".into(),
            ..Default::default()
        };
        assert!(cfg.blockers(false).is_empty(), "warnings never block");
        let heads: Vec<String> = cfg.warnings(false).iter().map(|w| w.headline.clone()).collect();
        assert!(heads.iter().any(|h| h.contains("not a local backend")), "{heads:?}");
        assert!(heads.iter().any(|h| h.contains("No client")), "{heads:?}");
    }
}
