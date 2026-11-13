//! Durable workspace workflow contracts. Repository content never grants authority.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repository {
    pub path: String,
    pub remote: String,
    pub head_sha: String,
    /// Confirmed upstream branch; absent in legacy pinned-only inventories.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub writable: bool,
}

pub fn relative_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && !path.contains('\\')
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
        && !path.chars().any(char::is_control)
}

pub fn validate_repositories(repos: &[Repository]) -> Result<(), String> {
    if repos.is_empty() || repos.len() > 100 {
        return Err("Select between 1 and 100 repositories".into());
    }
    let mut paths = BTreeSet::new();
    for repo in repos {
        if !relative_path(&repo.path) || !paths.insert(&repo.path) {
            return Err("Repository paths must be unique safe relative paths".into());
        }
        if ![40, 64].contains(&repo.head_sha.len())
            || !repo.head_sha.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("Repository HEAD must be a full commit SHA".into());
        }
        if repo.branch.as_ref().is_some_and(|b| b.is_empty() || b.len() > 255
            || b.starts_with('-') || b.starts_with('/') || b.ends_with('/')
            || b.ends_with('.') || b.ends_with(".lock") || b.contains("..")
            || b.contains("//") || b.contains("@{") || b == "HEAD"
            || b.chars().any(|c| c.is_whitespace() || c.is_control() || "~^:?*[\\".contains(c))) {
            return Err("Repository branch must be a safe named branch".into());
        }
        // Never accept URL credentials,
        // query parameters or fragments (they frequently carry access tokens).
        if repo.remote.len() > 1024
            || repo.remote.chars().any(char::is_control)
            || repo
                .remote
                .split_once("://")
                .is_some_and(|(_, p)| p.contains('@') || p.contains('?') || p.contains('#'))
        {
            return Err("Remote must not contain credentials".into());
        }
    }
    for a in repos {
        if a.path.split('/').any(|p| p == ".git")
            || repos
                .iter()
                .any(|b| b.path != a.path && b.path.starts_with(&format!("{}/", a.path)))
        {
            return Err("Repository paths cannot overlap or include Git metadata".into());
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Prepare,
    Agent,
    Check,
    Gate,
    Publish,
    Review,
    Notify,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub id: String,
    pub name: String,
    pub kind: StepKind,
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// A local, operator-installed adapter, never an arbitrary command from an issue.
    pub adapter: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub write_repositories: Vec<String>,
    #[serde(default = "one")]
    pub max_attempts: u32,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u32,
    /// Task instructions and required check names; never shell commands or credentials.
    #[serde(default)]
    pub options: serde_json::Value,
}
impl Step {
    /// Only the three dedicated, read-only QA phases may report findings without
    /// blocking publication. Build/tests and independent review stay blocking.
    pub fn qa_check(&self) -> Option<&'static str> {
        if !matches!(self.kind, StepKind::Check) || !self.write_repositories.is_empty() {
            return None;
        }
        let check = match self.adapter.as_str() {
            "isolated-functional-qa" => "functional",
            "isolated-design-qa" => "design",
            "isolated-security-qa" => "security",
            _ => return None,
        };
        let required = self.options.get("required_checks")?.as_array()?;
        (required.len() == 1 && required[0].as_str() == Some(check)).then_some(check)
    }
}
fn one() -> u32 {
    1
}
fn default_timeout() -> u32 {
    900
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub name: String,
    pub workspace_id: String,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub repair_step: Option<String>,
    #[serde(default = "two")]
    pub max_repairs: u32,
    #[serde(default = "default_cost")]
    pub max_cost_usd: f64,
}
fn two() -> u32 {
    2
}
fn default_cost() -> f64 {
    20.0
}

