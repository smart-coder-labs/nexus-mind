//! Backend-owned evidence adapters for Context Fabric.
//!
//! This module accepts locators only. It deliberately performs visibility checks
//! before fetching code/SDD content and returns stable, non-enumerating reasons.

use crate::{
    api::helpers::project_is_visible_to_actor,
    context_fabric::{AssembleRequest, CandidateEvidence, EvidenceReference, GenerationRef},
    db::queries,
    models::types::AuthContext,
};
use rusqlite::Connection;

const CODE_PROVENANCE: &str = "code-knowledge-graph";
const SDD_PROVENANCE: &str = "sdd-artifact-store";

pub fn resolve(
    conn: &Connection,
    auth: &AuthContext,
    request: &AssembleRequest,
) -> Result<Vec<CandidateEvidence>, &'static str> {
    let mut resolved = Vec::with_capacity(request.references.len());
    for reference in &request.references {
        let candidate = match reference.source.as_str() {
            "code" => resolve_code(conn, auth, request, reference)?,
            "sdd" => resolve_sdd(conn, auth, request, reference)?,
            _ => return Err("unsupported_unverified_source"),
        };
        resolved.push(candidate);
    }
    Ok(resolved)
}

fn source_generation(project_id: &str, indexed_at: Option<&str>) -> Option<GenerationRef> {
    let indexed_at = indexed_at?.trim();
    if indexed_at.is_empty() {
        return None;
    }
    Some(GenerationRef {
        id: format!("code-project:{project_id}:{indexed_at}"),
        version: 1,
    })
}

fn check_expected(
    reference: &EvidenceReference,
    content_hash: &str,
    source_generation: Option<&GenerationRef>,
) -> Result<(), &'static str> {
    if let Some(expected) = reference.expected_hash.as_deref() {
        if expected != content_hash {
            return Err("evidence_integrity_mismatch");
        }
    }
    if let Some(expected) = reference.expected_generation.as_ref() {
        if source_generation != Some(expected) {
            return Err("generation_mismatch");
        }
    }
    Ok(())
}

fn resolve_code(
    conn: &Connection,
    auth: &AuthContext,
    request: &AssembleRequest,
    reference: &EvidenceReference,
) -> Result<CandidateEvidence, &'static str> {
    if reference.locator.source != "code" || reference.locator.id.trim().is_empty() {
        return Err("unsupported_unverified_source");
    }
    let chunk_id = reference
        .locator
        .id
        .parse::<i64>()
        .map_err(|_| "evidence_not_found")?;
    let project = queries::get_code_project_by_id_visible(
        conn,
        &auth.org_id,
        chunk_project_id(conn, chunk_id)?,
        if auth.role.is_super_user() { None } else { Some(&auth.user_id) },
    )
    .map_err(|_| "evidence_not_found")?
    .ok_or("evidence_not_found")?;
    let chunk = queries::get_chunks_by_ids(conn, &[chunk_id])
        .map_err(|_| "evidence_not_found")?
        .into_iter()
        .next()
        .ok_or("evidence_not_found")?;
    if chunk.code_project_id.to_string() != project.id {
        return Err("evidence_not_found");
    }
    if let Some(symbol) = reference.locator.reference.as_deref() {
        if chunk.symbol.as_deref() != Some(symbol) {
            return Err("evidence_not_found");
        }
    }
    let generation = source_generation(&project.id, project.last_indexed_at.as_deref());
    check_expected(reference, &chunk.file_hash, generation.as_ref())?;
    let captured_at_unix = project
        .last_indexed_at
        .as_deref()
        .and_then(parse_timestamp);
    Ok(CandidateEvidence {
        unit_id: format!("code:{}", chunk.id),
        source: "code".into(),
        content: chunk.content,
        locator: reference.locator.clone(),
        provenance: CODE_PROVENANCE.into(),
        generation: request.generation.clone(),
        fresh: project.index_status.as_deref() == Some("success") && generation.is_some(),
        required: false,
        captured_at_unix,
        content_hash: Some(chunk.file_hash.clone()),
        snapshot: Some(chunk.file_hash),
        source_generation: generation,
        tenant_scope: Some(auth.org_id.clone()),
        acl_generation: Some(acl_generation(conn, &auth.org_id)),
        policy_generation: Some(policy_generation(conn, &auth.org_id)),
    })
}

