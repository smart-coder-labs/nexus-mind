//! Interim floor for autonomous merges (factory plan D10): until the per-action
//! policy engine exists, the PR reviewer may only merge PRs whose every changed
//! path is documentation or tests. Deliberately not configurable from run config,
//! which `autonomous_agent:update` holders can edit — it must never widen merge
//! eligibility.

/// One entry of GitHub's `GET /pulls/{n}/files`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedFile {
    pub filename: String,
    pub status: String,
    pub previous_filename: Option<String>,
}

/// The commit the review actually ran on. Only webhook triggers carry it; a run
/// without a full 40-hex SHA has no provable reviewed commit and cannot merge.
pub fn reviewed_head_sha(config: &serde_json::Value) -> Option<&str> {
    config
        .pointer("/trigger/head_sha")
        .and_then(|v| v.as_str())
        .filter(|sha| sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Every configured required check must have reported on the commit and be
/// green; with none configured, every reported run must be green. A missing
/// required check is a decline, never a pass.
pub fn required_checks_verdict(
    required: &[String],
    runs: &[serde_json::Value],
) -> Result<(), String> {
    let name =
        |run: &serde_json::Value| run.get("name").and_then(|v| v.as_str()).map(str::to_string);
    let is_green = |run: &serde_json::Value| {
        run.get("status").and_then(|s| s.as_str()) == Some("completed")
            && matches!(
                run.get("conclusion").and_then(|c| c.as_str()),
                Some("success" | "neutral" | "skipped")
            )
    };
    if required.is_empty() {
        if runs.is_empty() {
            return Err("no_checks_to_verify".into());
        }
        return if runs.iter().all(is_green) {
            Ok(())
        } else {
            Err("checks_not_green".into())
        };
    }
    for check in required {
        let matching: Vec<&serde_json::Value> = runs
            .iter()
            .filter(|run| name(run).as_deref() == Some(check.as_str()))
            .collect();
        if matching.is_empty() {
            return Err(format!("required_check_missing:{check}"));
        }
        if !matching.iter().all(|run| is_green(run)) {
            return Err("checks_not_green".into());
        }
    }
    Ok(())
}

/// Converts GitHub's combined commit status (`GET /commits/{sha}/status`, used by
/// Vercel, CircleCI and other non-Checks integrations) into check-run-shaped values
/// so `required_checks_verdict` judges both APIs the same way.
pub fn statuses_as_runs(combined: &serde_json::Value) -> Result<Vec<serde_json::Value>, String> {
    let statuses = combined
        .get("statuses")
        .and_then(|v| v.as_array())
        .ok_or("commit_statuses_unreadable")?;
    statuses
        .iter()
        .map(|status| {
            let context = status
                .get("context")
                .and_then(|v| v.as_str())
                .ok_or("commit_statuses_unreadable")?;
            let (state, conclusion) = match status.get("state").and_then(|v| v.as_str()) {
                Some("success") => ("completed", Some("success")),
                Some("failure" | "error") => ("completed", Some("failure")),
                Some("pending") => ("in_progress", None),
                _ => return Err("commit_statuses_unreadable".to_string()),
            };
            Ok(serde_json::json!({"name": context, "status": state, "conclusion": conclusion}))
        })
        .collect()
}

/// Maximum files inspected before declining; larger PRs are never automatic.
pub const MAX_PULL_FILES: usize = 300;

/// Parses one page of `GET /pulls/{n}/files`. Any malformed entry rejects the
/// whole page: an unreadable file list must decline the merge, not shrink it.
pub fn parse_changed_files(page: &serde_json::Value) -> Result<Vec<ChangedFile>, String> {
    let entries = page.as_array().ok_or("pull_files_unreadable")?;
    entries
        .iter()
        .map(|entry| {
            let field = |key: &str| entry.get(key).and_then(|v| v.as_str()).map(str::to_string);
            Ok(ChangedFile {
                filename: field("filename").ok_or("pull_files_unreadable")?,
                status: field("status").ok_or("pull_files_unreadable")?,
                previous_filename: field("previous_filename"),
            })
        })
        .collect()
}

/// The policy task class of an eligible PR: `docs` when every file is
/// documentation, otherwise `tests` (the only other eligible class).
pub fn merge_task_class(files: &[ChangedFile]) -> crate::factory::contracts::TaskClass {
    if files.iter().all(|file| is_doc_path(&file.filename)) {
        crate::factory::contracts::TaskClass::Docs
    } else {
        crate::factory::contracts::TaskClass::Tests
    }
}

/// `Ok(())` when every changed file may be auto-merged; otherwise the
/// `path_not_eligible:<path>` reason naming the first offending path.
pub fn auto_merge_path_verdict(files: &[ChangedFile]) -> Result<(), String> {
    if files.is_empty() {
        return Err("no_changed_files".into());
    }
    for file in files {
        // Removing a file (tests included) weakens verification; never automatic.
        if file.status == "removed" {
            return Err(format!("path_not_eligible:{}", file.filename));
        }
        if let Some(previous) = file.previous_filename.as_deref() {
            if !is_eligible_path(previous) {
                return Err(format!("path_not_eligible:{previous}"));
            }
        }
        if !is_eligible_path(&file.filename) {
            return Err(format!("path_not_eligible:{}", file.filename));
        }
    }
    Ok(())
}

fn is_eligible_path(path: &str) -> bool {
    if is_never_eligible(path) {
        return false;
    }
    let segments: Vec<&str> = path.split('/').collect();
    let name = segments.last().copied().unwrap_or_default();
    let in_test_dir = segments[..segments.len() - 1]
        .iter()
        .any(|segment| matches!(*segment, "tests" | "test" | "__tests__" | "e2e"));
    let is_test_file = name.contains(".test.")
        || name.contains(".spec.")
        || name.ends_with("_test.go")
        || name.ends_with("_test.rs")
        || name.ends_with("_test.py")
        || (name.starts_with("test_") && name.ends_with(".py"));
    is_doc_path(path) || in_test_dir || is_test_file
}

/// Under `docs/` only prose and images count: `docs/conf.py` or a site config is
/// executable in a docs build and must not ride along as "documentation". `.mdx`
/// is excluded everywhere: it compiles to JS and can be a live route.
fn is_doc_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".md")
        || (lower.starts_with("docs/")
            && [
                ".txt", ".rst", ".adoc", ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg",
            ]
            .iter()
            .any(|ext| lower.ends_with(ext)))
}

