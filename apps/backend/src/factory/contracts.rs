//! Wire contracts of the software factory. Mirrors `schemas/factory/*-v1.schema.json`;
//! `tests/factory_contracts.rs` keeps both in agreement over the shared fixtures in
//! `schemas/fixtures/factory/`. Closed enums reject unknown values, unknown fields are
//! rejected, and cross-field rules live in [`Contract::validate`].
//!
//! No payload carries an `org_id`: tenancy always comes from the authenticated
//! context, never from contract data.

use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::num::NonZeroU32;

/// A versioned factory contract: typed parse plus the rules serde cannot express.
pub trait Contract: Serialize + DeserializeOwned + PartialEq {
    fn validate(&self) -> Result<(), String>;
}

/// `"schema_version": 1`. Any other value fails to deserialize, so a v2 payload
/// can never be silently read as v1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SchemaV1;

impl Serialize for SchemaV1 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}

impl<'de> Deserialize<'de> for SchemaV1 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match integral(deserializer)? {
            1 => Ok(SchemaV1),
            other => Err(serde::de::Error::custom(format!(
                "unsupported schema_version {other}"
            ))),
        }
    }
}

/// JSON Schema's `integer` accepts `2.0` (any number with a zero fraction), while
/// serde's integer types reject it. These deserializers accept exactly what the
/// schema accepts so both validators agree; fractional values are still rejected.
fn integral<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let number = serde_json::Number::deserialize(deserializer)?;
    number
        .as_u64()
        .or_else(|| {
            number
                .as_f64()
                .filter(|f| f.fract() == 0.0 && *f >= 0.0 && *f < u64::MAX as f64)
                .map(|f| f as u64)
        })
        .ok_or_else(|| {
            serde::de::Error::custom(format!("expected a non-negative integer, got {number}"))
        })
}

fn opt_u8<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u8>, D::Error> {
    let value = integral(deserializer)?;
    u8::try_from(value)
        .map(Some)
        .map_err(|_| serde::de::Error::custom(format!("{value} out of range")))
}

fn opt_u64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
    integral(deserializer).map(Some)
}

fn nonzero_u32<'de, D: Deserializer<'de>>(deserializer: D) -> Result<NonZeroU32, D::Error> {
    let value = integral(deserializer)?;
    u32::try_from(value)
        .ok()
        .and_then(NonZeroU32::new)
        .ok_or_else(|| serde::de::Error::custom(format!("version {value} must be 1..=4294967295")))
}

// ---------------------------------------------------------------- shared enums

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Fix,
    Publish,
    OpenPr,
    Reply,
    Close,
    Approve,
    Merge,
    Deploy,
    Notify,
    Recover,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskClass {
    Docs,
    Tests,
    Ui,
    Backend,
    Bugfix,
    Refactor,
    Migration,
    Infra,
    Security,
    Unknown,
}

