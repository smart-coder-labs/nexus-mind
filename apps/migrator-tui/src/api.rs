//! The backend client for everything the runner does not do: reading the
//! review queue, deciding on candidates, and committing what was approved.
//!
//! The runner stages. A human decides. This module carries the decisions.
//! It is deliberately a separate path from the NDJSON stream — approval is not
//! something a background process should ever be able to do on its own.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub source_identity: String,
    pub destination_kind: String,
    pub content: String,
    #[serde(default)]
    pub destination_hint: serde_json::Value,
    #[serde(default)]
    pub source_excerpt: Option<String>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub attestation: serde_json::Value,
    pub provenance_kind: String,
    pub status: String,
    pub version: i64,
}

impl Candidate {
    /// A human-readable title, best-effort from the hint the connector attached.
    pub fn title(&self) -> String {
        self.destination_hint
            .get("title")
            .and_then(|t| t.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| self.source_identity.clone())
    }

    /// Whether approving this one carries a second gate.
    ///
    /// A harness candidate is executable configuration and a client-attested
    /// candidate carries someone's name on a legal agreement. Neither may ride
    /// along in a batch approval — the operator has to look at it.
    pub fn needs_individual_review(&self) -> bool {
        self.destination_kind == "harness"
            || self
                .attestation
                .get("client_attested")
                .map(|v| !v.is_null())
                .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ReviewResponse {
    pub applied: usize,
    pub conflicts: usize,
    #[serde(default)]
    pub results: Vec<ReviewResultEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewResultEntry {
    pub candidate_id: String,
    /// `applied` when the decision took effect. Anything else did not — the
    /// live backend answers `stale_version` for a lost race, which is why
    /// callers test for *not* `applied` rather than for a list of failure
    /// names they would have to keep in sync.
    pub outcome: String,
    // The response also carries `actual_version` on a conflict. It is not read
    // here on purpose: a conflict always reloads the queue, which returns the
    // authoritative versions anyway. Keeping a second copy could only ever go
    // stale.
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CommitResponse {
    pub committed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub indexed: usize,
    pub pending_index: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Run {
    pub id: String,
    pub source_kind: String,
    pub status: String,
    #[serde(default)]
    pub source_ref: Option<String>,
    pub created_at: String,
    // The rest of what the listing already returns. Carried so the runs screen
    // can show a run in full without a second round-trip per selection.
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub runner_version: Option<String>,
    #[serde(default)]
    pub created_by: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub attestation: serde_json::Value,
}

/// A backend project, as returned by `GET /v1/projects`.
///
/// Only the fields the monorepo planner needs are carried; the backend sends
/// more (`org_id`, `created_at`), and `#[serde(default)]` on the rest keeps a
/// widened response from breaking the picker. Matching a detected sub-package
/// to an existing project is done by `name`, which is unique per organization.
#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub client_id: Option<String>,
    /// Non-null when the project is soft-archived. An archived project is a poor
    /// migration target, so the planner shows it but does not match to it.
    #[serde(default)]
    pub archived_at: Option<String>,
}

impl Project {
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

/// A reviewer's decision.
///
/// The wire values are `approved` / `rejected` / `restaged` — past tense. An
/// earlier version of this file sent `approve` and `reject`, which the backend
/// rejects wholesale as an undeserializable body: no partial application, no
/// useful message, just `invalid_json`. A typed enum with one place that spells
/// the strings is what stops that from coming back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Approved,
    Rejected,
    /// Back to the queue, undecided — the honest option when a candidate needs
    /// someone else's eyes.
    Restaged,
}

impl Verdict {
    pub fn wire(self) -> &'static str {
        match self {
            Verdict::Approved => "approved",
            Verdict::Rejected => "rejected",
            Verdict::Restaged => "restaged",
        }
    }

    pub fn past_tense(self) -> &'static str {
        match self {
            Verdict::Approved => "approved",
            Verdict::Rejected => "rejected",
            Verdict::Restaged => "sent back to the queue",
        }
    }
}

pub struct Client {
    http: reqwest::blocking::Client,
    base: String,
    key: String,
}

impl Client {
    pub fn new(base: &str, key: &str) -> Result<Self> {
        Ok(Self {
            http: reqwest::blocking::Client::builder()
                // A hung backend must not look like a hung TUI.
                .timeout(Duration::from_secs(120))
                .build()?,
            base: base.trim_end_matches('/').to_string(),
            key: key.to_string(),
        })
    }

    fn url(&self, tail: &str) -> String {
        format!("{}{}", self.base, tail)
    }

    /// Confirms the backend is reachable and the key is accepted, before the
    /// operator invests minutes in a scan that would fail at the last step.
    pub fn probe(&self) -> Result<String> {
        let resp = self
            .http
            .get(self.url("/v1/migrations?limit=1"))
            .bearer_auth(&self.key)
            .send()
            .context("the backend did not answer")?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            anyhow::bail!("the backend rejected this API key ({status})");
        }
        resp.error_for_status()
            .context("the backend answered with an error")?;
        Ok(format!("connected to {} ({status})", self.base))
    }

    pub fn runs(&self) -> Result<Vec<Run>> {
        let value: serde_json::Value = self
            .http
            .get(self.url("/v1/migrations?limit=50"))
            .bearer_auth(&self.key)
            .send()?
            .error_for_status()?
            .json()?;
        // The endpoint has returned both a bare array and an envelope over its
        // life; accept either rather than break on the one that shows up.
        let items = value
            .get("runs")
            .or_else(|| value.get("items"))
            .cloned()
            .unwrap_or(value);
        Ok(serde_json::from_value(items).unwrap_or_default())
    }

    pub fn candidates(&self, run_id: &str, status: &str) -> Result<Vec<Candidate>> {
        let mut list: Vec<Candidate> = self
            .http
            .get(self.url(&format!(
                "/v1/migrations/{run_id}/candidates?status={status}&limit=500"
            )))
            .bearer_auth(&self.key)
            .send()?
            .error_for_status()?
            .json()?;
        // Lowest confidence first: the queue should open on the candidates that
        // most need a human, not on the easy ones.
        list.sort_by(|a, b| {
            a.confidence
                .unwrap_or(0.0)
                .total_cmp(&b.confidence.unwrap_or(0.0))
        });
        Ok(list)
    }

    /// Applies decisions. `expected_version` is what makes this safe to run
    /// against a queue somebody else is also reviewing: a candidate that moved
    /// under us comes back as a conflict instead of being silently overwritten.
    pub fn review(
        &self,
        run_id: &str,
        actions: &[(String, Verdict, i64)],
    ) -> Result<ReviewResponse> {
        let body = serde_json::json!({
            "actions": actions
                .iter()
                .map(|(id, verdict, version)| serde_json::json!({
                    "candidate_id": id,
                    "action": verdict.wire(),
                    "expected_version": version,
                }))
                .collect::<Vec<_>>()
        });
        Ok(self
            .http
            .post(self.url(&format!("/v1/migrations/{run_id}/review")))
            .bearer_auth(&self.key)
            .json(&body)
            .send()?
            .error_for_status()?
            .json()?)
    }

    /// Cancels a run's pending candidates.
    ///
    /// This is the safe way to retire a run. There is deliberately no delete:
    /// `migration_provenance.run_id` is `ON DELETE CASCADE`, so removing a run
    /// would take the provenance of everything it committed with it — the exact
    /// audit trail the migration pipeline exists to produce.
    pub fn cancel(&self, run_id: &str) -> Result<usize> {
        let resp = self
            .http
            .post(self.url(&format!("/v1/migrations/{run_id}/cancel")))
            .bearer_auth(&self.key)
            .json(&serde_json::json!({}))
            .send()?;
        if resp.status() == reqwest::StatusCode::BAD_REQUEST {
            let body: serde_json::Value = resp.json().unwrap_or_default();
            anyhow::bail!(
                "{}",
                body.get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("this run cannot be cancelled")
            );
        }
        let body: serde_json::Value = resp.error_for_status()?.json()?;
        Ok(body
            .get("cancelled")
            .and_then(|v| v.as_u64())
            .unwrap_or_default() as usize)
    }

    pub fn commit(&self, run_id: &str) -> Result<CommitResponse> {
        Ok(self
            .http
            .post(self.url(&format!("/v1/migrations/{run_id}/commit")))
            .bearer_auth(&self.key)
            .json(&serde_json::json!({}))
            .send()?
            .error_for_status()?
            .json()?)
    }

    /// The projects the monorepo planner matches detected sub-packages against.
    ///
    /// Scoped to one client when the run has one: a migration for `acme` should
    /// only ever match — and never accidentally reuse — a project that belongs
    /// to `acme`. With no client the listing is org-wide, which is what an
    /// internal (u2s) migration wants.
    pub fn projects(&self, client_id: Option<&str>) -> Result<Vec<Project>> {
        let mut url = self.url("/v1/projects");
        if let Some(cid) = client_id.filter(|c| !c.trim().is_empty()) {
            url.push_str(&format!("?client_id={}", cid.trim()));
        }
        let value: serde_json::Value = self
            .http
            .get(url)
            .bearer_auth(&self.key)
            .send()?
            .error_for_status()?
            .json()?;
        // Tolerate either a bare array or an envelope, like `runs` does.
        let items = value
            .get("projects")
            .or_else(|| value.get("items"))
            .cloned()
            .unwrap_or(value);
        Ok(serde_json::from_value(items).unwrap_or_default())
    }

    /// Creates a project for a detected sub-package that had no match.
    ///
    /// `client_id` owns it (the run's client, or `None` for internal work);
    /// `parent_id` is set when the operator asked for the repo to be a parent
    /// project. A name that already exists comes back as a 409, surfaced as a
    /// plain message rather than an opaque error — the planner turns that into
    /// "select the existing one instead".
    pub fn create_project(
        &self,
        name: &str,
        client_id: Option<&str>,
        parent_id: Option<&str>,
    ) -> Result<Project> {
        let mut body = serde_json::Map::new();
        body.insert("name".into(), serde_json::json!(name));
        if let Some(cid) = client_id.filter(|c| !c.trim().is_empty()) {
            body.insert("client_id".into(), serde_json::json!(cid.trim()));
        }
        if let Some(pid) = parent_id.filter(|p| !p.trim().is_empty()) {
            body.insert("parent_id".into(), serde_json::json!(pid.trim()));
        }
        let resp = self
            .http
            .post(self.url("/v1/projects"))
            .bearer_auth(&self.key)
            .json(&serde_json::Value::Object(body))
            .send()?;
        if resp.status() == reqwest::StatusCode::CONFLICT {
            anyhow::bail!("a project named `{name}` already exists — select it instead");
        }
        Ok(resp.error_for_status()?.json()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(kind: &str, attestation: serde_json::Value) -> Candidate {
        Candidate {
            id: "c1".into(),
            source_identity: "repo-docs:x:docs/a.md#intro:abc".into(),
            destination_kind: kind.into(),
            content: "body".into(),
            destination_hint: serde_json::json!({ "title": "Intro" }),
            source_excerpt: None,
            confidence: Some(0.9),
            attestation,
            provenance_kind: "migrated".into(),
            status: "staged".into(),
            version: 1,
        }
    }

    #[test]
    fn a_harness_candidate_can_never_be_batch_approved() {
        assert!(candidate("harness", serde_json::json!({})).needs_individual_review());
        assert!(!candidate("memory", serde_json::json!({})).needs_individual_review());
    }

    #[test]
    fn a_client_attested_candidate_can_never_be_batch_approved() {
        let c = candidate(
            "memory",
            serde_json::json!({ "client_attested": "MSA-2026-014" }),
        );
        assert!(c.needs_individual_review());
    }

    /// A key present but null must not read as an attestation.
    #[test]
    fn a_null_attestation_is_not_an_attestation() {
        let c = candidate("memory", serde_json::json!({ "client_attested": null }));
        assert!(!c.needs_individual_review());
    }

    #[test]
    fn a_candidate_falls_back_to_its_source_identity_when_the_hint_has_no_title() {
        let mut c = candidate("memory", serde_json::json!({}));
        c.destination_hint = serde_json::json!({});
        assert_eq!(c.title(), "repo-docs:x:docs/a.md#intro:abc");
    }

    /// Captured from a live backend. These pin the parts of the wire format
    /// that unit tests written from the struct definitions cannot: which fields
    /// the server actually omits.
    #[test]
    fn a_real_candidates_response_deserializes() {
        let raw = include_str!("../tests/fixtures/candidates.json");
        let list: Vec<Candidate> = serde_json::from_str(raw).expect("live shape must parse");
        assert!(!list.is_empty());
        assert!(
            !raw.contains("\"confidence\""),
            "the fixture is only interesting because the server omits this field"
        );
        assert!(
            list[0].confidence.is_none(),
            "an omitted confidence must default, not fail the whole queue"
        );
        assert_eq!(list[0].status, "staged");
        assert!(!list[0].title().is_empty());
    }

    #[test]
    fn a_real_runs_response_deserializes() {
        let list: Vec<Run> = serde_json::from_str(include_str!("../tests/fixtures/runs.json"))
            .expect("the run picker parses a bare array, not an envelope");
        assert!(!list.is_empty());
        assert!(!list[0].id.is_empty());
        assert!(!list[0].source_kind.is_empty());
    }

    /// A lost race must be visible. The literal the backend sends is
    /// `stale_version`; anything not `applied` counts.
    #[test]
    fn a_stale_version_is_recognised_as_not_applied() {
        let resp: ReviewResponse = serde_json::from_str(
            r#"{"applied":0,"conflicts":1,"results":[
                 {"candidate_id":"c1","outcome":"stale_version","actual_version":2}]}"#,
        )
        .unwrap();
        assert_eq!(resp.conflicts, 1);
        let unapplied: Vec<&str> = resp
            .results
            .iter()
            .filter(|r| r.outcome != "applied")
            .map(|r| r.candidate_id.as_str())
            .collect();
        assert_eq!(unapplied, vec!["c1"]);
    }

    /// Pinned against `ReviewVerdict` in the backend's models. If these ever
    /// disagree the whole request is refused, so a drift here is total.
    #[test]
    fn the_wire_verdicts_are_past_tense() {
        assert_eq!(Verdict::Approved.wire(), "approved");
        assert_eq!(Verdict::Rejected.wire(), "rejected");
        assert_eq!(Verdict::Restaged.wire(), "restaged");
    }

    #[test]
    fn the_base_url_tolerates_a_trailing_slash() {
        let c = Client::new("http://localhost:8080/", "k").unwrap();
        assert_eq!(c.url("/v1/migrations"), "http://localhost:8080/v1/migrations");
    }
}