/// Files that change build, CI, dependencies or agent behavior. Blocked wherever
/// they live, so a `docs/` or `tests/` prefix cannot smuggle them in. Matching is
/// case-insensitive because file systems and tools often are.
fn is_never_eligible(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    let name = path.rsplit('/').next().unwrap_or(&path);
    // Dot-directories and dotfiles hold CI, editor, package-manager and agent
    // configuration (.github, .claude, .cursor, .npmrc, .mcp.json, .vitepress...).
    let hidden = path.split('/').any(|segment| segment.starts_with('.'));
    // Plugin layouts keep agent instructions in ordinary directories
    // (`plugins/x/commands/*.md`, `skills/*/references/*.md`, `agents/*.md`).
    let in_agent_dir = path.split('/').any(|segment| {
        matches!(
            segment,
            "agents" | "commands" | "skills" | "prompts" | "hooks" | "rules" | "plugins"
        )
    });
    let agent_instructions = (in_agent_dir && name.ends_with(".md"))
        || (name.starts_with("claude") && name.ends_with(".md"))
        || matches!(
            name,
            "agents.md" | "gemini.md" | "copilot-instructions.md" | "skill.md"
        );
    let manifest = matches!(
        name,
        "cargo.lock"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "pnpm-workspace.yaml"
            | "yarn.lock"
            | "bun.lockb"
            | "package.json"
            | "go.mod"
            | "go.sum"
            | "gemfile"
            | "pom.xml"
            | "setup.py"
            | "setup.cfg"
            | "conftest.py"
            | "makefile"
    ) || (name.starts_with("requirements") && name.ends_with(".txt"))
        || name.starts_with("gemfile")
        || name.starts_with("dockerfile")
        || name.starts_with("containerfile")
        || [".toml", ".lock", ".gradle", ".gradle.kts", ".csproj", ".sh"]
            .iter()
            .any(|ext| name.ends_with(ext));
    hidden || agent_instructions || manifest || path == "openspec/config.yaml"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(filename: &str, status: &str) -> ChangedFile {
        ChangedFile {
            filename: filename.into(),
            status: status.into(),
            previous_filename: None,
        }
    }

    fn renamed(from: &str, to: &str) -> ChangedFile {
        ChangedFile {
            filename: to.into(),
            status: "renamed".into(),
            previous_filename: Some(from.into()),
        }
    }

    #[test]
    fn docs_and_tests_are_eligible() {
        let files = [
            file("README.md", "modified"),
            file("docs/factory/PLAN.md", "added"),
            file("docs/diagram.png", "added"),
            file("apps/admin/src/pages/Home.test.tsx", "modified"),
            file("apps/landing/src/nav.spec.ts", "added"),
            file("apps/backend/tests/merge.rs", "added"),
            file("apps/admin/src/__tests__/x.tsx", "modified"),
            file("svc/handler_test.go", "modified"),
            file("tools/test_parser.py", "added"),
            file("e2e/login.ts", "modified"),
        ];
        assert_eq!(auto_merge_path_verdict(&files), Ok(()));
    }

    #[test]
    fn source_paths_are_not_eligible() {
        let files = [
            file("docs/a.md", "modified"),
            file("apps/backend/src/auth/mod.rs", "modified"),
        ];
        assert_eq!(
            auto_merge_path_verdict(&files),
            Err("path_not_eligible:apps/backend/src/auth/mod.rs".into())
        );
    }

    #[test]
    fn sensitive_files_are_never_eligible_even_under_docs_or_tests() {
        for path in [
            ".github/workflows/ci.yml",
            "docs/.github/workflows/x.yml",
            "Cargo.lock",
            "apps/admin/package-lock.json",
            "pnpm-lock.yaml",
            "tests/package.json",
            "tests/Cargo.toml",
            "docs/Dockerfile",
            "CLAUDE.md",
            "apps/backend/AGENTS.md",
            ".mcp.json",
            "openspec/config.yaml",
        ] {
            assert_eq!(
                auto_merge_path_verdict(&[file(path, "modified")]),
                Err(format!("path_not_eligible:{path}")),
                "{path} must not be auto-mergeable"
            );
        }
    }

    #[test]
    fn deleting_a_test_is_not_eligible() {
        assert_eq!(
            auto_merge_path_verdict(&[file("apps/backend/tests/merge.rs", "removed")]),
            Err("path_not_eligible:apps/backend/tests/merge.rs".into())
        );
    }

    #[test]
    fn rename_requires_both_sides_eligible() {
        assert_eq!(
            auto_merge_path_verdict(&[renamed("docs/a.md", "docs/b.md")]),
            Ok(())
        );
        assert_eq!(
            auto_merge_path_verdict(&[renamed("src/lib.rs", "docs/lib.md")]),
            Err("path_not_eligible:src/lib.rs".into())
        );
    }

    #[test]
    fn reviewed_head_comes_only_from_the_trigger_and_must_be_a_full_sha() {
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let config = serde_json::json!({"trigger": {"head_sha": sha}});
        assert_eq!(reviewed_head_sha(&config), Some(sha));
        assert_eq!(reviewed_head_sha(&serde_json::json!({})), None);
        assert_eq!(
            reviewed_head_sha(&serde_json::json!({"head_sha": sha})),
            None
        );
        for bad in [
            "main",
            "0123abc",
            "zz23456789abcdef0123456789abcdef01234567",
        ] {
            assert_eq!(
                reviewed_head_sha(&serde_json::json!({"trigger": {"head_sha": bad}})),
                None,
                "{bad}"
            );
        }
    }

    #[test]
    fn parses_github_pull_files_page() {
        let page = serde_json::json!([
            {"filename": "docs/b.md", "status": "renamed", "previous_filename": "docs/a.md"},
            {"filename": "tests/x.rs", "status": "added"}
        ]);
        assert_eq!(
            parse_changed_files(&page),
            Ok(vec![
                renamed("docs/a.md", "docs/b.md"),
                file("tests/x.rs", "added")
            ])
        );
    }

    #[test]
    fn malformed_pull_files_page_is_rejected() {
        assert!(parse_changed_files(&serde_json::json!({"message": "Not Found"})).is_err());
        assert!(parse_changed_files(&serde_json::json!([{"status": "added"}])).is_err());
    }

    #[test]
    fn empty_file_list_is_not_eligible() {
        assert_eq!(auto_merge_path_verdict(&[]), Err("no_changed_files".into()));
    }

    #[test]
    fn agent_instruction_files_are_never_eligible() {
        for path in [
            ".claude/commands/review.md",
            ".claude/agents/fixer.md",
            "skills/deploy/SKILL.md",
            ".cursor/rules/style.md",
            "CLAUDE.local.md",
            "claude.md",
            "docs/agents.md",
            "GEMINI.md",
            "copilot-instructions.md",
        ] {
            assert_eq!(
                auto_merge_path_verdict(&[file(path, "added")]),
                Err(format!("path_not_eligible:{path}")),
                "{path} changes agent behavior"
            );
        }
    }

    #[test]
    fn task_class_is_docs_only_when_every_file_is_documentation() {
        use crate::factory::contracts::TaskClass;
        assert_eq!(
            merge_task_class(&[file("README.md", "modified"), file("docs/a.png", "added")]),
            TaskClass::Docs
        );
        assert_eq!(
            merge_task_class(&[file("docs/a.md", "modified"), file("tests/x.rs", "added")]),
            TaskClass::Tests
        );
    }

    #[test]
    fn plugin_instruction_directories_are_never_eligible() {
        for path in [
            "plugins/x/commands/review.md",
            "agents/fixer.md",
            "skills/deploy/references/api.md",
            "prompts/system.md",
            "hooks/pre-commit.md",
            "rules/style.md",
        ] {
            assert!(
                auto_merge_path_verdict(&[file(path, "modified")]).is_err(),
                "{path} is agent behavior"
            );
        }
    }

    #[test]
    fn mdx_is_code_not_docs() {
        for path in ["docs/guide.mdx", "apps/landing/app/pricing/page.mdx"] {
            assert!(
                auto_merge_path_verdict(&[file(path, "modified")]).is_err(),
                "{path} compiles to JS"
            );
        }
    }

    #[test]
    fn commit_statuses_become_check_runs() {
        let combined = serde_json::json!({"statuses": [
            {"context": "vercel", "state": "success"},
            {"context": "ci/circleci", "state": "failure"},
            {"context": "deploy", "state": "error"},
            {"context": "preview", "state": "pending"}
        ]});
        assert_eq!(
            statuses_as_runs(&combined),
            Ok(vec![
                run("vercel", "completed", Some("success")),
                run("ci/circleci", "completed", Some("failure")),
                run("deploy", "completed", Some("failure")),
                run("preview", "in_progress", None),
            ])
        );
        assert!(statuses_as_runs(&serde_json::json!({"message": "Not Found"})).is_err());
    }

    #[test]
    fn a_red_commit_status_blocks_the_merge() {
        let runs = [
            run("ci", "completed", Some("success")),
            run("vercel", "completed", Some("failure")),
        ];
        assert_eq!(
            required_checks_verdict(&[], &runs),
            Err("checks_not_green".into())
        );
    }

    #[test]
    fn executable_files_under_docs_are_not_eligible() {
        for path in [
            "docs/conf.py",
            "docs/.vitepress/config.ts",
            "docs/Makefile",
            "docs/deploy.sh",
        ] {
            assert!(
                auto_merge_path_verdict(&[file(path, "modified")]).is_err(),
                "{path}"
            );
        }
    }

    #[test]
    fn dependency_and_build_manifests_are_never_eligible_in_any_ecosystem() {
        for path in [
            "tests/requirements.txt",
            "tests/requirements-dev.txt",
            "e2e/go.mod",
            "e2e/go.sum",
            "test/Gemfile",
            "test/Gemfile.lock",
            "tests/pom.xml",
            "tests/build.gradle",
            "tests/build.gradle.kts",
            "tests/App.Tests.csproj",
            "tests/.npmrc",
            "tests/pnpm-workspace.yaml",
            "tests/setup.py",
            "tests/setup.cfg",
            "tests/conftest.py",
            "tests/Containerfile",
            "tests/dockerfile",
            "tests/Makefile",
        ] {
            assert!(
                auto_merge_path_verdict(&[file(path, "modified")]).is_err(),
                "{path}"
            );
        }
    }

    fn run(name: &str, status: &str, conclusion: Option<&str>) -> serde_json::Value {
        serde_json::json!({"name": name, "status": status, "conclusion": conclusion})
    }

    #[test]
    fn every_required_check_must_be_present_and_green() {
        let required = vec!["ci".to_string(), "security".to_string()];
        let only_ci = [run("ci", "completed", Some("success"))];
        assert_eq!(
            required_checks_verdict(&required, &only_ci),
            Err("required_check_missing:security".into())
        );
        let both = [
            run("ci", "completed", Some("success")),
            run("security", "completed", Some("skipped")),
            run("lint", "completed", Some("failure")),
        ];
        assert_eq!(required_checks_verdict(&required, &both), Ok(()));
        let pending = [
            run("ci", "completed", Some("success")),
            run("security", "in_progress", None),
        ];
        assert_eq!(
            required_checks_verdict(&required, &pending),
            Err("checks_not_green".into())
        );
    }

    #[test]
    fn a_failing_run_blocks_even_when_another_run_with_that_name_passed() {
        // The check-runs endpoint already returns only the latest run per name
        // (`filter=latest`), so two runs sharing a name come from different apps.
        // Fail closed rather than guess which one is authoritative.
        let required = vec!["ci".to_string()];
        let runs = [
            run("ci", "completed", Some("failure")),
            run("ci", "completed", Some("success")),
        ];
        assert_eq!(
            required_checks_verdict(&required, &runs),
            Err("checks_not_green".into())
        );
    }

    #[test]
    fn without_required_checks_all_reported_runs_must_be_green() {
        assert_eq!(
            required_checks_verdict(&[], &[]),
            Err("no_checks_to_verify".into())
        );
        assert_eq!(
            required_checks_verdict(&[], &[run("ci", "completed", Some("success"))]),
            Ok(())
        );
        assert_eq!(
            required_checks_verdict(
                &[],
                &[
                    run("ci", "completed", Some("success")),
                    run("e2e", "completed", Some("failure"))
                ]
            ),
            Err("checks_not_green".into())
        );
    }

    #[test]
    fn test_word_inside_a_name_is_not_a_test_path() {
        // `latest/` and `contest.rs` contain "test" but are production code.
        for path in [
            "src/latest/mod.rs",
            "src/contest.rs",
            "src/testing_utils.rs",
        ] {
            assert!(
                auto_merge_path_verdict(&[file(path, "modified")]).is_err(),
                "{path}"
            );
        }
    }
}
