//! Persistent, opt-in BQ/MRL sidecars for Context Fabric.
//!
//! Sidecars are derived from authorized Float32 rows. They never contain memory
//! content and are never used by the legacy search path.

use anyhow::{anyhow, Result};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const DIMENSION: usize = 768;
const BITS: i64 = 1;
const MRL_PREFIXES: &[usize] = &[64, 128, 256, 512, 768];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarBuildRequest {
    pub profile_id: String,
    pub profile_version: u32,
    pub generation_id: String,
    pub generation_version: u64,
    pub acl_generation: u64,
    pub policy_generation: u64,
    pub build_manifest: String,
    pub build_checksum: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub mrl_prefix_dimension: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RebuildSummary {
    pub status: String,
    pub processed: usize,
    pub built: usize,
    pub tombstoned: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SidecarStatus {
    pub active: i64,
    pub tombstoned: i64,
    pub failed: i64,
    pub cancelled: i64,
    pub rebuild_status: Option<String>,
    pub processed: i64,
    pub built: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SidecarShadowRequest {
    pub capability: String,
    pub profile_id: String,
    pub profile_version: u32,
    pub generation_id: String,
    pub generation_version: u64,
    pub acl_generation: u64,
    pub policy_generation: u64,
    pub build_manifest: String,
    pub build_checksum: String,
    pub query: Vec<f32>,
    pub k: usize,
    pub alpha: f32,
    #[serde(default)]
    pub prefix_dimension: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SidecarShadowResponse {
    pub baseline_ids: Vec<String>,
    pub candidate_ids: Vec<String>,
    pub rescored_ids: Vec<String>,
    pub fallback: String,
    pub reason_codes: Vec<String>,
    pub sidecar_count: usize,
}

fn validate_build(request: &SidecarBuildRequest) -> Result<(bool, bool, usize)> {
    if request.profile_id.trim().is_empty()
        || request.profile_version == 0
        || request.generation_id.trim().is_empty()
        || request.generation_version == 0
    {
        return Err(anyhow!("invalid_generation_identity"));
    }
    if request.build_manifest.trim().is_empty() || request.build_checksum.trim().is_empty() {
        return Err(anyhow!("missing_build_manifest_or_checksum"));
    }
    let expected_manifest_checksum = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(request.build_manifest.as_bytes()))
    );
    if request.build_checksum != expected_manifest_checksum {
        return Err(anyhow!("build_checksum_mismatch"));
    }
    let bq = request.capabilities.is_empty()
        || request
            .capabilities
            .iter()
            .any(|v| v.eq_ignore_ascii_case("bq"));
    let mrl = request.capabilities.is_empty()
        || request
            .capabilities
            .iter()
            .any(|v| v.eq_ignore_ascii_case("mrl"));
    if !bq && !mrl {
        return Err(anyhow!("unsupported_sidecar_capability"));
    }
    let prefix = request.mrl_prefix_dimension.unwrap_or(DIMENSION);
    if mrl && !MRL_PREFIXES.contains(&prefix) {
        return Err(anyhow!("unsupported_mrl_prefix_dimension"));
    }
    Ok((bq, mrl, prefix))
}

fn source_hash(blob: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(blob)))
}

fn cancelled(conn: &Connection, org: &str, request: &SidecarBuildRequest) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT status='cancelled' FROM cf_bq_mrl_rebuilds WHERE org_id=?1 AND profile_id=?2 AND profile_version=?3 AND generation_id=?4 AND generation_version=?5",
        rusqlite::params![org, request.profile_id, request.profile_version, request.generation_id, request.generation_version],
        |row| row.get(0),
    ).optional()?.unwrap_or(false))
}

fn set_rebuild(
    conn: &Connection,
    org: &str,
    request: &SidecarBuildRequest,
    status: &str,
    processed: usize,
    built: usize,
) -> Result<()> {
    conn.execute(
        "INSERT INTO cf_bq_mrl_rebuilds (org_id,profile_id,profile_version,generation_id,generation_version,status,processed,built,updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,datetime('now'))
         ON CONFLICT(org_id,profile_id,profile_version,generation_id,generation_version) DO UPDATE SET status=excluded.status,processed=excluded.processed,built=excluded.built,updated_at=excluded.updated_at",
        rusqlite::params![org, request.profile_id, request.profile_version, request.generation_id, request.generation_version, status, processed as i64, built as i64],
    )?;
    Ok(())
}

