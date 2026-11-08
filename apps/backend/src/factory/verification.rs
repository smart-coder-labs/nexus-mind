//! Verification evidence (factory F1 §5): the commands a task pod runs on the
//! exact head, plus the required CI checks, folded into the F0
//! [`VerificationReport`] contract that the merge path requires.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::contracts::{CheckResult, CheckStatus, SchemaV1, VerificationReport};

/// Programs a verification command may start with.
const ALLOWED_PROGRAMS: &[&str] = &["npm", "npx", "pnpm", "yarn", "bun", "cargo", "python3", "python", "go", "flutter", "dart", "./gradlew", "gradle", "swift", "xcodebuild", "dotnet", "mvn", "./mvnw", "make"];
const MAX_COMMANDS: usize = 8;
/// Per-command limit inside the pod.
pub const COMMAND_TIMEOUT_SECS: u64 = 300;
/// Time a command may take including the kill grace, for budgeting the pod.
pub const COMMAND_BUDGET_SECS: u64 = COMMAND_TIMEOUT_SECS + 10;

/// Parses `verification_commands` (a list of argv arrays) against the allowlist.
pub fn parse_verification_commands(value: Option<&Value>) -> anyhow::Result<Vec<Vec<String>>> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(Vec::new());
    };
    let commands = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("verification_commands_invalid"))?;
    if commands.len() > MAX_COMMANDS {
        anyhow::bail!("too_many_verification_commands")
    }
    commands
        .iter()
        .map(|argv| {
            let parts = argv
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("command_must_be_argv"))?;
            let args = parts
                .iter()
                .map(|part| {
                    part.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| anyhow::anyhow!("command_arg_invalid"))
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            let Some(program) = args.first() else {
                anyhow::bail!("empty_command")
            };
            if !ALLOWED_PROGRAMS.contains(&program.as_str()) {
                anyhow::bail!("command_not_allowlisted")
            }
            Ok(args)
        })
        .collect()
}

/// The outcome of one verification command run in the pod.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerificationReceipt {
    pub argv: Vec<String>,
    /// `None` when the command could not be run or its status was lost.
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}

