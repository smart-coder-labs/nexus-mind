//! Jev-assisted workflow decisions: which checks a change actually needs, and
//! which phase (if any) can resolve a failed one.
//!
//! Deterministic floors come first and Jev cannot override them: a repository
//! with code changes always runs its checks, a repository that depends on a
//! changed package runs too, and a change set without code (documentation only,
//! or nothing at all) runs nothing. Jev only decides the remaining checks. Any
//! Jev failure falls back to running the check, never to skipping it.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Below this probability that a check is needed, Jev may skip it.
pub const RUN_THRESHOLD: f64 = 0.35;
/// Minimum confidence for Jev to send a failure back to another phase.
pub const RECOVERY_CONFIDENCE: f64 = 0.6;
/// Checks per Jev call; larger plans are split into parallel batches.
pub const MAX_QUESTIONS: usize = 40;
pub const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Active Jev decisions need both the provider key and an explicit per-org
/// opt-in (`FACTORY_JEV_DECISION_ORGS`), separate from shadow mode.
pub fn decision_key(org: &str) -> Option<String> {
    let listed = std::env::var("FACTORY_JEV_DECISION_ORGS")
        .unwrap_or_default()
        .split(',')
        .any(|id| !id.trim().is_empty() && id.trim() == org);
    if !listed {
        return None;
    }
    std::env::var("TYPESAFE_API_KEY").ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct CheckRef {
    pub name: String,
    pub cwd: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepoInfo {
    pub path: String,
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Package names this repository depends on.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Paths changed by the run in this repository (relative to its root).
    #[serde(default)]
    pub changed_paths: Vec<String>,
    /// Code files whose diff only adds or removes comments and blank lines.
    /// Context for Jev; never counted as a code change by the floors.
    #[serde(default)]
    pub comment_only_paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Decision {
    pub check: CheckRef,
    pub run: bool,
    pub source: &'static str,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probability: Option<f64>,
}

/// Documentation and metadata paths that cannot change runtime behavior.
pub fn documentation_only(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let documentation_dir = lower.starts_with("docs/") || lower.starts_with("doc/") || lower.contains("/docs/");
    let documentation_file = [".md", ".mdx", ".markdown", ".rst", ".adoc", ".txt"].iter().any(|e| name.ends_with(e));
    let metadata = ["license", "licence", "codeowners", "authors", "notice"].iter().any(|n| name == *n || name.starts_with(&format!("{n}.")));
    documentation_dir || documentation_file || metadata
}

fn owner<'a>(cwd: &str, repos: &'a [RepoInfo]) -> Option<&'a RepoInfo> {
    repos
        .iter()
        .filter(|r| cwd == r.path || cwd.starts_with(&format!("{}/", r.path.trim_end_matches('/'))))
        .max_by_key(|r| r.path.len())
}

/// Floors that Jev cannot override. Returns decided checks and the rest.
pub fn floors(checks: &[CheckRef], repos: &[RepoInfo]) -> (Vec<Decision>, Vec<CheckRef>) {
    let changed: BTreeSet<&str> = repos
        .iter()
        .filter(|r| r.changed_paths.iter().any(|p| !documentation_only(p)))
        .map(|r| r.path.as_str())
        .collect();
    let changed_packages: BTreeSet<&str> = repos
        .iter()
        .filter(|r| changed.contains(r.path.as_str()))
        .filter_map(|r| r.package.as_deref())
        .collect();
    let mut decided = Vec::new();
    let mut open = Vec::new();
    for check in checks {
        let repo = owner(&check.cwd, repos);
        let decision = |run: bool, reason: &str| Decision { check: check.clone(), run, source: "floor", reason: reason.into(), probability: None };
        if changed.is_empty() {
            decided.push(decision(false, "Ningún repositorio tiene cambios de código (solo documentación o ninguno)."));
        } else if repo.is_some_and(|r| changed.contains(r.path.as_str())) {
            decided.push(decision(true, "El repositorio tiene cambios de código."));
        } else if repo.is_some_and(|r| r.dependencies.iter().any(|d| changed_packages.contains(d.as_str()))) {
            decided.push(decision(true, "Depende de un paquete modificado en este cambio."));
        } else {
            open.push(check.clone());
        }
    }
    (decided, open)
}

fn truncate(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn repository_state(repos: &[RepoInfo]) -> Value {
    json!(repos
        .iter()
        .take(120)
        .map(|r| json!({
            "repository": r.path,
            "package": r.package,
            "responsibility": r.description.as_deref().map(|d| truncate(d, 300)),
            "changed_paths": r.changed_paths.iter().take(50).collect::<Vec<_>>(),
            "comment_only_paths": r.comment_only_paths.iter().take(50).collect::<Vec<_>>(),
        }))
        .collect::<Vec<_>>())
}

pub fn validation_request(task: &Value, repos: &[RepoInfo], open: &[CheckRef]) -> Value {
    let mut questions = serde_json::Map::new();
    for (index, check) in open.iter().take(MAX_QUESTIONS).enumerate() {
        questions.insert(
            format!("q{index}"),
            json!({
                "type": "noul",
                "instructions": format!(
                    "Is running the `{}` check in repository `{}` needed to validate this change? Consider the changed repositories, their paths and whether `{}` can be affected.",
                    check.name, check.cwd, check.cwd
                ),
                "criteria": {"true": "Needed: the change can affect this check", "false": "Not needed: the change cannot affect it"},
            }),
        );
    }
    json!({
        "model": crate::factory::jev::MODEL,
        "state": {
            "title": truncate(task["title"].as_str().unwrap_or(""), 300),
            "description": truncate(task["acceptance_criteria"].as_str().unwrap_or(""), 2000),
            "repositories": repository_state(repos),
        },
        "questions": questions,
    })
}

/// Applies Jev's answers. A missing or invalid answer runs the check.
pub fn apply_validation(open: &[CheckRef], answers: Option<&Value>) -> Vec<Decision> {
    open.iter()
        .enumerate()
        .map(|(index, check)| {
            let probability = (index < MAX_QUESTIONS)
                .then(|| answers.and_then(|a| a.pointer(&format!("/answers/q{index}/noul"))).and_then(Value::as_f64))
                .flatten()
                .filter(|p| (0.0..=1.0).contains(p));
            match probability {
                Some(p) if p < RUN_THRESHOLD => Decision { check: check.clone(), run: false, source: "jev", reason: "Jev: el cambio no puede afectar este check.".into(), probability: Some(p) },
                Some(p) => Decision { check: check.clone(), run: true, source: "jev", reason: "Jev: el cambio puede afectar este check.".into(), probability: Some(p) },
                None => Decision { check: check.clone(), run: true, source: "fallback", reason: "Sin respuesta válida de Jev; se ejecuta por seguridad.".into(), probability: None },
            }
        })
        .collect()
}

/// Failures no phase can fix by running again: re-running only spends the
/// repair budget. They block immediately with their reason.
pub const INFRASTRUCTURE_FAILURES: [&str; 1] = ["model_unavailable"];

pub fn infrastructure_failure(step_result: &Value) -> Option<&str> {
    let code = step_result.pointer("/output/error").and_then(Value::as_str)?;
    INFRASTRUCTURE_FAILURES.contains(&code).then_some(code)
}

/// A phase Jev may send a failure back to.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub id: String,
    pub description: String,
}

pub fn recovery_request(task: &Value, failed_step: &str, failure: &Value, candidates: &[Candidate]) -> Value {
    let mut criteria = serde_json::Map::new();
    for c in candidates {
        criteria.insert(c.id.clone(), json!(c.description));
    }
    criteria.insert(
        "none".into(),
        json!("No phase can fix it: it needs infrastructure, environment configuration, credentials or a person"),
    );
    json!({
        "model": crate::factory::jev::MODEL,
        "state": {
            "title": truncate(task["title"].as_str().unwrap_or(""), 300),
            "description": truncate(task["acceptance_criteria"].as_str().unwrap_or(""), 1500),
            "failed_phase": failed_step,
            "failure": failure,
        },
        "questions": {
            "recovery": {
                "type": "choice",
                "instructions": "A workflow phase failed. Which phase can resolve this failure if it runs again with the failure as feedback?",
                "criteria": criteria,
            }
        },
    })
}

/// Jev's recovery choice when valid and confident enough.
pub fn apply_recovery(answers: &Value, candidates: &[Candidate]) -> (Option<String>, Option<f64>) {
    let choice = answers.pointer("/answers/recovery/choice").and_then(Value::as_str);
    let confidence = answers.pointer("/answers/recovery/confidence").and_then(Value::as_f64).filter(|c| (0.0..=1.0).contains(c));
    match (choice, confidence) {
        (Some(id), Some(c)) if c >= RECOVERY_CONFIDENCE && candidates.iter().any(|x| x.id == id) => (Some(id.to_string()), Some(c)),
        (_, c) => (None, c),
    }
}

/// Owned request for concurrent batches.
pub fn validation_request_owned(task: &Value, repos: &[RepoInfo], batch: &[CheckRef]) -> Value {
    validation_request(task, repos, batch)
}

pub async fn ask(key: &str, request: impl std::borrow::Borrow<Value>) -> anyhow::Result<Value> {
    let request = request.borrow();
    let response = reqwest::Client::new()
        .post(crate::factory::jev::ENDPOINT)
        .bearer_auth(key)
        .timeout(TIMEOUT)
        .json(request)
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("jev_http_{}", status.as_u16());
    }
    Ok(response.json().await?)
}

