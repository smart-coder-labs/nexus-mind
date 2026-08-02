//! Versioned Context Fabric contracts and the read-only Compiler v0.

use anyhow::{anyhow, Result};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{collections::{HashMap, HashSet}, time::Instant};

pub const CONTRACT_VERSION: &str = "context-fabric.v0";
pub const BASELINE_PROFILE: &str = "baseline-nomic-768-f32-v1";
pub const CONTEXT_FABRIC_SCHEMA_VERSION: i32 = 58;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct GenerationRef {
    pub id: String,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelDescriptor {
    pub provider: String,
    pub revision: String,
    pub hash: String,
    pub license: String,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactDescriptor {
    pub name: String,
    pub checksum: String,
    pub size_bytes: u64,
}

/// Immutable, self-contained identity for an index/profile generation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextFabricManifest {
    pub contract_version: String,
    pub schema_version: String,
    pub profile_id: String,
    pub profile_version: u32,
    pub snapshot: String,
    pub source_commit: String,
    pub tenant_scope: String,
    pub chunker: String,
    pub preprocessing: String,
    pub prefixes: Vec<String>,
    pub model: ModelDescriptor,
    pub dimension: u32,
    pub dtype: String,
    pub normalization: String,
    pub tokenizer: String,
    pub acl_generation: u64,
    pub policy_generation: u64,
    pub generation: GenerationRef,
    pub artifacts: Vec<ArtifactDescriptor>,
    pub consumer_compatibility: Vec<String>,
}

impl ContextFabricManifest {
    pub fn validate(&self) -> Result<()> {
        let required = [
            (&self.contract_version, "contract_version"),
            (&self.schema_version, "schema_version"),
            (&self.profile_id, "profile_id"),
            (&self.snapshot, "snapshot"),
            (&self.source_commit, "source_commit"),
            (&self.tenant_scope, "tenant_scope"),
            (&self.chunker, "chunker"),
            (&self.preprocessing, "preprocessing"),
            (&self.model.provider, "model.provider"),
            (&self.model.revision, "model.revision"),
            (&self.model.hash, "model.hash"),
            (&self.model.license, "model.license"),
            (&self.model.origin, "model.origin"),
            (&self.dtype, "dtype"),
            (&self.normalization, "normalization"),
            (&self.tokenizer, "tokenizer"),
            (&self.generation.id, "generation.id"),
        ];
        if let Some((_, name)) = required.iter().find(|(value, _)| value.trim().is_empty()) {
            return Err(anyhow!("missing_{name}"));
        }
        if self.contract_version != CONTRACT_VERSION {
            return Err(anyhow!("unsupported_contract_version"));
        }
        if self.dimension == 0 || self.generation.version == 0 || self.profile_version == 0 {
            return Err(anyhow!("invalid_version_or_dimension"));
        }
        if self.model.hash.trim().is_empty() || self.model.hash == "sha256:" {
            return Err(anyhow!("empty_model_hash"));
        }
        if self.preprocessing.trim().is_empty() || self.chunker.trim().is_empty() {
            return Err(anyhow!("implicit_preprocessing"));
        }
        if self.profile_id == BASELINE_PROFILE
            && (self.dimension != 768 || self.dtype.to_lowercase() != "f32")
        {
            return Err(anyhow!("baseline_dimension_dtype_mismatch"));
        }
        if self.dimension == 768 && self.dtype.to_lowercase() != "f32" {
            return Err(anyhow!("dimension_dtype_mismatch"));
        }
        if self.artifacts.is_empty() {
            return Err(anyhow!("missing_artifacts"));
        }
        if self.artifacts.iter().any(|artifact| {
            artifact.name.trim().is_empty()
                || artifact.checksum.trim().is_empty()
                || artifact.size_bytes == 0
        }) {
            return Err(anyhow!("invalid_artifact"));
        }
        if self.prefixes.is_empty() || self.consumer_compatibility.is_empty() {
            return Err(anyhow!("missing_manifest_compatibility"));
        }
        Ok(())
    }
}

pub type ProfileManifest = ContextFabricManifest;
pub type GenerationManifest = ContextFabricManifest;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetrievalProfile {
    pub id: String,
    pub version: u32,
    pub method: String,
    pub dimension: u32,
    pub dtype: String,
    pub preprocessing: String,
    pub generation: GenerationRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexManifest {
    pub contract_version: String,
    pub profile: RetrievalProfile,
    pub snapshot: String,
    pub tenant_scope: String,
    pub model_hash: String,
    pub acl_generation: u64,
    pub tokenizer: String,
    pub normalization: String,
    pub consumer_compatibility: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Locator {
    pub source: String,
    pub id: String,
    pub reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CandidateEvidence {
    pub unit_id: String,
    pub source: String,
    pub content: String,
    pub locator: Locator,
    pub provenance: String,
    pub generation: GenerationRef,
    pub fresh: bool,
    pub required: bool,
    #[serde(default)]
    pub captured_at_unix: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssembleRequest {
    pub contract_version: String,
    pub tokenizer: String,
    pub token_budget: usize,
    pub source_cap: usize,
    #[serde(default)]
    pub excluded_sources: Vec<String>,
    #[serde(default)]
    pub required_sources: Vec<String>,
    pub generation: GenerationRef,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub profile_version: Option<u32>,
    pub candidates: Vec<CandidateEvidence>,
    #[serde(default)]
    pub freshness_window_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompileDiagnostics {
    pub reason_codes: Vec<String>,
    pub candidate_count: usize,
    pub selected_count: usize,
    pub omitted_sources: Vec<String>,
    pub coverage: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssembleResponse {
    pub contract_version: String,
    pub abstained: bool,
    pub units: Vec<CandidateEvidence>,
    pub diagnostics: CompileDiagnostics,
}

/// Versioned input to generation. The assembled response is intentionally embedded rather
/// than accepting raw candidates: generation cannot bypass the Compiler v0 boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GenerateRequest {
    pub contract_version: String,
    pub profile_id: String,
    pub profile_version: u32,
    pub generation: GenerationRef,
    pub model: String,
    pub provider: String,
    pub output_token_budget: usize,
    pub assembled: AssembleResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claim {
    pub id: String,
    pub text: String,
    pub unit_id: String,
    pub locator: Locator,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerationMetadata {
    pub contract_version: String,
    pub profile_id: String,
    pub profile_version: u32,
    pub generation: GenerationRef,
    pub model: String,
    pub provider: String,
    pub budgets: BudgetReport,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BudgetReport {
    pub requested_tokens: usize,
    pub used_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvenanceRecord {
    pub unit_id: String,
    pub locator: Locator,
    pub provenance: String,
    pub generation: GenerationRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerateResponse {
    pub output: Option<String>,
    pub metadata: GenerationMetadata,
    pub provenance: Vec<ProvenanceRecord>,
    pub claims: Vec<Claim>,
    pub abstained: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VerifyRequest {
    pub contract_version: String,
    pub profile_id: String,
    pub profile_version: u32,
    pub generation: GenerationRef,
    pub model: String,
    pub provider: String,
    pub assembled: AssembleResponse,
    pub output: Option<String>,
    pub claims: Vec<Claim>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClaimStatus {
    Verified,
    Unsupported,
    Contradicted,
    Stale,
    Unauthorized,
    Abstained,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaimVerification {
    pub id: String,
    pub status: ClaimStatus,
    pub reason_codes: Vec<String>,
    pub locator: Option<Locator>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifyResponse {
    pub status: ClaimStatus,
    pub metadata: GenerationMetadata,
    pub claims: Vec<ClaimVerification>,
    pub reason_codes: Vec<String>,
}

pub const DETERMINISTIC_PROVIDER: &str = "deterministic-extractive-v0";

pub fn generation_metadata(request: &GenerateRequest, used_tokens: usize, reasons: Vec<String>) -> GenerationMetadata {
    GenerationMetadata {
        contract_version: request.contract_version.clone(),
        profile_id: request.profile_id.clone(),
        profile_version: request.profile_version,
        generation: request.generation.clone(),
        model: request.model.clone(),
        provider: request.provider.clone(),
        budgets: BudgetReport { requested_tokens: request.output_token_budget, used_tokens },
        reason_codes: reasons,
    }
}

pub fn validate_generation_identity(
    contract_version: &str,
    profile_id: &str,
    profile_version: u32,
    generation: &GenerationRef,
    model: &str,
    provider: &str,
    assembled: &AssembleResponse,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if contract_version != CONTRACT_VERSION { reasons.push("unsupported_contract_version".into()); }
    if profile_id.trim().is_empty() { reasons.push("missing_profile".into()); }
    if profile_version == 0 { reasons.push("invalid_profile_version".into()); }
    if generation.id.trim().is_empty() || generation.version == 0 { reasons.push("invalid_generation".into()); }
    if model.trim().is_empty() { reasons.push("missing_model".into()); }
    if provider.trim().is_empty() { reasons.push("missing_provider".into()); }
    if assembled.contract_version != contract_version { reasons.push("compiled_contract_mismatch".into()); }
    if assembled.abstained
        || assembled.units.is_empty()
        || assembled.diagnostics.selected_count != assembled.units.len()
        || !assembled.diagnostics.reason_codes.is_empty()
    {
        reasons.push("context_not_compiled".into());
    }
    if assembled.units.iter().any(|unit| unit.generation != *generation) { reasons.push("generation_mismatch".into()); }
    reasons.sort();
    reasons.dedup();
    reasons
}

/// Lab-only deterministic provider. It is deliberately extractive and performs no I/O.
pub fn generate_deterministic(request: &GenerateRequest) -> GenerateResponse {
    let mut reasons = validate_generation_identity(
        &request.contract_version, &request.profile_id, request.profile_version,
        &request.generation, &request.model, &request.provider, &request.assembled,
    );
    if request.output_token_budget == 0 { reasons.push("invalid_budget".into()); }
    if request.provider != DETERMINISTIC_PROVIDER { reasons.push("provider_unavailable".into()); }
    reasons.sort();
    reasons.dedup();
    if !reasons.is_empty() {
        return GenerateResponse {
            output: None,
            metadata: generation_metadata(request, 0, reasons),
            provenance: Vec::new(),
            claims: Vec::new(),
            abstained: true,
        };
    }

    let mut used = 0;
    let mut parts = Vec::new();
    let mut provenance = Vec::new();
    let mut claims = Vec::new();
    for unit in &request.assembled.units {
        let count = token_count(&unit.content, "whitespace-v0");
        if used + count > request.output_token_budget { reasons.push("budget_exceeded".into()); break; }
        used += count;
        parts.push(unit.content.clone());
        provenance.push(ProvenanceRecord { unit_id: unit.unit_id.clone(), locator: unit.locator.clone(), provenance: unit.provenance.clone(), generation: unit.generation.clone() });
        claims.push(Claim { id: unit.unit_id.clone(), text: unit.content.clone(), unit_id: unit.unit_id.clone(), locator: unit.locator.clone() });
    }
    if parts.is_empty() || !reasons.is_empty() {
        reasons.push("abstained".into());
        reasons.sort();
        reasons.dedup();
        return GenerateResponse { output: None, metadata: generation_metadata(request, used, reasons), provenance: Vec::new(), claims: Vec::new(), abstained: true };
    }
    GenerateResponse { output: Some(parts.join("\n")), metadata: generation_metadata(request, used, reasons), provenance, claims, abstained: false }
}

fn token_count(content: &str, tokenizer: &str) -> usize {
    // Explicit tokenizer is part of the contract; whitespace is the deterministic v0
    // fallback until tokenizer implementations become versioned dependencies.
    let _ = tokenizer;
    content.split_whitespace().count()
}

/// Compile complete authorized evidence units without silently truncating any unit.
pub fn compile(request: &AssembleRequest) -> AssembleResponse {
    let mut reasons = Vec::new();
    if request.contract_version != CONTRACT_VERSION {
        reasons.push("unsupported_contract_version".into());
    }
    if request.tokenizer.trim().is_empty() {
        reasons.push("invalid_tokenizer".into());
    }
    if request.token_budget == 0 {
        reasons.push("invalid_budget".into());
    }
    if request.source_cap == 0 {
        reasons.push("invalid_source_cap".into());
    }
    if request.candidates.iter().any(|c| c.source != "memory") {
        reasons.push("unsupported_unverified_source".into());
    }

    let excluded: HashSet<&str> = request
        .excluded_sources
        .iter()
        .map(String::as_str)
        .collect();
    let mut seen_units = HashSet::new();
    let mut seen_content = HashSet::new();
    let mut source_counts: HashMap<&str, usize> = HashMap::new();
    let mut units = Vec::new();
    let mut used_tokens = 0;
    let mut budget_omissions = 0;
    let mut freshness_omissions = false;

    for candidate in &request.candidates {
        if let Some(window) = request.freshness_window_secs {
            let fresh = candidate.captured_at_unix.map(|captured| {
                let now = chrono::Utc::now().timestamp();
                captured <= now && now.saturating_sub(captured) <= window as i64
            }).unwrap_or(false);
            if !fresh { freshness_omissions = true; continue; }
        }
        if excluded.contains(candidate.source.as_str())
            || candidate.generation != request.generation
            || !candidate.fresh
        {
            continue;
        }
        if !seen_units.insert(candidate.unit_id.as_str())
            || !seen_content.insert(candidate.content.as_str())
        {
            continue;
        }
        let count = token_count(&candidate.content, &request.tokenizer);
        if used_tokens + count > request.token_budget {
            budget_omissions += 1;
            continue;
        }
        let source_count = source_counts.entry(candidate.source.as_str()).or_default();
        if *source_count >= request.source_cap {
            continue;
        }
        *source_count += 1;
        used_tokens += count;
        units.push(candidate.clone());
    }

    let selected_sources: HashSet<&str> = units.iter().map(|u| u.source.as_str()).collect();
    let mut omitted_sources: Vec<String> = request
        .required_sources
        .iter()
        .filter(|source| !selected_sources.contains(source.as_str()))
        .cloned()
        .collect();
    omitted_sources.sort();
    if !omitted_sources.is_empty() {
        reasons.push("required_source_unavailable".into());
    }
    if budget_omissions > 0 && units.is_empty() {
        reasons.push("budget_exceeded".into());
    }
    if request
        .candidates
        .iter()
        .any(|c| c.generation != request.generation)
    {
        reasons.push("generation_mismatch".into());
    }
    if request.candidates.iter().any(|c| !c.fresh) {
        reasons.push("stale_evidence_excluded".into());
    }
    if freshness_omissions {
        reasons.push("stale_evidence_excluded".into());
    }
    if request.freshness_window_secs.is_some() && request.candidates.iter().any(|c| c.captured_at_unix.is_none()) {
        reasons.push("freshness_unknown_timestamp".into());
    }
    reasons.sort();
    reasons.dedup();
    let mut coverage: Vec<String> = selected_sources.into_iter().map(str::to_string).collect();
    coverage.sort();
    let abstained = !reasons.is_empty();
    let selected_count = if abstained { 0 } else { units.len() };
    if abstained {
        units.clear();
    }

    AssembleResponse {
        contract_version: CONTRACT_VERSION.into(),
        abstained,
        units,
        diagnostics: CompileDiagnostics {
            reason_codes: reasons,
            candidate_count: request.candidates.len(),
            selected_count,
            omitted_sources,
            coverage,
        },
    }
}

fn registry_required(conn: &Connection) -> Result<()> {
    crate::db::migrations::verify_context_fabric(conn)
        .map_err(|_| anyhow!("context_fabric_migration_pending"))
}

pub fn publish_generation(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    manifest: &ContextFabricManifest,
) -> Result<ContextFabricManifest> {
    manifest.validate()?;
    if manifest.tenant_scope != org_id && manifest.tenant_scope != "org" {
        return Err(anyhow!("tenant_scope_mismatch"));
    }
    registry_required(conn)?;
    let json = serde_json::to_string(manifest)?;
    let marker = format!("{}:{}", manifest.generation.id, manifest.generation.version);
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        conn.execute(
            "INSERT INTO cf_manifests
             (org_id, profile_id, profile_version, generation_id, generation_version,
              tenant_scope, status, manifest_json, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'private', ?7, ?8)",
            rusqlite::params![
                org_id,
                manifest.profile_id,
                manifest.profile_version,
                manifest.generation.id,
                manifest.generation.version,
                manifest.tenant_scope,
                json,
                user_id
            ],
        )?;
        conn.execute(
            "UPDATE cf_manifests SET status = 'committed', commit_marker = ?1
             WHERE org_id = ?2 AND profile_id = ?3 AND profile_version = ?4
               AND generation_id = ?5 AND generation_version = ?6",
            rusqlite::params![
                marker,
                org_id,
                manifest.profile_id,
                manifest.profile_version,
                manifest.generation.id,
                manifest.generation.version
            ],
        )?;
        conn.execute(
            "UPDATE cf_manifests SET status = 'retired'
             WHERE org_id = ?1 AND status = 'active'",
            [org_id],
        )?;
        conn.execute(
            "UPDATE cf_manifests SET status = 'active'
             WHERE org_id = ?1 AND profile_id = ?2 AND profile_version = ?3
               AND generation_id = ?4 AND generation_version = ?5",
            rusqlite::params![
                org_id,
                manifest.profile_id,
                manifest.profile_version,
                manifest.generation.id,
                manifest.generation.version
            ],
        )?;
        conn.execute(
            "INSERT INTO cf_active_pointers
             (org_id, profile_id, profile_version, generation_id, generation_version)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(org_id) DO UPDATE SET profile_id=excluded.profile_id,
             profile_version=excluded.profile_version, generation_id=excluded.generation_id,
             generation_version=excluded.generation_version, updated_at=datetime('now')",
            rusqlite::params![
                org_id,
                manifest.profile_id,
                manifest.profile_version,
                manifest.generation.id,
                manifest.generation.version
            ],
        )?;
        Ok::<_, anyhow::Error>(())
    })();
    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            Ok(manifest.clone())
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

pub fn active_manifest(conn: &Connection, org_id: &str) -> Result<Option<ContextFabricManifest>> {
    registry_required(conn)?;
    let raw = conn
        .query_row(
            "SELECT m.manifest_json FROM cf_active_pointers p
             JOIN cf_manifests m ON m.org_id=p.org_id AND m.profile_id=p.profile_id
               AND m.profile_version=p.profile_version AND m.generation_id=p.generation_id
               AND m.generation_version=p.generation_version AND m.status='active'
             WHERE p.org_id=?1",
            [org_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    raw.map(|json| {
        let manifest: ContextFabricManifest = serde_json::from_str(&json)?;
        manifest.validate()?;
        Ok(manifest)
    })
    .transpose()
}

pub fn rollback_generation(
    conn: &Connection,
    org_id: &str,
    generation: &GenerationRef,
) -> Result<ContextFabricManifest> {
    registry_required(conn)?;
    let raw: String = conn.query_row(
        "SELECT manifest_json, created_by FROM cf_manifests
         WHERE org_id=?1 AND generation_id=?2 AND generation_version=?3
           AND status IN ('active', 'retired', 'committed')",
        rusqlite::params![org_id, generation.id, generation.version],
        |row| row.get(0),
    )?;
    let manifest: ContextFabricManifest = serde_json::from_str(&raw)?;
    manifest.validate()?;
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        conn.execute(
            "UPDATE cf_manifests SET status='retired' WHERE org_id=?1 AND status='active'",
            [org_id],
        )?;
        conn.execute(
            "UPDATE cf_manifests SET status='active' WHERE org_id=?1 AND generation_id=?2 AND generation_version=?3",
            rusqlite::params![org_id, generation.id, generation.version],
        )?;
        conn.execute(
            "UPDATE cf_active_pointers SET profile_id=?2, profile_version=?3,
             generation_id=?4, generation_version=?5, updated_at=datetime('now') WHERE org_id=?1",
            rusqlite::params![
                org_id,
                manifest.profile_id,
                manifest.profile_version,
                manifest.generation.id,
                manifest.generation.version
            ],
        )?;
        Ok::<_, anyhow::Error>(())
    })();
    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            Ok(manifest)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

pub fn verify_request_generation(
    conn: &Connection,
    org_id: &str,
    request: &AssembleRequest,
) -> Result<()> {
    let Some(profile_id) = request.profile_id.as_deref() else {
        return Ok(());
    };
    let active = active_manifest(conn, org_id)?.ok_or_else(|| anyhow!("no_active_generation"))?;
    if active.profile_id != profile_id
        || request.profile_version != Some(active.profile_version)
        || request.generation != active.generation
    {
        return Err(anyhow!("cross_generation_profile_mismatch"));
    }
    Ok(())
}

// Experimental retrieval primitives stay outside the active dense lane. A caller must
// explicitly request a shadow run and the result is never promoted automatically.
pub const MRL_DIMENSIONS: &[usize] = &[768, 512, 256, 128, 64];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShadowVector { pub id: String, pub tenant_scope: String, pub dense: Vec<f32>, pub authorized: bool, pub fresh: bool }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowRequest {
    pub capability: String,
    pub manifest: ContextFabricManifest,
    pub baseline_manifest: ContextFabricManifest,
    pub query: Vec<f32>,
    pub arena: Vec<ShadowVector>,
    pub k: usize,
    pub alpha: f32,
    #[serde(default)] pub prefix_dimension: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShadowMetrics {
    pub candidate_recall_at_k: f32, pub alpha: f32, pub candidate_latency_ms: f64,
    pub dense_rescore_latency_ms: f64, pub candidate_payload_bytes: usize,
    pub dense_payload_bytes: usize, pub theoretical_payload_reduction: f32,
    pub rss_theoretical_bytes: usize, pub quality_delta: f32,
    pub security_violations: usize, pub freshness_violations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShadowResponse {
    pub capability: String, pub baseline_ids: Vec<String>, pub candidate_ids: Vec<String>,
    pub rescored_ids: Vec<String>, pub metrics: ShadowMetrics, pub gate_pass: bool,
    pub promotion: bool, pub fallback: String, pub reason_codes: Vec<String>,
}

pub fn sign_bit_encode(vector: &[f32]) -> Vec<u8> {
    vector.chunks(8).map(|chunk| chunk.iter().enumerate().fold(0u8, |word, (bit, value)| word | (((*value).is_sign_positive() as u8) << bit))).collect()
}

pub fn sign_bit_encode_words(vector: &[f32]) -> Vec<u64> {
    vector.chunks(64).map(|chunk| chunk.iter().enumerate().fold(0u64, |word, (bit, value)| word | (((*value).is_sign_positive() as u64) << bit))).collect()
}

pub fn sign_bit_decode(encoded: &[u8], dimension: usize) -> Vec<f32> {
    (0..dimension).map(|index| if encoded[index / 8] & (1 << (index % 8)) != 0 { 1.0 } else { -1.0 }).collect()
}

pub fn hamming_distance(left: &[u8], right: &[u8]) -> u32 {
    left.iter().zip(right).map(|(a, b)| (a ^ b).count_ones()).sum::<u32>() + (left.len().saturating_sub(right.len()) * 8) as u32
}

pub fn hamming_distance_words(left: &[u64], right: &[u64]) -> u32 {
    left.iter().zip(right).map(|(a, b)| (a ^ b).count_ones()).sum::<u32>() + (left.len().saturating_sub(right.len()) * 64) as u32
}

fn dot_distance(left: &[f32], right: &[f32], dimension: usize) -> f32 {
    let dimensions = dimension.min(left.len()).min(right.len());
    let (dot, left_norm, right_norm) = (0..dimensions).fold((0.0, 0.0, 0.0), |(dot, ln, rn), i| (dot + left[i] * right[i], ln + left[i] * left[i], rn + right[i] * right[i]));
    if left_norm == 0.0 || right_norm == 0.0 { 1.0 } else { 1.0 - dot / (left_norm.sqrt() * right_norm.sqrt()) }
}

fn sorted_ids(mut scored: Vec<(String, f32)>) -> Vec<String> {
    scored.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    scored.into_iter().map(|(id, _)| id).collect()
}

fn compatible_shadow_manifests(request: &ShadowRequest) -> Result<usize> {
    request.manifest.validate()?; request.baseline_manifest.validate()?;
    if request.manifest.tenant_scope != request.baseline_manifest.tenant_scope
        || request.manifest.snapshot != request.baseline_manifest.snapshot
        || request.manifest.source_commit != request.baseline_manifest.source_commit
        || request.manifest.preprocessing != request.baseline_manifest.preprocessing
        || request.manifest.chunker != request.baseline_manifest.chunker
        || request.manifest.normalization != request.baseline_manifest.normalization
        || request.manifest.tokenizer != request.baseline_manifest.tokenizer
        || request.manifest.model.hash != request.baseline_manifest.model.hash
        || request.manifest.acl_generation != request.baseline_manifest.acl_generation
        || request.manifest.policy_generation != request.baseline_manifest.policy_generation
        || request.manifest.generation != request.baseline_manifest.generation { return Err(anyhow!("incompatible_manifest_preprocessing_or_generation")); }
    if request.manifest.dimension != 768 || request.manifest.dtype.to_ascii_lowercase() != "f32" { return Err(anyhow!("shadow_base_dimension_dtype_mismatch")); }
    let dimension = request.prefix_dimension.unwrap_or(768);
    if request.capability.eq_ignore_ascii_case("mrl") && !MRL_DIMENSIONS.contains(&dimension) { return Err(anyhow!("unsupported_mrl_prefix_dimension")); }
    if request.capability.eq_ignore_ascii_case("bq") && dimension != 768 { return Err(anyhow!("bq_dimension_mismatch")); }
    if !matches!(request.capability.to_ascii_lowercase().as_str(), "bq" | "mrl") { return Err(anyhow!("unsupported_shadow_capability")); }
    Ok(dimension)
}

pub fn run_shadow(request: &ShadowRequest, tenant: &str, flag: &str) -> Result<ShadowResponse> {
    if flag != "shadow" { return Err(anyhow!("capability_flag_not_shadow")); }
    if request.k == 0 { return Err(anyhow!("invalid_k")); }
    if !(request.alpha.is_finite() && request.alpha > 0.0 && request.alpha <= 16.0) { return Err(anyhow!("invalid_alpha")); }
    let dimension = compatible_shadow_manifests(request)?;
    if request.manifest.tenant_scope != tenant && request.manifest.tenant_scope != "org" { return Err(anyhow!("manifest_tenant_scope_mismatch")); }
    if request.query.len() != 768 || request.arena.iter().any(|v| v.dense.len() != 768) { return Err(anyhow!("vector_dimension_mismatch")); }
    if request.arena.iter().any(|v| v.tenant_scope != tenant) { return Err(anyhow!("authorization_isolation_violation")); }
    if request.arena.iter().any(|v| !v.authorized) { return Err(anyhow!("unauthorized_vector")); }
    if request.arena.iter().any(|v| !v.fresh) { return Err(anyhow!("stale_vector")); }
    let baseline_ids = sorted_ids(request.arena.iter().map(|v| (v.id.clone(), dot_distance(&request.query, &v.dense, 768))).collect()).into_iter().take(request.k).collect::<Vec<_>>();
    let candidate_start = Instant::now();
    let candidate_ids = if request.capability.eq_ignore_ascii_case("bq") {
        let query_bits = sign_bit_encode(&request.query);
        sorted_ids(request.arena.iter().map(|v| (v.id.clone(), hamming_distance(&query_bits, &sign_bit_encode(&v.dense)) as f32)).collect())
    } else { sorted_ids(request.arena.iter().map(|v| (v.id.clone(), dot_distance(&request.query, &v.dense, dimension))).collect()) };
    let candidate_count = ((request.k as f32 * request.alpha).ceil() as usize).min(candidate_ids.len());
    let candidate_ids = candidate_ids.into_iter().take(candidate_count).collect::<Vec<_>>();
    let candidate_latency_ms = candidate_start.elapsed().as_secs_f64() * 1000.0;
    let rescore_start = Instant::now();
    let rescored_ids = sorted_ids(candidate_ids.iter().filter_map(|id| request.arena.iter().find(|v| &v.id == id).map(|v| (id.clone(), dot_distance(&request.query, &v.dense, 768)))).collect()).into_iter().take(request.k).collect::<Vec<_>>();
    let dense_rescore_latency_ms = rescore_start.elapsed().as_secs_f64() * 1000.0;
    let denominator = request.k.min(baseline_ids.len()).max(1) as f32;
    let candidate_recall_at_k = baseline_ids.iter().filter(|id| candidate_ids.contains(id)).count() as f32 / denominator;
    let quality_delta = 1.0 - baseline_ids.iter().zip(&rescored_ids).filter(|(a, b)| a == b).count() as f32 / denominator;
    let candidate_payload_bytes = if request.capability.eq_ignore_ascii_case("bq") { sign_bit_encode(&request.query).len() } else { dimension * 4 / 8 } * candidate_ids.len();
    let dense_payload_bytes = 768 * 4 * candidate_ids.len();
    let mut reason_codes = Vec::new();
    if request.alpha > 8.0 { reason_codes.push("alpha_diagnostic_only".into()); }
    if candidate_recall_at_k < 0.98 { reason_codes.push("candidate_recall_gate_failed".into()); }
    if request.alpha > 8.0 { reason_codes.push("alpha_gate_failed".into()); }
    if quality_delta > 0.01 { reason_codes.push("quality_gate_failed".into()); }
    let gate_pass = candidate_recall_at_k >= 0.98 && request.alpha <= 8.0 && quality_delta <= 0.01;
    reason_codes.push(if gate_pass { "manual_promotion_required" } else { "baseline_fallback" }.into());
    Ok(ShadowResponse { capability: request.capability.clone(), baseline_ids, candidate_ids, rescored_ids, metrics: ShadowMetrics { candidate_recall_at_k, alpha: request.alpha, candidate_latency_ms, dense_rescore_latency_ms, candidate_payload_bytes, dense_payload_bytes, theoretical_payload_reduction: dense_payload_bytes as f32 / candidate_payload_bytes.max(1) as f32, rss_theoretical_bytes: candidate_payload_bytes, quality_delta, security_violations: 0, freshness_violations: 0 }, gate_pass, promotion: false, fallback: BASELINE_PROFILE.into(), reason_codes })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn generation() -> GenerationRef {
        GenerationRef {
            id: "g1".into(),
            version: 1,
        }
    }
    fn candidate(id: &str, source: &str, content: &str) -> CandidateEvidence {
        CandidateEvidence {
            unit_id: id.into(),
            source: source.into(),
            content: content.into(),
            locator: Locator {
                source: source.into(),
                id: id.into(),
                reference: None,
            },
            provenance: "memory-search".into(),
            generation: generation(),
            fresh: true,
            required: false,
            captured_at_unix: None,
        }
    }
    fn request(candidates: Vec<CandidateEvidence>) -> AssembleRequest {
        AssembleRequest {
            contract_version: CONTRACT_VERSION.into(),
            tokenizer: "whitespace-v0".into(),
            token_budget: 3,
            source_cap: 20,
            excluded_sources: vec![],
            required_sources: vec![],
            generation: generation(),
            profile_id: None,
            profile_version: None,
            candidates,
            freshness_window_secs: None,
        }
    }
    #[test]
    fn compiler_deduplicates_and_preserves_provenance() {
        let result = compile(&request(vec![
            candidate("a", "memory", "one two"),
            candidate("a", "memory", "one two"),
        ]));
        assert!(!result.abstained);
        assert_eq!(result.units.len(), 1);
        assert_eq!(result.units[0].provenance, "memory-search");
    }
    #[test]
    fn compiler_does_not_truncate_a_unit_over_budget() {
        let result = compile(&request(vec![candidate(
            "a",
            "memory",
            "one two three four",
        )]));
        assert!(result.abstained);
        assert!(result.units.is_empty());
        assert_eq!(result.diagnostics.selected_count, 0);
    }
    #[test]
    fn compiler_abstains_for_required_missing_source_and_generation_mismatch() {
        let mut req = request(vec![candidate("a", "memory", "one")]);
        req.required_sources = vec!["code".into()];
        req.candidates[0].generation = GenerationRef {
            id: "g2".into(),
            version: 2,
        };
        let result = compile(&req);
        assert!(result.abstained);
        assert!(result
            .diagnostics
            .reason_codes
            .contains(&"generation_mismatch".into()));
        assert_eq!(result.diagnostics.omitted_sources, vec!["code"]);
    }

    #[test]
    fn compiler_rejects_unverified_sources_and_zero_source_cap() {
        let mut req = request(vec![candidate("a", "code", "one")]);
        req.source_cap = 0;
        let result = compile(&req);
        assert!(result.abstained);
        assert!(result
            .diagnostics
            .reason_codes
            .contains(&"invalid_source_cap".into()));
        assert!(result
            .diagnostics
            .reason_codes
            .contains(&"unsupported_unverified_source".into()));
        assert!(result.units.is_empty());
    }

    #[test]
    fn compiler_fails_closed_at_freshness_boundaries_and_unknown_timestamps() {
        let mut unknown = request(vec![candidate("unknown", "memory", "one")]);
        unknown.freshness_window_secs = Some(60);
        let result = compile(&unknown);
        assert!(result.abstained);
        assert!(result.diagnostics.reason_codes.contains(&"freshness_unknown_timestamp".into()));

        let mut boundary = request(vec![candidate("boundary", "memory", "one")]);
        boundary.freshness_window_secs = Some(60);
        boundary.candidates[0].captured_at_unix = Some(chrono::Utc::now().timestamp() - 61);
        let result = compile(&boundary);
        assert!(result.abstained);
        assert!(result.diagnostics.reason_codes.contains(&"stale_evidence_excluded".into()));
    }

    fn manifest(profile_id: &str, generation: &str, version: u64) -> ContextFabricManifest {
        ContextFabricManifest {
            contract_version: CONTRACT_VERSION.into(),
            schema_version: "cf-manifest.v1".into(),
            profile_id: profile_id.into(),
            profile_version: 1,
            snapshot: "snapshot-1".into(),
            source_commit: "commit-1".into(),
            tenant_scope: "org".into(),
            chunker: "semantic-v1".into(),
            preprocessing: "lowercase-none-v1".into(),
            prefixes: vec!["search_document:".into(), "search_query:".into()],
            model: ModelDescriptor {
                provider: "nomic".into(),
                revision: "v1".into(),
                hash: "sha256:model".into(),
                license: "Apache-2.0".into(),
                origin: "registry".into(),
            },
            dimension: 768,
            dtype: "f32".into(),
            normalization: "l2".into(),
            tokenizer: "whitespace-v0".into(),
            acl_generation: 1,
            policy_generation: 1,
            generation: GenerationRef {
                id: generation.into(),
                version,
            },
            artifacts: vec![ArtifactDescriptor {
                name: "dense.index".into(),
                checksum: "sha256:index".into(),
                size_bytes: 1,
            }],
            consumer_compatibility: vec![CONTRACT_VERSION.into()],
        }
    }

    #[test]
    fn manifest_validation_is_fail_closed_and_baseline_is_named() {
        let mut invalid = manifest(BASELINE_PROFILE, "g1", 1);
        invalid.model.hash.clear();
        assert!(invalid.validate().is_err());
        let mut mismatch = manifest(BASELINE_PROFILE, "g1", 1);
        mismatch.dimension = 384;
        assert!(mismatch.validate().is_err());
        let mut implicit = manifest("new-profile", "g1", 1);
        implicit.preprocessing.clear();
        assert!(implicit.validate().is_err());
    }

    fn shadow_request(capability: &str, alpha: f32) -> ShadowRequest {
        let base = manifest(BASELINE_PROFILE, "g1", 1);
        let mut candidate = base.clone(); candidate.profile_id = format!("{capability}-shadow");
        ShadowRequest { capability: capability.into(), manifest: candidate, baseline_manifest: base, query: vec![1.0; 768], arena: vec![
            ShadowVector { id: "a".into(), tenant_scope: "org".into(), dense: vec![1.0; 768], authorized: true, fresh: true },
            ShadowVector { id: "b".into(), tenant_scope: "org".into(), dense: vec![-1.0; 768], authorized: true, fresh: true },
        ], k: 1, alpha, prefix_dimension: Some(768) }
    }

    #[test]
    fn sign_encoding_decoding_hamming_and_words_are_deterministic() {
        let values = vec![1.0, -1.0, 0.0, -0.0, 2.0, -2.0, 3.0, -3.0, 1.0];
        let encoded = sign_bit_encode(&values);
        assert_eq!(encoded.len(), 2);
        assert_eq!(sign_bit_decode(&encoded, values.len())[0], 1.0);
        assert_eq!(sign_bit_decode(&encoded, values.len())[1], -1.0);
        assert_eq!(hamming_distance(&encoded, &encoded), 0);
        assert_eq!(hamming_distance(&[0], &[255]), hamming_distance(&[255], &[0]));
        assert_eq!(sign_bit_encode_words(&values).len(), 1);
        assert_eq!(hamming_distance_words(&[0], &[u64::MAX]), 64);
    }

    #[test]
    fn mrl_prefixes_invalid_manifests_and_shadow_flags_fail_closed() {
        for dimension in MRL_DIMENSIONS { let mut request = shadow_request("mrl", 2.0); request.prefix_dimension = Some(*dimension); assert!(run_shadow(&request, "org", "shadow").is_ok()); }
        let mut invalid = shadow_request("mrl", 2.0); invalid.prefix_dimension = Some(32);
        assert_eq!(run_shadow(&invalid, "org", "shadow").unwrap_err().to_string(), "unsupported_mrl_prefix_dimension");
        invalid.prefix_dimension = Some(128); invalid.manifest.preprocessing = "different-v2".into();
        assert_eq!(run_shadow(&invalid, "org", "shadow").unwrap_err().to_string(), "incompatible_manifest_preprocessing_or_generation");
        assert_eq!(run_shadow(&shadow_request("bq", 2.0), "org", "off").unwrap_err().to_string(), "capability_flag_not_shadow");
    }

    #[test]
    fn shadow_recall_ties_authorization_and_failed_gate_fallback() {
        let result = run_shadow(&shadow_request("bq", 2.0), "org", "shadow").unwrap();
        assert_eq!(result.baseline_ids, vec!["a"]); assert_eq!(result.rescored_ids, vec!["a"]);
        assert_eq!(result.metrics.candidate_recall_at_k, 1.0); assert!(!result.promotion);
        let mut gate_input = shadow_request("bq", 1.0);
        gate_input.arena[0].dense = vec![0.01; 768]; gate_input.arena[0].dense[0] = -0.01;
        gate_input.arena[1].dense = vec![0.0; 768]; gate_input.arena[1].dense[0] = 1.0;
        let failed = run_shadow(&gate_input, "org", "shadow").unwrap();
        assert!(failed.reason_codes.contains(&"candidate_recall_gate_failed".into()));
        assert!(failed.reason_codes.contains(&"baseline_fallback".into()));
        gate_input.arena[0].tenant_scope = "other-org".into();
        assert_eq!(run_shadow(&gate_input, "org", "shadow").unwrap_err().to_string(), "authorization_isolation_violation");
    }

    #[test]
    fn publication_and_rollback_keep_only_complete_active_generations() {
        let conn = crate::db::connection::connect(":memory:").unwrap();
        crate::db::migrations::run_all(&conn).unwrap();
        let (org, user, _) =
            crate::db::queries::bootstrap(&conn, "Org", "org", "a@org", "Admin").unwrap();
        crate::db::migrations::apply_context_fabric(&conn).unwrap();
        let first = manifest(BASELINE_PROFILE, "g1", 1);
        publish_generation(&conn, &org.id, &user.id, &first).unwrap();
        let mut second = manifest("candidate", "g2", 1);
        second.dimension = 384;
        publish_generation(&conn, &org.id, &user.id, &second).unwrap();
        assert_eq!(
            active_manifest(&conn, &org.id)
                .unwrap()
                .unwrap()
                .generation
                .id,
            "g2"
        );
        rollback_generation(&conn, &org.id, &first.generation).unwrap();
        assert_eq!(
            active_manifest(&conn, &org.id)
                .unwrap()
                .unwrap()
                .generation
                .id,
            "g1"
        );
    }
}