/// Builds the report for `head_sha`. Every check is blocking: a failed or
/// errored command or CI check blocks the merge, and a report without any
/// passing check is not eligible.
pub fn build_report(
    task_id: &str,
    head_sha: &str,
    receipts: &[VerificationReceipt],
    required_checks: &[String],
    ci_runs: &[Value],
) -> anyhow::Result<VerificationReport> {
    use super::contracts::Contract;
    let mut checks: Vec<CheckResult> = receipts
        .iter()
        .map(|receipt| CheckResult {
            name: truncate(format!("cmd:{}", receipt.argv.join(" "))),
            status: match receipt.exit_code {
                Some(0) => CheckStatus::Pass,
                Some(_) => CheckStatus::Fail,
                None => CheckStatus::Error,
            },
            duration_ms: Some(receipt.duration_ms),
            artifact: None,
        })
        .collect();
    let name_of = |run: &Value| {
        run.get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let ci_status = |run: &Value| {
        if run.get("status").and_then(Value::as_str) != Some("completed") {
            return CheckStatus::Error;
        }
        match run.get("conclusion").and_then(Value::as_str) {
            Some("success") => CheckStatus::Pass,
            _ => CheckStatus::Fail,
        }
    };
    if required_checks.is_empty() {
        checks.extend(ci_runs.iter().map(|run| CheckResult {
            name: truncate(format!("ci:{}", name_of(run))),
            status: ci_status(run),
            duration_ms: None,
            artifact: None,
        }));
    } else {
        for required in required_checks {
            let matching: Vec<CheckStatus> = ci_runs
                .iter()
                .filter(|run| name_of(run) == *required)
                .map(ci_status)
                .collect();
            // Worst status wins; a required check that never reported fails.
            let status = if matching.is_empty() {
                CheckStatus::Fail
            } else if matching.contains(&CheckStatus::Error) {
                CheckStatus::Error
            } else if matching.contains(&CheckStatus::Fail) {
                CheckStatus::Fail
            } else {
                CheckStatus::Pass
            };
            checks.push(CheckResult {
                name: truncate(format!("ci:{required}")),
                status,
                duration_ms: None,
                artifact: None,
            });
        }
    }
    let mut blocking_failures: Vec<String> = checks
        .iter()
        .filter(|check| matches!(check.status, CheckStatus::Fail | CheckStatus::Error))
        .map(|check| check.name.clone())
        .collect();
    if !checks.iter().any(|check| check.status == CheckStatus::Pass) && blocking_failures.is_empty()
    {
        blocking_failures.push("no_verification_evidence".into());
    }
    let passed = blocking_failures.is_empty();
    let report = VerificationReport {
        schema_version: SchemaV1,
        task_id: task_id.to_string(),
        head_sha: head_sha.to_string(),
        passed,
        checks,
        blocking_failures,
        eligible_for_merge: passed,
        human_approval_required: !passed,
    };
    report
        .validate()
        .map_err(|reason| anyhow::anyhow!("invalid_verification_report: {reason}"))?;
    Ok(report)
}

/// Waive command checks that fail on the base commit as well: they are
/// pre-existing, not caused by the change. They are kept in the report as
/// `Skip` with a `[preexisting]` suffix and stop blocking; a check that fails only
/// on the head still blocks, and a report with no passing check still has no
/// evidence.
pub fn waive_preexisting(
    mut report: VerificationReport,
    preexisting: &[Vec<String>],
) -> anyhow::Result<VerificationReport> {
    use super::contracts::Contract;
    if preexisting.is_empty() {
        return Ok(report);
    }
    let names: Vec<String> = preexisting
        .iter()
        .map(|argv| truncate(format!("cmd:{}", argv.join(" "))))
        .collect();
    for check in report.checks.iter_mut() {
        if matches!(check.status, CheckStatus::Fail | CheckStatus::Error) && names.contains(&check.name) {
            check.status = CheckStatus::Skip;
            check.name = truncate(format!("{} [preexisting]", check.name));
        }
    }
    let mut blocking: Vec<String> = report
        .checks
        .iter()
        .filter(|check| matches!(check.status, CheckStatus::Fail | CheckStatus::Error))
        .map(|check| check.name.clone())
        .collect();
    if blocking.is_empty() && !report.checks.iter().any(|check| check.status == CheckStatus::Pass) {
        blocking.push("no_verification_evidence".into());
    }
    report.passed = blocking.is_empty();
    report.eligible_for_merge = report.passed;
    report.human_approval_required = !report.passed;
    report.blocking_failures = blocking;
    report
        .validate()
        .map_err(|reason| anyhow::anyhow!("invalid_verification_report: {reason}"))?;
    Ok(report)
}

/// Check names are capped at 255 characters by the contract.
fn truncate(mut name: String) -> String {
    if name.len() > 255 {
        let mut end = 255;
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        name.truncate(end);
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::contracts::Contract;

    #[test]
    fn preexisting_failures_stop_blocking_but_new_ones_do_not() {
        let receipt = |argv: &[&str], code: i32| VerificationReceipt {
            argv: argv.iter().map(|s| s.to_string()).collect(),
            exit_code: Some(code),
            duration_ms: 1,
        };
        let id = "00000000-0000-4000-8000-000000000001";
        let head = "0123456789abcdef0123456789abcdef01234567";
        let receipts = [receipt(&["npm", "ci"], 0), receipt(&["npm", "run", "test"], 1), receipt(&["npm", "run", "lint"], 1)];
        let report = build_report(id, head, &receipts, &[], &[]).unwrap();
        assert!(!report.passed);
        let lint = vec!["npm".to_string(), "run".into(), "lint".into()];
        let partly = waive_preexisting(report.clone(), &[lint.clone()]).unwrap();
        assert!(!partly.passed);
        assert_eq!(partly.blocking_failures, vec!["cmd:npm run test".to_string()]);
        assert!(partly.checks.iter().any(|c| c.name == "cmd:npm run lint [preexisting]" && c.status == CheckStatus::Skip));
        let test = vec!["npm".to_string(), "run".into(), "test".into()];
        assert!(waive_preexisting(report, &[lint.clone(), test.clone()]).unwrap().passed);
        let nothing_passes = build_report(id, head, &[receipt(&["npm", "run", "lint"], 1)], &[], &[]).unwrap();
        let waived = waive_preexisting(nothing_passes, &[lint]).unwrap();
        assert!(!waived.passed);
        assert_eq!(waived.blocking_failures, vec!["no_verification_evidence".to_string()]);
    }
    use serde_json::json;

    const TASK: &str = "0f9b7c2e-1d2a-4c3b-9e8f-0123456789ab";
    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    fn receipt(argv: &[&str], exit_code: Option<i32>) -> VerificationReceipt {
        VerificationReceipt {
            argv: argv.iter().map(|a| a.to_string()).collect(),
            exit_code,
            duration_ms: 1200,
        }
    }

    fn run(name: &str, status: &str, conclusion: Option<&str>) -> Value {
        json!({"name": name, "status": status, "conclusion": conclusion})
    }

    #[test]
    fn commands_must_be_allowlisted_argv_arrays() {
        for absent in [None, Some(&Value::Null)] {
            assert_eq!(
                parse_verification_commands(absent).unwrap(),
                Vec::<Vec<String>>::new()
            );
        }
        assert_eq!(
            parse_verification_commands(Some(&json!([
                ["npm", "test"],
                ["cargo", "test", "--lib"]
            ])))
            .unwrap(),
            vec![
                vec!["npm".to_string(), "test".into()],
                vec!["cargo".into(), "test".into(), "--lib".into()]
            ]
        );
        for (value, code) in [
            (
                json!([["sh", "-c", "curl evil"]]),
                "command_not_allowlisted",
            ),
            (json!(["npm test"]), "command_must_be_argv"),
            (json!([[]]), "empty_command"),
            (json!([["npm", 1]]), "command_arg_invalid"),
            (json!("npm test"), "verification_commands_invalid"),
            (
                json!(vec![json!(["npm", "test"]); 9]),
                "too_many_verification_commands",
            ),
        ] {
            assert_eq!(
                parse_verification_commands(Some(&value))
                    .unwrap_err()
                    .to_string(),
                code,
                "{value}"
            );
        }
    }

    #[test]
    fn green_commands_and_checks_make_an_eligible_report() {
        let report = build_report(
            TASK,
            SHA,
            &[receipt(&["npm", "test"], Some(0))],
            &["build".into()],
            &[
                run("build", "completed", Some("success")),
                run("lint", "completed", Some("failure")),
            ],
        )
        .unwrap();
        report.validate().unwrap();
        assert!(report.passed && report.eligible_for_merge && !report.human_approval_required);
        assert_eq!(report.head_sha, SHA);
        let names: Vec<&str> = report.checks.iter().map(|c| c.name.as_str()).collect();
        // Only required CI checks count when some are required.
        assert_eq!(names, ["cmd:npm test", "ci:build"]);
    }

    #[test]
    fn any_failure_blocks_and_is_named() {
        let report = build_report(
            TASK,
            SHA,
            &[
                receipt(&["npm", "test"], Some(1)),
                receipt(&["cargo", "test"], None),
            ],
            &["build".into(), "e2e".into()],
            &[run("build", "in_progress", None)],
        )
        .unwrap();
        report.validate().unwrap();
        assert!(!report.passed && !report.eligible_for_merge && report.human_approval_required);
        assert_eq!(
            report.blocking_failures,
            ["cmd:npm test", "cmd:cargo test", "ci:build", "ci:e2e"]
        );
        let status = |name: &str| {
            report
                .checks
                .iter()
                .find(|c| c.name == name)
                .unwrap()
                .status
        };
        assert_eq!(status("cmd:npm test"), CheckStatus::Fail);
        assert_eq!(status("cmd:cargo test"), CheckStatus::Error);
        assert_eq!(
            status("ci:build"),
            CheckStatus::Error,
            "pending is not a pass"
        );
        assert_eq!(
            status("ci:e2e"),
            CheckStatus::Fail,
            "a required check that never ran"
        );
    }

    #[test]
    fn without_required_checks_every_reported_run_counts() {
        let report = build_report(
            TASK,
            SHA,
            &[],
            &[],
            &[
                run("a", "completed", Some("success")),
                run("b", "completed", Some("skipped")),
            ],
        )
        .unwrap();
        assert!(!report.passed, "a skipped CI check is not evidence that it passed");
        assert_eq!(report.checks.len(), 2);
    }

    #[test]
    fn a_report_with_no_evidence_is_not_eligible() {
        let report = build_report(TASK, SHA, &[], &[], &[]).unwrap();
        report.validate().unwrap();
        assert!(!report.passed && !report.eligible_for_merge);
        assert_eq!(report.blocking_failures, ["no_verification_evidence"]);
    }

    #[test]
    fn invalid_identifiers_are_rejected() {
        assert!(build_report("not-a-uuid", SHA, &[], &[], &[]).is_err());
        assert!(build_report(TASK, "abc", &[], &[], &[]).is_err());
    }
}