// ---------------------------------------------------------------- TaskSpec

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSpec {
    pub schema_version: SchemaV1,
    pub task_id: String,
    pub source: TaskSource,
    pub origin_trust: OriginTrust,
    pub privacy_class: PrivacyClass,
    pub repository: TaskRepository,
    pub title: String,
    pub description: String,
    pub task_class: TaskClass,
    pub requirements: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSource {
    pub kind: SourceKind,
    #[serde(rename = "ref")]
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    GithubIssue,
    NexusmindTask,
    Slack,
    Sentry,
    Gmail,
    Transcript,
    Manual,
}

impl SourceKind {
    /// Sources whose author can be anyone outside the team. Their content is
    /// always untrusted input (plan D13).
    pub fn is_external(self) -> bool {
        matches!(
            self,
            Self::Slack | Self::Sentry | Self::Gmail | Self::Transcript
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginTrust {
    Trusted,
    Untrusted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    Public,
    Internal,
    Confidential,
    Restricted,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRepository {
    pub remote: String,
    pub base_ref: String,
}

impl Contract for TaskSpec {
    fn validate(&self) -> Result<(), String> {
        canonical_uuid("task_id", &self.task_id)?;
        text("source.ref", &self.source.reference, 1, 512)?;
        if let Some(url) = &self.source.url {
            text("source.url", url, 1, 2048)?;
            reqwest::Url::parse(url).map_err(|_| "source.url: not a URI".to_string())?;
        }
        if self.source.kind.is_external() && self.origin_trust == OriginTrust::Trusted {
            return Err("origin_trust: external sources are always untrusted".into());
        }
        repository_remote(&self.repository.remote)?;
        text("repository.base_ref", &self.repository.base_ref, 1, 255)?;
        text("title", &self.title, 1, 512)?;
        text("description", &self.description, 0, 65536)?;
        items("requirements", &self.requirements)?;
        items("acceptance_criteria", &self.acceptance_criteria)
    }
}

// ---------------------------------------------------------------- RoutingDecision

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingDecision {
    pub schema_version: SchemaV1,
    pub task_id: String,
    pub task_type: String,
    pub execution_tier: ExecutionTier,
    pub risk: f64,
    pub confidence: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blast_radius: Option<BlastRadius>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_factors: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_context: Option<Vec<String>>,
    pub required_checks: Vec<String>,
    #[serde(
        default,
        deserialize_with = "opt_u8",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_attempts: Option<u8>,
    pub human_approval_required: bool,
    pub decided_by: DecidedBy,
    /// Independent per-action verdicts (plan D12). One verdict never implies another.
    pub actions: BTreeMap<Action, ActionVerdict>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionTier {
    Deterministic,
    DecisionModel,
    LocalSmall,
    CheapCloud,
    Frontier,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlastRadius {
    SingleSymbol,
    SingleFile,
    Package,
    Service,
    CrossService,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecidedBy {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionVerdict {
    pub verdict: Verdict,
    pub reason: String,
    pub source: VerdictSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Allow,
    Hold,
    Deny,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictSource {
    Policy,
    Floor,
    DecisionModel,
    Human,
}

impl Contract for RoutingDecision {
    fn validate(&self) -> Result<(), String> {
        canonical_uuid("task_id", &self.task_id)?;
        text("task_type", &self.task_type, 1, 64)?;
        unit_interval("risk", self.risk)?;
        unit_interval("confidence", self.confidence)?;
        if let Some(values) = &self.risk_factors {
            items("risk_factors", values)?;
        }
        if let Some(values) = &self.required_context {
            items("required_context", values)?;
        }
        items("required_checks", &self.required_checks)?;
        if self.max_attempts.is_some_and(|n| n > 3) {
            return Err("max_attempts: must be at most 3".into());
        }
        text("decided_by.provider", &self.decided_by.provider, 1, 64)?;
        if let Some(model) = &self.decided_by.model {
            text("decided_by.model", model, 1, 128)?;
        }
        for verdict in self.actions.values() {
            text("actions.reason", &verdict.reason, 1, 1024)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- ContextPack

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextPack {
    pub schema_version: SchemaV1,
    pub task_id: String,
    pub repository: PackRepository,
    pub artifacts: Vec<ContextArtifact>,
    pub constraints: Vec<String>,
    pub acceptance_tests: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackRepository {
    pub commit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextArtifact {
    pub path: String,
    #[serde(default)]
    pub symbol: Option<String>,
    pub kind: ArtifactKind,
    pub reason: String,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval_score: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Code,
    Test,
    Schema,
    Config,
    Documentation,
    History,
}

impl Contract for ContextPack {
    fn validate(&self) -> Result<(), String> {
        canonical_uuid("task_id", &self.task_id)?;
        full_sha("repository.commit", &self.repository.commit)?;
        if let Some(branch) = &self.repository.branch {
            text("repository.branch", branch, 1, 255)?;
        }
        for artifact in &self.artifacts {
            text("artifacts.path", &artifact.path, 1, 1024)?;
            if let Some(symbol) = &artifact.symbol {
                text("artifacts.symbol", symbol, 0, 512)?;
            }
            text("artifacts.reason", &artifact.reason, 1, 1024)?;
            text("artifacts.content_hash", &artifact.content_hash, 1, 128)?;
        }
        items("constraints", &self.constraints)?;
        items("acceptance_tests", &self.acceptance_tests)
    }
}

// ---------------------------------------------------------------- CodeChangeProposal

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeChangeProposal {
    pub schema_version: SchemaV1,
    pub task_id: String,
    pub summary: String,
    pub operations: Vec<FileOperation>,
    pub assumptions: Vec<String>,
    pub verification_plan: Vec<String>,
    pub needs_more_context: bool,
    pub requires_human_approval: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileOperation {
    pub path: String,
    pub action: FileAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub rationale: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    Create,
    Modify,
    Delete,
}

impl Contract for CodeChangeProposal {
    fn validate(&self) -> Result<(), String> {
        canonical_uuid("task_id", &self.task_id)?;
        text("summary", &self.summary, 1, 4096)?;
        for operation in &self.operations {
            text("operations.path", &operation.path, 1, 1024)?;
            if let Some(symbol) = &operation.symbol {
                text("operations.symbol", symbol, 0, 512)?;
            }
            text("operations.rationale", &operation.rationale, 1, 2048)?;
        }
        items("assumptions", &self.assumptions)?;
        items("verification_plan", &self.verification_plan)
    }
}

// ---------------------------------------------------------------- VerificationReport

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationReport {
    pub schema_version: SchemaV1,
    pub task_id: String,
    /// The exact commit the evidence applies to.
    pub head_sha: String,
    pub passed: bool,
    pub checks: Vec<CheckResult>,
    pub blocking_failures: Vec<String>,
    pub eligible_for_merge: bool,
    pub human_approval_required: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    #[serde(
        default,
        deserialize_with = "opt_u64",
        skip_serializing_if = "Option::is_none"
    )]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub artifact: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CheckStatus {
    Pass,
    Fail,
    Skip,
    Error,
}

impl Contract for VerificationReport {
    fn validate(&self) -> Result<(), String> {
        canonical_uuid("task_id", &self.task_id)?;
        full_sha("head_sha", &self.head_sha)?;
        for check in &self.checks {
            text("checks.name", &check.name, 1, 255)?;
            if let Some(artifact) = &check.artifact {
                text("checks.artifact", artifact, 0, 1024)?;
            }
        }
        items("blocking_failures", &self.blocking_failures)?;
        if self.eligible_for_merge && (!self.passed || !self.blocking_failures.is_empty()) {
            return Err("eligible_for_merge: requires passed and no blocking failures".into());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- ActionPolicy

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionPolicy {
    pub schema_version: SchemaV1,
    pub action: Action,
    pub mode: PolicyMode,
    pub scope: PolicyScope,
    pub allow: Vec<String>,
    pub stop: Vec<String>,
    #[serde(deserialize_with = "nonzero_u32")]
    pub version: NonZeroU32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyMode {
    Never,
    Manual,
    Criteria,
    AfterFix,
    AfterMerge,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyScope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_class: Option<TaskClass>,
}

impl Contract for ActionPolicy {
    fn validate(&self) -> Result<(), String> {
        if let Some(project) = &self.scope.project {
            text("scope.project", project, 1, 255)?;
        }
        items("allow", &self.allow)?;
        items("stop", &self.stop)?;
        if self.mode == PolicyMode::Criteria && self.allow.is_empty() {
            return Err("allow: criteria mode needs at least one allow condition".into());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- field rules
// Each mirrors a JSON Schema keyword so both validators reject the same payloads.

fn text(field: &str, value: &str, min: usize, max: usize) -> Result<(), String> {
    let len = value.chars().count();
    if len < min || len > max {
        return Err(format!("{field}: length must be {min}..={max}"));
    }
    Ok(())
}

/// Array items with `minLength: 1`.
fn items(field: &str, values: &[String]) -> Result<(), String> {
    if values.iter().any(String::is_empty) {
        return Err(format!("{field}: items must not be empty"));
    }
    Ok(())
}

fn unit_interval(field: &str, value: f64) -> Result<(), String> {
    if !(0.0..=1.0).contains(&value) {
        return Err(format!("{field}: must be within 0..=1"));
    }
    Ok(())
}

/// Lowercase hyphenated UUID, the only form the schema pattern accepts.
fn canonical_uuid(field: &str, value: &str) -> Result<(), String> {
    match uuid::Uuid::parse_str(value) {
        Ok(parsed) if parsed.hyphenated().to_string() == value => Ok(()),
        _ => Err(format!("{field}: must be a lowercase hyphenated UUID")),
    }
}

fn full_sha(field: &str, value: &str) -> Result<(), String> {
    if value.len() == 40
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err(format!("{field}: must be a 40-char lowercase hex SHA"))
    }
}

fn repository_remote(value: &str) -> Result<(), String> {
    let valid = value.split_once('/').is_some_and(|(owner, repo)| {
        !owner.is_empty()
            && !repo.is_empty()
            && owner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            && repo
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    });
    if valid {
        Ok(())
    } else {
        Err("repository.remote: must be owner/repo".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../schemas/fixtures/factory"
    );

    fn reason<T: Contract>(fixture: &str) -> String {
        let text = std::fs::read_to_string(format!("{FIXTURES}/{fixture}")).unwrap();
        let value: T =
            serde_json::from_str(&text).expect("fixture must parse; rule is cross-field");
        value.validate().unwrap_err()
    }

    #[test]
    fn external_sources_can_never_be_trusted() {
        assert!(
            reason::<TaskSpec>("task-spec/v1/invalid/trusted-gmail.json")
                .starts_with("origin_trust:")
        );
    }

    #[test]
    fn merge_eligibility_requires_a_clean_passing_report() {
        for fixture in ["eligible-but-failed.json", "eligible-with-blocking.json"] {
            assert!(
                reason::<VerificationReport>(&format!("verification-report/v1/invalid/{fixture}"))
                    .starts_with("eligible_for_merge:"),
                "{fixture}"
            );
        }
    }

    #[test]
    fn criteria_policies_need_allow_conditions() {
        assert!(
            reason::<ActionPolicy>("action-policy/v1/invalid/criteria-without-allow.json")
                .starts_with("allow:")
        );
    }

    #[test]
    fn range_rules_fail_for_the_right_field() {
        assert!(
            reason::<RoutingDecision>("routing-decision/v1/invalid/risk-out-of-range.json")
                .starts_with("risk:")
        );
        assert!(
            reason::<RoutingDecision>("routing-decision/v1/invalid/too-many-attempts.json")
                .starts_with("max_attempts:")
        );
        assert!(
            reason::<VerificationReport>("verification-report/v1/invalid/short-sha.json")
                .starts_with("head_sha:")
        );
        assert!(
            reason::<CodeChangeProposal>("code-change-proposal/v1/invalid/empty-summary.json")
                .starts_with("summary:")
        );
    }

    #[test]
    fn a_v2_payload_is_never_read_as_v1() {
        let text = std::fs::read_to_string(format!(
            "{FIXTURES}/task-spec/v1/invalid/wrong-version.json"
        ))
        .unwrap();
        let error = serde_json::from_str::<TaskSpec>(&text)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unsupported schema_version 2"), "{error}");
    }
}