impl Definition {
    pub fn validate(&self, repos: &[Repository]) -> Result<(), String> {
        if self.name.trim().is_empty()
            || self.name.len() > 120
            || self.steps.is_empty()
            || self.steps.len() > 64
            || self.max_repairs > 2
            || !self.max_cost_usd.is_finite()
            || self.max_cost_usd <= 0.0
            || self.max_cost_usd > 1000.0
        {
            return Err("A workflow needs a name and 1–64 steps".into());
        }
        let mut known = BTreeSet::new();
        for step in &self.steps {
            if !relative_path(&step.id)
                || !step
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
                || !known.insert(step.id.clone())
                || step.name.trim().is_empty()
                || step.name.len() > 120
                || !relative_path(&step.adapter)
                || !step
                    .adapter
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
                || !(1..=3).contains(&step.max_attempts)
                || !(10..=7200).contains(&step.timeout_seconds)
                || step.capabilities.len() > 16
                || step
                    .capabilities
                    .iter()
                    .any(|c| c.is_empty() || c.len() > 100 || c.chars().any(char::is_control))
            {
                return Err("Invalid step id, name, adapter, retry limit or timeout".into());
            }
            for path in &step.write_repositories {
                if !matches!(step.kind, StepKind::Agent)
                    || !repos.iter().any(|r| &r.path == path && r.writable)
                {
                    return Err(
                        "Only implementation agents may write explicitly authorized repositories"
                            .into(),
                    );
                }
            }
            if matches!(step.kind, StepKind::Review) && !step.write_repositories.is_empty() {
                return Err("Review steps must be read-only".into());
            }
        }
        if self.repair_step.as_ref().is_some_and(|id| {
            !self.steps.iter().any(|s| {
                &s.id == id && matches!(s.kind, StepKind::Agent) && !s.write_repositories.is_empty()
            })
        }) {
            return Err("Repair target must be a write-capable agent".into());
        }
        let mut done = BTreeSet::new();
        loop {
            let previous = done.len();
            for step in &self.steps {
                let deps: BTreeSet<_> = step.depends_on.iter().collect();
                if deps.len() != step.depends_on.len()
                    || deps.contains(&step.id)
                    || !deps.iter().all(|d| known.contains(*d))
                {
                    return Err("Dependencies must be unique existing steps, excluding self".into());
                }
                if step.depends_on.iter().all(|d| done.contains(d)) {
                    done.insert(step.id.clone());
                }
            }
            if done.len() == self.steps.len() {
                return Ok(());
            }
            if previous == done.len() {
                return Err("Workflow contains a dependency cycle".into());
            }
        }
    }
}

/// A reproducible evidence reference; no arbitrary raw log or secret payloads.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub check: String,
    pub status: EvidenceStatus,
    pub commits: BTreeMap<String, String>,
    pub artifact_digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    Pass,
    Fail,
    NotApplicable,
    Blocked,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    #[serde(default)]
    pub input_revision: u32,
    #[serde(default)]
    pub heads: BTreeMap<String, String>,
    #[serde(default)]
    pub output: serde_json::Value,
    #[serde(default)]
    pub cost_usd: f64,
}

pub fn valid_sha(sha: &str) -> bool {
    [40, 64].contains(&sha.len()) && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn repo() -> Repository {
        Repository {
            path: "api".into(),
            remote: "git@github.com:acme/api.git".into(),
            head_sha: "a".repeat(40),
            branch: None,
            writable: true,
        }
    }
    fn step(id: &str, deps: &[&str]) -> Step {
        Step {
            id: id.into(),
            name: id.into(),
            kind: StepKind::Agent,
            depends_on: deps.iter().map(|v| v.to_string()).collect(),
            adapter: "isolated-agent".into(),
            capabilities: vec![],
            write_repositories: vec![],
            max_attempts: 1,
            timeout_seconds: 900,
            options: serde_json::Value::Null,
        }
    }
    #[test]
    fn rejects_cycles_unknown_dependencies_and_escalation() {
        let mut d = Definition {
            name: "Fix".into(),
            workspace_id: "w".into(),
            steps: vec![step("a", &["b"]), step("b", &["a"])],
            repair_step: None,
            max_repairs: 2,
            max_cost_usd: 20.0,
        };
        assert!(d.validate(&[repo()]).is_err());
        d.steps = vec![step("a", &["missing"])];
        assert!(d.validate(&[repo()]).is_err());
        d.steps = vec![step("a", &[])];
        d.steps[0].write_repositories.push("other".into());
        assert!(d.validate(&[repo()]).is_err());
        d.steps[0].write_repositories = vec!["api".into()];
        assert!(d.validate(&[repo()]).is_ok());
        d.steps[0].kind = StepKind::Review;
        assert!(d.validate(&[repo()]).is_err());
    }
    #[test]
    fn paths_and_inventory_are_safe() {
        for p in ["../api", "/tmp/api", "a/../b", "a\\b", "a//b", "a\n"] {
            assert!(!relative_path(p));
        }
        assert!(validate_repositories(&[repo()]).is_ok());
        let mut r = repo();
        r.remote = "https://token@github.com/a/b".into();
        assert!(validate_repositories(&[r]).is_err());
    }
}