/// Summaries per check for the decisions event (no paths beyond the request).
pub fn summary(decisions: &[Decision]) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for d in decisions {
        *counts.entry(if d.run { "run" } else { "skip" }).or_insert(0) += 1;
        *counts.entry(d.source).or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(path: &str, package: &str, deps: &[&str], changed: &[&str]) -> RepoInfo {
        RepoInfo {
            path: path.into(),
            package: Some(package.into()),
            description: None,
            dependencies: deps.iter().map(|d| d.to_string()).collect(),
            changed_paths: changed.iter().map(|p| p.to_string()).collect(),
            comment_only_paths: Vec::new(),
        }
    }
    fn check(name: &str, cwd: &str) -> CheckRef {
        CheckRef { name: name.into(), cwd: cwd.into() }
    }

    #[test]
    fn documentation_only_changes_run_nothing() {
        let repos = [repo("api", "@x/api", &[], &["README.md", "docs/flow.mdx"]), repo("web", "@x/web", &[], &[])];
        let (decided, open) = floors(&[check("tests", "api"), check("build", "web")], &repos);
        assert!(open.is_empty());
        assert!(decided.iter().all(|d| !d.run && d.source == "floor"));
        assert!(documentation_only("LICENSE") && !documentation_only("src/docs.ts"));
    }

    #[test]
    fn comment_only_code_changes_do_not_force_checks() {
        let mut api = repo("api", "@x/api", &[], &["docs/design.md"]);
        api.comment_only_paths = vec!["src/refund.ts".into()];
        let (decided, open) = floors(&[check("tests", "api")], &[api]);
        assert!(open.is_empty() && !decided[0].run);
    }

    #[test]
    fn changed_and_dependent_repositories_always_run_and_the_rest_ask_jev() {
        let repos = [
            repo("domain", "@x/domain", &[], &["src/sale.ts"]),
            repo("sales", "@x/sales", &["@x/domain"], &[]),
            repo("audit", "@x/audit", &[], &[]),
        ];
        let checks = [check("tests", "domain"), check("build", "sales/app"), check("tests", "audit")];
        let (decided, open) = floors(&checks, &repos);
        assert_eq!(decided.iter().filter(|d| d.run).count(), 2);
        assert_eq!(open, vec![check("tests", "audit")]);
    }

    #[test]
    fn jev_can_only_skip_with_a_valid_low_probability() {
        let open = [check("tests", "a"), check("tests", "b"), check("tests", "c")];
        let answers = json!({"answers": {"q0": {"type": "noul", "noul": 0.1}, "q1": {"type": "noul", "noul": 0.8}, "q2": {"type": "noul", "noul": 7}}});
        let decisions = apply_validation(&open, Some(&answers));
        assert_eq!(decisions.iter().map(|d| d.run).collect::<Vec<_>>(), vec![false, true, true]);
        assert_eq!(decisions[2].source, "fallback");
        assert!(apply_validation(&open, None).iter().all(|d| d.run));
    }

    #[test]
    fn an_unavailable_model_is_infrastructure_not_a_phase_to_retry() {
        assert_eq!(infrastructure_failure(&json!({"output": {"error": "model_unavailable"}})), Some("model_unavailable"));
        assert_eq!(infrastructure_failure(&json!({"output": {"error": "agent_invalid_json"}})), None);
        assert_eq!(infrastructure_failure(&Value::Null), None);
    }

    #[test]
    fn recovery_needs_a_known_candidate_and_confidence() {
        let candidates = [Candidate { id: "implement".into(), description: "x".into() }];
        let pick = |choice: &str, confidence: f64| apply_recovery(&json!({"answers": {"recovery": {"choice": choice, "confidence": confidence}}}), &candidates).0;
        assert_eq!(pick("implement", 0.9), Some("implement".into()));
        assert_eq!(pick("implement", 0.3), None);
        assert_eq!(pick("none", 0.99), None);
        assert_eq!(pick("publish", 0.99), None);
    }
}
