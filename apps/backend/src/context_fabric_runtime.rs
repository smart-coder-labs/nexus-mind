//! Opt-in runtime controls for the new Context Fabric only.
//!
//! This deliberately does not replace the legacy memory/search paths. Cache entries are
//! scoped by the complete authorization and generation identity and are never persisted.

use crate::context_fabric::GenerationRef;
use serde::{Deserialize, Serialize};
use std::{collections::{HashMap, HashSet}, sync::{Arc, Mutex}, time::{Duration, SystemTime, UNIX_EPOCH}};

pub const BASELINE_LANE: &str = "baseline";
pub const CANDIDATE_LANE: &str = "candidate";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum CacheStage { Retrieval, Compile, Generate, Verify, Memory }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CacheIdentity {
    pub tenant: String,
    pub caller_scope: String,
    pub caller_user: String,
    pub project: String,
    pub acl_generation: u64,
    pub policy_generation: u64,
    pub profile: String,
    pub captured_generation: GenerationRef,
    pub freshness: String,
    pub source_type: String,
    pub contract_version: String,
    pub lane: String,
    pub budget: Option<usize>,
    pub tokenizer: Option<String>,
    pub stage: CacheStage,
}

impl CacheIdentity {
    pub fn is_isolated(&self) -> bool {
        !self.tenant.trim().is_empty()
            && !self.caller_scope.trim().is_empty()
            && !self.caller_user.trim().is_empty()
            && !self.contract_version.trim().is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheRecord {
    /// Opaque stage output. The service does not inspect or expose it through diagnostics.
    pub value: Vec<u8>,
    pub expires_at_unix: i64,
    pub authorized: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CacheStats {
    pub enabled: bool,
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub puts: u64,
    pub invalidations: u64,
    pub expirations: u64,
    pub last_invalidation_reason: Option<String>,
    pub invalidation_events: u64,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvalidationEvent {
    #[serde(default)]
    pub event_id: String,
    pub tenant: String,
    pub reason: String,
    pub project: Option<String>,
    pub memory_id: Option<String>,
    pub acl_generation: Option<u64>,
    pub policy_generation: Option<u64>,
    pub generation: Option<GenerationRef>,
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RolloutState {
    pub shadow_enabled: bool,
    pub canary_enabled: bool,
    pub promotion_enabled: bool,
    pub baseline_fallback: bool,
    pub active_lane: String,
    pub active_profile: Option<String>,
    pub active_generation: Option<GenerationRef>,
    pub approval_operator: Option<String>,
    pub last_manifest_evidence: Option<String>,
    pub last_run_evidence: Option<String>,
}

impl Default for RolloutState {
    fn default() -> Self { Self { shadow_enabled: false, canary_enabled: false, promotion_enabled: false, baseline_fallback: true,
        active_lane: BASELINE_LANE.into(), active_profile: None, active_generation: None,
        approval_operator: None, last_manifest_evidence: None, last_run_evidence: None } }
}

#[derive(Clone)]
pub struct ContextFabricRuntime {
    enabled: bool,
    default_ttl: Duration,
    entries: Arc<Mutex<HashMap<CacheIdentity, CacheRecord>>>,
    stats: Arc<Mutex<CacheStats>>,
    rollout: Arc<Mutex<RolloutState>>,
    seen_events: Arc<Mutex<HashSet<String>>>,
}

impl ContextFabricRuntime {
    pub fn new(enabled: bool) -> Self { Self::with_ttl(enabled, Duration::from_secs(60)) }

    pub fn with_ttl(enabled: bool, default_ttl: Duration) -> Self { Self { enabled, default_ttl, entries: Arc::new(Mutex::new(HashMap::new())),
        stats: Arc::new(Mutex::new(CacheStats { enabled, ..Default::default() })), rollout: Arc::new(Mutex::new(RolloutState::default())), seen_events: Arc::new(Mutex::new(HashSet::new())) } }

    pub fn from_env() -> Self {
        let enabled = ["NEXUSMIND_CONTEXT_FABRIC_CACHE", "CONTEXT_FABRIC_ENABLED"]
            .iter().any(|name| std::env::var(name).as_deref() == Ok("true"));
        let ttl = std::env::var("CONTEXT_FABRIC_CACHE_TTL_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|seconds| *seconds > 0)
            .unwrap_or(60);
        Self::with_ttl(enabled, Duration::from_secs(ttl))
    }
    pub fn enabled(&self) -> bool { self.enabled }
    pub fn ttl(&self, freshness_window_secs: Option<u64>) -> Duration {
        let configured = self.default_ttl.as_secs();
        let seconds = freshness_window_secs.map_or(configured, |window| window.min(configured));
        Duration::from_secs(seconds)
    }

    pub fn get(&self, identity: &CacheIdentity) -> Option<Vec<u8>> {
        if !self.enabled { return None; }
        let now = unix_now();
        let mut entries = self.entries.lock().expect("context cache lock");
        let mut stats = self.stats.lock().expect("context cache stats lock");
        match entries.get(identity) {
            Some(record) if record.authorized && record.expires_at_unix > now => { stats.hits += 1; Some(record.value.clone()) }
            Some(_) => { entries.remove(identity); stats.misses += 1; stats.expirations += 1; None }
            None => { stats.misses += 1; None }
        }
    }

    pub fn put(&self, identity: CacheIdentity, value: Vec<u8>, ttl: Duration, authorized: bool) -> bool {
        if !self.enabled || !identity.is_isolated() || !authorized || ttl.is_zero() { return false; }
        self.entries.lock().expect("context cache lock").insert(identity, CacheRecord {
            value, expires_at_unix: unix_now().saturating_add(ttl.as_secs() as i64), authorized,
        });
        self.stats.lock().expect("context cache stats lock").puts += 1;
        true
    }

    pub fn invalidate(&self, event: &InvalidationEvent) -> usize {
        if !self.enabled { return 0; }
        if !event.event_id.is_empty() && !self.seen_events.lock().expect("context cache events lock").insert(event.event_id.clone()) {
            return 0;
        }
        let mut entries = self.entries.lock().expect("context cache lock");
        let removed = entries.iter().filter(|(key, _)| {
            if key.tenant != event.tenant { return false; }
            event.project.as_deref().map_or(true, |p| key.project == p)
                && event.acl_generation.map_or(true, |g| key.acl_generation < g)
                && event.policy_generation.map_or(true, |g| key.policy_generation < g)
                && event.generation.as_ref().map_or(true, |g| key.captured_generation != *g)
                && event.profile.as_deref().map_or(true, |p| key.profile == p)
                && event.memory_id.as_deref().map_or(true, |id| {
                    key.source_type.split(',').any(|source| source == format!("memory:{id}"))
                })
        }).map(|(key, _)| key.clone()).collect::<Vec<_>>();
        let count = removed.len();
        for key in removed { entries.remove(&key); }
        let mut stats = self.stats.lock().expect("context cache stats lock");
        stats.invalidations += count as u64;
        stats.invalidation_events += 1;
        stats.last_invalidation_reason = Some(event.reason.clone());
        if !stats.reason_codes.contains(&event.reason) { stats.reason_codes.push(event.reason.clone()); }
        count
    }

    pub fn invalidate_memory(&self, tenant: &str, memory_id: &str, reason: &str) -> usize {
        self.invalidate(&InvalidationEvent { event_id: format!("memory:{tenant}:{memory_id}:{reason}"), tenant: tenant.into(), reason: reason.into(), project: None,
            memory_id: Some(memory_id.into()), acl_generation: None, policy_generation: None, generation: None, profile: None })
    }
    pub fn invalidate_generation(&self, tenant: &str, generation: &GenerationRef, reason: &str) -> usize {
        self.invalidate(&InvalidationEvent { event_id: format!("generation:{tenant}:{}:{}:{reason}", generation.id, generation.version), tenant: tenant.into(), reason: reason.into(), project: None,
            memory_id: None, acl_generation: None, policy_generation: None, generation: Some(generation.clone()), profile: None })
    }
    pub fn invalidate_policy(&self, tenant: &str, generation: u64, reason: &str) -> usize {
        // Policy rows do not expose a monotonic generation in the legacy schema.
        // A timestamp is not safe to compare with the opaque cache stamp, so
        // invalidate the tenant rather than risk serving an old authorization.
        let _ = generation;
        self.invalidate_all(tenant, reason)
    }
    pub fn invalidate_all(&self, tenant: &str, reason: &str) -> usize {
        self.invalidate(&InvalidationEvent { event_id: String::new(), tenant: tenant.into(), reason: reason.into(), project: None, memory_id: None, acl_generation: None, policy_generation: None, generation: None, profile: None })
    }
    pub fn invalidate_project(&self, tenant: &str, project: &str, reason: &str) -> usize {
        self.invalidate(&InvalidationEvent { event_id: String::new(), tenant: tenant.into(), reason: reason.into(), project: Some(project.into()), memory_id: None, acl_generation: None, policy_generation: None, generation: None, profile: None })
    }
    pub fn purge_expired(&self, tenant: &str) -> usize {
        if !self.enabled { return 0; }
        let now = unix_now();
        let mut entries = self.entries.lock().expect("context cache lock");
        let keys: Vec<_> = entries.iter().filter(|(key, record)| key.tenant == tenant && record.expires_at_unix <= now).map(|(key, _)| key.clone()).collect();
        for key in &keys { entries.remove(key); }
        if !keys.is_empty() {
            let mut stats = self.stats.lock().expect("context cache stats lock");
            stats.expirations += keys.len() as u64;
            stats.last_invalidation_reason = Some("memory_ttl_expired".into());
            if !stats.reason_codes.contains(&"memory_ttl_expired".into()) { stats.reason_codes.push("memory_ttl_expired".into()); }
        }
        keys.len()
    }
    pub fn stats(&self) -> CacheStats { let mut stats = self.stats.lock().expect("context cache stats lock"); stats.entries = self.entries.lock().expect("context cache lock").len(); stats.clone() }
    pub fn rollout(&self) -> RolloutState { self.rollout.lock().expect("rollout lock").clone() }
    pub fn set_rollout(&self, state: RolloutState) { *self.rollout.lock().expect("rollout lock") = state; }
}

fn unix_now() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64 }

#[cfg(test)]
mod tests {
    use super::*;
    fn key(tenant: &str, user: &str) -> CacheIdentity { CacheIdentity { tenant: tenant.into(), caller_scope: "project".into(), caller_user: user.into(), project: "p".into(), acl_generation: 1, policy_generation: 1, profile: "baseline".into(), captured_generation: GenerationRef { id: "g".into(), version: 1 }, freshness: "bounded:60".into(), source_type: "memory:m1".into(), contract_version: "context-fabric.v0".into(), lane: BASELINE_LANE.into(), budget: Some(10), tokenizer: Some("whitespace-v0".into()), stage: CacheStage::Retrieval } }
    #[test] fn cache_is_opt_in_and_isolated_by_tenant_and_user() { let c = ContextFabricRuntime::new(true); let a = key("a", "u"); let mut b = a.clone(); b.tenant = "b".into(); let mut u = a.clone(); u.caller_user = "other".into(); assert!(c.put(a.clone(), b"safe".to_vec(), Duration::from_secs(60), true)); assert_eq!(c.get(&a), Some(b"safe".to_vec())); assert_eq!(c.get(&b), None); assert_eq!(c.get(&u), None); }
    #[test] fn unauthorized_values_are_never_put() { let c = ContextFabricRuntime::new(true); assert!(!c.put(key("a", "u"), b"secret".to_vec(), Duration::from_secs(60), false)); }
    #[test] fn policy_generation_invalidates_only_old_entries() { let c = ContextFabricRuntime::new(true); let k = key("a", "u"); c.put(k.clone(), vec![1], Duration::from_secs(60), true); let n = c.invalidate(&InvalidationEvent { event_id: "policy-1".into(), tenant: "a".into(), reason: "policy_changed".into(), policy_generation: Some(2), project: None, memory_id: None, acl_generation: None, generation: None, profile: None }); assert_eq!(n, 1); assert!(c.get(&k).is_none()); }
    #[test]
    fn memory_invalidation_isolated_by_memory_and_tenant() {
        let c = ContextFabricRuntime::new(true);
        let matching = key("tenant-a", "u");
        let mut other_memory = matching.clone();
        other_memory.source_type = "memory:m2".into();
        let mut other_tenant = matching.clone();
        other_tenant.tenant = "tenant-b".into();
        c.put(matching.clone(), vec![1], Duration::from_secs(60), true);
        c.put(other_memory.clone(), vec![2], Duration::from_secs(60), true);
        c.put(other_tenant.clone(), vec![3], Duration::from_secs(60), true);

        assert_eq!(c.invalidate_memory("tenant-a", "m1", "memory_deleted"), 1);
        assert_eq!(c.get(&matching), None);
        assert_eq!(c.get(&other_memory), Some(vec![2]));
        assert_eq!(c.get(&other_tenant), Some(vec![3]));
    }
    #[test] fn generation_invalidation_removes_old_but_keeps_target_and_replay_is_idempotent() {
        let c = ContextFabricRuntime::new(true);
        let old = key("a", "u");
        let mut current = old.clone(); current.captured_generation = GenerationRef { id: "g2".into(), version: 2 };
        c.put(old.clone(), vec![1], Duration::from_secs(60), true);
        c.put(current.clone(), vec![2], Duration::from_secs(60), true);
        let event = InvalidationEvent { event_id: "generation-2".into(), tenant: "a".into(), reason: "generation_changed".into(), project: None, memory_id: None, acl_generation: None, policy_generation: None, generation: Some(current.captured_generation.clone()), profile: None };
        assert_eq!(c.invalidate(&event), 1);
        assert_eq!(c.invalidate(&event), 0);
        assert_eq!(c.get(&old), None);
        assert_eq!(c.get(&current), Some(vec![2]));
    }
}