pub fn cancel_rebuild(conn: &Connection, org: &str, request: &SidecarBuildRequest) -> Result<()> {
    validate_build(request)?;
    set_rebuild(conn, org, request, "cancelled", 0, 0)
}

/// Rebuilds only from existing authorized Float32 rows. There is no model load,
/// network access, or cross-generation write.
pub fn rebuild(
    conn: &Connection,
    org: &str,
    viewer: Option<&str>,
    request: &SidecarBuildRequest,
) -> Result<RebuildSummary> {
    let (bq, mrl, prefix) = validate_build(request)?;
    set_rebuild(conn, org, request, "running", 0, 0)?;
    let rows = crate::db::queries::get_embeddings_for_org_visible(conn, org, viewer)?;
    let mut processed = 0usize;
    let mut built = 0usize;
    for (memory_id, blob) in rows {
        if cancelled(conn, org, request)? {
            return Ok(RebuildSummary {
                status: "cancelled".into(),
                processed,
                built,
                tombstoned: 0,
                skipped: 0,
            });
        }
        processed += 1;
        let dense = crate::embed::deserialize(&blob);
        if dense.len() != DIMENSION {
            continue;
        }
        let source = source_hash(&blob);
        let bits = crate::context_fabric::sign_bit_encode(&dense);
        let specs = [("bq", bq, DIMENSION), ("mrl", mrl, prefix)];
        for (capability, enabled, prefix_dimension) in specs {
            if !enabled {
                continue;
            }
            let sidecar = if capability == "bq" {
                bits.clone()
            } else {
                let mut value = bits.clone();
                value.truncate(prefix_dimension.div_ceil(8));
                value
            };
            // The manifest checksum binds the build; source_hash plus the
            // deterministic sidecar encoding bind each derived row.
            conn.execute(
                "INSERT INTO cf_bq_mrl_sidecars (org_id,memory_id,capability,profile_id,profile_version,generation_id,generation_version,dimension,bits,prefix_dimension,source_hash,acl_generation,policy_generation,build_manifest,build_checksum,sidecar,status,updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,'active',datetime('now'))
                 ON CONFLICT(org_id,memory_id,capability,profile_id,profile_version,generation_id,generation_version) DO UPDATE SET dimension=excluded.dimension,bits=excluded.bits,prefix_dimension=excluded.prefix_dimension,source_hash=excluded.source_hash,acl_generation=excluded.acl_generation,policy_generation=excluded.policy_generation,build_manifest=excluded.build_manifest,build_checksum=excluded.build_checksum,sidecar=excluded.sidecar,status='active',updated_at=datetime('now')",
                 rusqlite::params![org, memory_id, capability, request.profile_id, request.profile_version, request.generation_id, request.generation_version, DIMENSION as i64, BITS, if capability == "mrl" { Some(prefix_dimension as i64) } else { None }, source, request.acl_generation as i64, request.policy_generation as i64, request.build_manifest, request.build_checksum, sidecar],
            )?;
            built += 1;
        }
        set_rebuild(conn, org, request, "running", processed, built)?;
    }
    // Older generations remain durable for explicit rollback, but are never
    // eligible for a query whose identity points at this generation.
    let tombstoned = 0usize;
    set_rebuild(conn, org, request, "completed", processed, built)?;
    Ok(RebuildSummary {
        status: "completed".into(),
        processed,
        built,
        tombstoned,
        skipped: processed.saturating_sub(built),
    })
}