fn resolve_sdd(
    conn: &Connection,
    auth: &AuthContext,
    request: &AssembleRequest,
    reference: &EvidenceReference,
) -> Result<CandidateEvidence, &'static str> {
    if reference.locator.source != "sdd" || reference.locator.id.trim().is_empty() {
        return Err("unsupported_unverified_source");
    }
    let revision = reference
        .locator
        .reference
        .as_deref()
        .ok_or("unsupported_unverified_source")?
        .parse::<i64>()
        .map_err(|_| "evidence_not_found")?;
    if revision <= 0 {
        return Err("evidence_not_found");
    }
    let (_, project, _) = queries::get_sdd_artifact_metadata(conn, &auth.org_id, &reference.locator.id)
        .map_err(|_| "evidence_not_found")?
        .ok_or("evidence_not_found")?;
    if !project_is_visible_to_actor(conn, auth, &project).map_err(|_| "evidence_not_found")? {
        return Err("evidence_not_found");
    }
    let stored = queries::get_sdd_artifact_revision(
        conn,
        &auth.org_id,
        &reference.locator.id,
        revision,
    )
    .map_err(|_| "evidence_not_found")?
    .ok_or("evidence_not_found")?;
    let source_generation = GenerationRef {
        id: format!("sdd-artifact:{}", stored.artifact_id),
        version: stored.revision as u64,
    };
    check_expected(reference, &stored.content_hash, Some(&source_generation))?;
    let captured_at_unix = parse_timestamp(&stored.created_at);
    Ok(CandidateEvidence {
        unit_id: format!("sdd:{}:{}", stored.artifact_id, stored.revision),
        source: "sdd".into(),
        content: stored.content,
        locator: reference.locator.clone(),
        provenance: SDD_PROVENANCE.into(),
        generation: request.generation.clone(),
        fresh: true,
        required: false,
        captured_at_unix,
        content_hash: Some(stored.content_hash.clone()),
        snapshot: Some(format!("sha256:{}", stored.content_hash)),
        source_generation: Some(source_generation),
        tenant_scope: Some(auth.org_id.clone()),
        acl_generation: Some(acl_generation(conn, &auth.org_id)),
        policy_generation: Some(policy_generation(conn, &auth.org_id)),
    })
}

fn chunk_project_id(conn: &Connection, chunk_id: i64) -> Result<i64, &'static str> {
    conn.query_row(
        "SELECT code_project_id FROM code_chunks WHERE id = ?1",
        [chunk_id],
        |row| row.get(0),
    )
    .map_err(|_| "evidence_not_found")
}

fn parse_timestamp(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.timestamp())
}

