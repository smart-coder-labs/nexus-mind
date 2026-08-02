//! Versioned Context Fabric contracts and the read-only Compiler v0.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const CONTRACT_VERSION: &str = "context-fabric.v0";
pub const BASELINE_PROFILE: &str = "nomic-768-f32-baseline";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerationRef {
    pub id: String,
    pub version: u64,
}

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
}