pub fn status(
    conn: &Connection,
    org: &str,
    request: Option<&SidecarBuildRequest>,
) -> Result<SidecarStatus> {
    let counts = ["active", "tombstoned", "failed", "cancelled"].map(|state| {
        conn.query_row(
            "SELECT COUNT(*) FROM cf_bq_mrl_sidecars WHERE org_id=?1 AND status=?2",
            rusqlite::params![org, state],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
    });
    let rebuild = request.and_then(|r| conn.query_row("SELECT status,processed,built FROM cf_bq_mrl_rebuilds WHERE org_id=?1 AND profile_id=?2 AND profile_version=?3 AND generation_id=?4 AND generation_version=?5", rusqlite::params![org,r.profile_id,r.profile_version,r.generation_id,r.generation_version], |row| Ok((row.get::<_,String>(0)?,row.get::<_,i64>(1)?,row.get::<_,i64>(2)?))).optional().ok().flatten());
    Ok(SidecarStatus {
        active: counts[0],
        tombstoned: counts[1],
        failed: counts[2],
        cancelled: counts[3],
        rebuild_status: rebuild.as_ref().map(|v| v.0.clone()),
        processed: rebuild.as_ref().map(|v| v.1).unwrap_or(0),
        built: rebuild.as_ref().map(|v| v.2).unwrap_or(0),
    })
}

pub fn verify(conn: &Connection, org: &str) -> Result<serde_json::Value> {
    let invalid: i64 = conn.query_row("SELECT COUNT(*) FROM cf_bq_mrl_sidecars WHERE org_id=?1 AND (dimension != 768 OR bits != 1 OR status NOT IN ('active','tombstoned','failed','cancelled'))", [org], |r| r.get(0))?;
    Ok(serde_json::json!({"valid": invalid == 0, "invalid": invalid}))
}

pub fn cleanup(conn: &Connection, org: &str) -> Result<usize> {
    Ok(conn.execute("DELETE FROM cf_bq_mrl_sidecars WHERE org_id=?1 AND status IN ('tombstoned','failed','cancelled')", [org])?)
}

/// Reads bits only after tenant/project visibility and identity checks. Dense
/// Float32 rows are used solely for rescore and are never returned.
pub fn shadow(
    conn: &Connection,
    org: &str,
    viewer: Option<&str>,
    request: &SidecarShadowRequest,
) -> Result<SidecarShadowResponse> {
    if request.query.len() != DIMENSION
        || request.k == 0
        || !request.alpha.is_finite()
        || request.alpha <= 0.0
    {
        return Err(anyhow!("sidecar_unavailable"));
    }
    let prefix = request.prefix_dimension.unwrap_or(DIMENSION);
    if !matches!(
        request.capability.to_ascii_lowercase().as_str(),
        "bq" | "mrl"
    ) || (request.capability.eq_ignore_ascii_case("mrl") && !MRL_PREFIXES.contains(&prefix))
    {
        return Err(anyhow!("sidecar_unavailable"));
    }
    let embeddings = crate::db::queries::get_embeddings_for_org_visible(conn, org, viewer)?;
    let dense: std::collections::HashMap<String, Vec<f32>> = embeddings
        .into_iter()
        .map(|(id, b)| (id, crate::embed::deserialize(&b)))
        .filter(|(_, v)| v.len() == DIMENSION)
        .collect();
    let mut stmt = conn.prepare("SELECT memory_id,sidecar,source_hash,build_checksum,build_manifest,dimension,prefix_dimension FROM cf_bq_mrl_sidecars WHERE org_id=?1 AND capability=?2 AND profile_id=?3 AND profile_version=?4 AND generation_id=?5 AND generation_version=?6 AND acl_generation=?7 AND policy_generation=?8 AND status='active'")?;
    let rows = stmt.query_map(
        rusqlite::params![
            org,
            request.capability.to_ascii_lowercase(),
            request.profile_id,
            request.profile_version,
            request.generation_id,
            request.generation_version,
            request.acl_generation as i64,
            request.policy_generation as i64
        ],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, Option<i64>>(6)?,
            ))
        },
    )?;
    let query_bits = crate::context_fabric::sign_bit_encode(&request.query);
    let mut candidates = Vec::new();
    let mut baseline = dense
        .iter()
        .map(|(id, vector)| {
            (
                id.clone(),
                1.0 - crate::embed::cosine(&request.query, vector),
            )
        })
        .collect::<Vec<_>>();
    for row in rows {
        let (id, bits, source, _row_checksum, manifest, dimension, row_prefix) = row?;
        let Some(vector) = dense.get(&id) else {
            continue;
        };
        let mut expected_sidecar = crate::context_fabric::sign_bit_encode(vector);
        if request.capability.eq_ignore_ascii_case("mrl") {
            expected_sidecar.truncate(prefix.div_ceil(8));
        }
        if dimension != DIMENSION as i64
            || (request.capability.eq_ignore_ascii_case("mrl") && row_prefix != Some(prefix as i64))
            || bits != expected_sidecar
            || manifest != request.build_manifest
            || format!(
                "sha256:{}",
                hex::encode(Sha256::digest(manifest.as_bytes()))
            ) != request.build_checksum
        {
            continue;
        }
        let blob = crate::embed::serialize(vector);
        if source_hash(&blob) != source {
            continue;
        }
        candidates.push((
            id,
            crate::context_fabric::hamming_distance(&query_bits, &bits) as f32,
        ));
    }
    if candidates.is_empty() {
        baseline.sort_by(|a, b| a.1.total_cmp(&b.1));
        return Ok(SidecarShadowResponse {
            baseline_ids: baseline.into_iter().take(request.k).map(|v| v.0).collect(),
            candidate_ids: vec![],
            rescored_ids: vec![],
            fallback: crate::context_fabric::BASELINE_PROFILE.into(),
            reason_codes: vec!["sidecar_unavailable".into()],
            sidecar_count: 0,
        });
    }
    baseline.sort_by(|a, b| a.1.total_cmp(&b.1));
    candidates.sort_by(|a, b| a.1.total_cmp(&b.1));
    let baseline_ids = baseline
        .iter()
        .take(request.k)
        .map(|v| v.0.clone())
        .collect::<Vec<_>>();
    let candidate_ids = candidates
        .iter()
        .take(((request.k as f32 * request.alpha).ceil() as usize).min(candidates.len()))
        .map(|v| v.0.clone())
        .collect::<Vec<_>>();
    let mut rescored = candidate_ids
        .iter()
        .filter_map(|id| {
            dense
                .get(id)
                .map(|v| (id.clone(), 1.0 - crate::embed::cosine(&request.query, v)))
        })
        .collect::<Vec<_>>();
    rescored.sort_by(|a, b| a.1.total_cmp(&b.1));
    Ok(SidecarShadowResponse {
        baseline_ids,
        candidate_ids,
        rescored_ids: rescored.into_iter().take(request.k).map(|v| v.0).collect(),
        fallback: crate::context_fabric::BASELINE_PROFILE.into(),
        reason_codes: vec!["shadow_only".into(), "manual_promotion_required".into()],
        sidecar_count: candidates.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{connection::connect, migrations, queries},
        models::types::StoreMemoryRequest,
    };

    fn setup() -> (Connection, String, String, String) {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        let (org, user, _) = queries::bootstrap(&conn, "Acme", "acme", "a@acme", "Admin").unwrap();
        migrations::apply_context_fabric(&conn).unwrap();
        migrations::apply_context_fabric_provenance(&conn).unwrap();
        migrations::apply_context_fabric_sidecar(&conn).unwrap();
        let memory = queries::upsert_memory(
            &conn,
            &org.id,
            &user.id,
            &StoreMemoryRequest {
                project: None,
                tool: "test".into(),
                content: "sidecar source".into(),
                tags: None,
                title: None,
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: None,
                context_fabric_metadata: None,
            },
        )
        .unwrap();
        queries::store_embedding(
            &conn,
            &memory.id,
            &crate::embed::serialize(&vec![1.0; DIMENSION]),
        )
        .unwrap();
        (conn, org.id, user.id, memory.id)
    }

    fn request() -> SidecarBuildRequest {
        let manifest = "manifest-v1";
        SidecarBuildRequest {
            profile_id: "bq-shadow".into(),
            profile_version: 1,
            generation_id: "g1".into(),
            generation_version: 1,
            acl_generation: 1,
            policy_generation: 1,
            build_manifest: manifest.into(),
            build_checksum: format!(
                "sha256:{}",
                hex::encode(Sha256::digest(manifest.as_bytes()))
            ),
            capabilities: vec!["bq".into(), "mrl".into()],
            mrl_prefix_dimension: Some(128),
        }
    }

    #[test]
    fn rebuild_is_idempotent_and_keeps_float32_authority() {
        let (conn, org, user, memory) = setup();
        let first = rebuild(&conn, &org, Some(&user), &request()).unwrap();
        let second = rebuild(&conn, &org, Some(&user), &request()).unwrap();
        assert_eq!(first.built, 2);
        assert_eq!(second.built, 2);
        assert_eq!(
            conn.query_row(
                "SELECT length(embedding) FROM memory_embeddings WHERE memory_id=?1",
                [&memory],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            (DIMENSION * 4) as i64
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM cf_bq_mrl_sidecars WHERE status='active'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
    }

    #[test]
    fn checksum_mismatch_and_generation_are_fail_closed() {
        let (conn, org, user, _) = setup();
        let mut bad = request();
        bad.build_checksum = "sha256:wrong".into();
        assert_eq!(
            rebuild(&conn, &org, Some(&user), &bad)
                .unwrap_err()
                .to_string(),
            "build_checksum_mismatch"
        );
        let mut different = request();
        different.generation_id = "g2".into();
        let result = rebuild(&conn, &org, Some(&user), &different).unwrap();
        assert_eq!(result.status, "completed");
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM cf_bq_mrl_sidecars WHERE generation_id='g1'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(conn.query_row("SELECT COUNT(*) FROM cf_bq_mrl_sidecars WHERE generation_id='g2' AND status='active'", [], |r| r.get::<_,i64>(0)).unwrap(), 2);
    }

    #[test]
    fn memory_update_and_delete_leave_tombstones() {
        let (conn, org, user, memory) = setup();
        rebuild(&conn, &org, Some(&user), &request()).unwrap();
        queries::update_memory_fields(&conn, &org, &memory, Some("changed"), None, None).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM cf_bq_mrl_sidecars WHERE status='tombstoned'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        queries::delete_memory(&conn, &org, &memory).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM cf_bq_mrl_sidecars WHERE status='tombstoned'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
    }

    #[test]
    fn shadow_falls_back_when_sidecar_is_absent() {
        let (conn, org, user, _) = setup();
        let response = shadow(
            &conn,
            &org,
            Some(&user),
            &SidecarShadowRequest {
                capability: "bq".into(),
                profile_id: "bq-shadow".into(),
                profile_version: 1,
                generation_id: "g1".into(),
                generation_version: 1,
                acl_generation: 1,
                policy_generation: 1,
                build_manifest: "manifest-v1".into(),
                build_checksum: request().build_checksum,
                query: vec![1.0; DIMENSION],
                k: 1,
                alpha: 2.0,
                prefix_dimension: None,
            },
        )
        .unwrap();
        assert_eq!(response.reason_codes, vec!["sidecar_unavailable"]);
        assert_eq!(response.fallback, crate::context_fabric::BASELINE_PROFILE);
    }

    #[test]
    fn shadow_uses_bq_and_mrl_sidecars_after_rebuild() {
        let (conn, org, user, memory) = setup();
        let build = request();
        rebuild(&conn, &org, Some(&user), &build).unwrap();
        let blob: Vec<u8> = conn
            .query_row(
                "SELECT embedding FROM memory_embeddings WHERE memory_id=?1",
                [&memory],
                |row| row.get(0),
            )
            .unwrap();
        let query = crate::embed::deserialize(&blob);
        for (capability, prefix_dimension) in [("bq", None), ("mrl", Some(128))] {
            let response = shadow(
                &conn,
                &org,
                Some(&user),
                &SidecarShadowRequest {
                    capability: capability.into(),
                    profile_id: build.profile_id.clone(),
                    profile_version: build.profile_version,
                    generation_id: build.generation_id.clone(),
                    generation_version: build.generation_version,
                    acl_generation: build.acl_generation,
                    policy_generation: build.policy_generation,
                    build_manifest: build.build_manifest.clone(),
                    build_checksum: build.build_checksum.clone(),
                    query: query.clone(),
                    k: 1,
                    alpha: 0.5,
                    prefix_dimension,
                },
            )
            .unwrap();
            assert_eq!(response.fallback, crate::context_fabric::BASELINE_PROFILE);
            assert!(response.reason_codes.contains(&"shadow_only".to_string()));
            assert_eq!(response.sidecar_count, 1);
            assert_eq!(response.rescored_ids, vec![memory.clone()]);
        }
    }
}