fn acl_generation(conn: &Connection, org_id: &str) -> u64 {
    conn.query_row(
        "SELECT count(*) || ':' || COALESCE(group_concat(pm.role), '')
         FROM project_members pm JOIN projects p ON p.id = pm.project_id WHERE p.org_id = ?1",
        [org_id],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .map(|value| stable_hash(&value))
    .unwrap_or_default()
}

fn policy_generation(conn: &Connection, org_id: &str) -> u64 {
    conn.query_row(
        "SELECT count(*) || ':' || COALESCE(max(updated_at), '') FROM policies WHERE org_id = ?1",
        [org_id],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .map(|value| stable_hash(&value))
    .unwrap_or_default()
}

fn stable_hash(value: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{connection::connect, migrations, queries},
        models::types::{AuthContext, SaveArtifactRequest},
    };

    fn setup() -> (Connection, AuthContext, i64, i64, String) {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        let (org, admin, _) = queries::bootstrap(&conn, "Acme", "acme", "a@acme.test", "Admin").unwrap();
        let (member, _) = queries::invite_user(&conn, &org.id, "m@acme.test", "Member", "member").unwrap();
        let canonical = queries::create_project(&conn, &org.id, "visible", None, None).unwrap();
        queries::upsert_project_member(&conn, &canonical.id, &member.id, "member").unwrap();
        let code_project_id = queries::upsert_code_project(&conn, &org.id, "visible", "/repo").unwrap();
        queries::set_code_project_success(&conn, code_project_id, 1, "2026-08-02T00:00:00Z").unwrap();
        let chunk_id = queries::insert_code_chunk(&conn, code_project_id, "src/lib.rs", "hash-v1", Some("rust"), Some("f"), 1, 2, "fn f() {}", None).unwrap();
        let auth = AuthContext { org_id: org.id, user_id: member.id, role: "member".parse().unwrap() };
        (conn, auth, code_project_id, chunk_id, admin.id)
    }

    fn request(source: &str, locator: crate::context_fabric::Locator) -> AssembleRequest {
        AssembleRequest {
            contract_version: crate::context_fabric::CONTRACT_VERSION.into(),
            tokenizer: "whitespace-v0".into(), token_budget: 20, source_cap: 2,
            excluded_sources: vec![], required_sources: vec![],
            generation: GenerationRef { id: "g1".into(), version: 1 },
            profile_id: None, profile_version: None, candidates: vec![],
            references: vec![EvidenceReference { source: source.into(), locator, expected_hash: None, expected_generation: None }],
            freshness_window_secs: None,
        }
    }

    #[test]
    fn visible_code_is_resolved_and_hash_generation_are_checked() {
        let (conn, auth, _, chunk_id, _) = setup();
        let mut req = request("code", crate::context_fabric::Locator { source: "code".into(), id: chunk_id.to_string(), reference: Some("f".into()) });
        req.references[0].expected_hash = Some("hash-v1".into());
        let candidate = resolve(&conn, &auth, &req).unwrap().remove(0);
        assert_eq!(candidate.content, "fn f() {}");
        assert_eq!(candidate.content_hash.as_deref(), Some("hash-v1"));
        req.references[0].expected_hash = Some("hash-tampered".into());
        assert_eq!(resolve(&conn, &auth, &req), Err("evidence_integrity_mismatch"));
        assert!(serde_json::from_value::<EvidenceReference>(serde_json::json!({
            "source": "code",
            "locator": {"source": "code", "id": "1"},
            "content": "client tamper"
        })).is_err());
    }

    #[test]
    fn hidden_code_and_cross_tenant_sdd_are_not_enumerable() {
        let (conn, auth, _, _, admin_id) = setup();
        let visible_artifact = queries::upsert_sdd_artifact(&conn, &auth.org_id, &admin_id, &SaveArtifactRequest {
            project: "visible".into(), change_name: "change".into(), kind: "design".into(), capability: None,
            content: "visible design".into(), path: None, git_commit: None, git_ref: None, source: None,
        }, "agent").unwrap().0;
        let visible_req = request("sdd", crate::context_fabric::Locator { source: "sdd".into(), id: visible_artifact.id, reference: Some("1".into()) });
        assert_eq!(resolve(&conn, &auth, &visible_req).unwrap()[0].content, "visible design");
        let hidden = queries::create_project(&conn, &auth.org_id, "hidden", None, None).unwrap();
        let hidden_code = queries::upsert_code_project(&conn, &auth.org_id, "hidden", "/hidden").unwrap();
        let hidden_chunk = queries::insert_code_chunk(&conn, hidden_code, "secret.rs", "hash-secret", None, None, 1, 1, "secret", None).unwrap();
        let hidden_req = request("code", crate::context_fabric::Locator { source: "code".into(), id: hidden_chunk.to_string(), reference: None });
        assert_eq!(resolve(&conn, &auth, &hidden_req), Err("evidence_not_found"));
        let artifact = queries::upsert_sdd_artifact(&conn, &auth.org_id, &admin_id, &SaveArtifactRequest {
            project: "hidden".into(), change_name: "change".into(), kind: "design".into(), capability: None,
            content: "private design".into(), path: None, git_commit: None, git_ref: None, source: None,
        }, "agent").unwrap().0;
        let sdd_req = request("sdd", crate::context_fabric::Locator { source: "sdd".into(), id: artifact.id, reference: Some("1".into()) });
        assert_eq!(resolve(&conn, &auth, &sdd_req), Err("evidence_not_found"));
        let other = "other-org";
        assert_eq!(queries::get_sdd_artifact_metadata(&conn, other, "missing").unwrap(), None);
        let _ = hidden;
    }
}
