//! Versioned Context Fabric contracts and the read-only Compiler v0.

use anyhow::{anyhow, Result};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const CONTRACT_VERSION: &str = "context-fabric.v0";
pub const BASELINE_PROFILE: &str = "baseline-nomic-768-f32-v1";
pub const CONTEXT_FABRIC_SCHEMA_VERSION: i32 = 58;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

    for candidate in &request.candidates {
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
