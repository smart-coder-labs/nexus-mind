pub mod sqlite;

use anyhow::Result;

use crate::models::types::{Memory, StoreMemoryRequest};

/// Filters for listing memories.
pub struct MemoryFilters<'a> {
    pub user_id:     Option<&'a str>,
    pub tool:        Option<&'a str>,
    pub project:     Option<&'a str>,
    pub memory_type: Option<&'a str>,
    pub scope:       Option<&'a str>,
    pub limit:       i64,
    pub offset:      i64,
}

/// Controls how the `search` method retrieves memories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    /// Full-text search only (FTS5). Default, always available.
    Keyword,
    /// Embedding-based cosine KNN only. Falls back to `Keyword` when no embed service.
    Semantic,
    /// Hybrid: FTS5 + cosine KNN merged via Reciprocal Rank Fusion (k=60).
    /// Falls back to `Keyword` when no embed service.
    Hybrid,
}

impl SearchMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "semantic" => SearchMode::Semantic,
            "hybrid"   => SearchMode::Hybrid,
            _          => SearchMode::Keyword,
        }
    }
}

/// Backend-agnostic memory storage interface.
///
/// `SqliteStore` is the current implementation. A future `PostgresStore` will implement the
/// same trait behind the `postgres` feature flag — no changes to handlers required.
pub trait MemoryStore: Send + Sync {
    /// Insert or upsert a memory (upsert when `req.topic_key` is set).
    /// Implementations should write an audit event after a successful write.
    fn store(&self, org_id: &str, user_id: &str, req: &StoreMemoryRequest) -> Result<Memory>;

    /// Search memories using the given mode. Implementations that lack an embed service
    /// should silently fall back to `Keyword` for `Semantic` and `Hybrid` modes.
    /// Implementations should write a `search` audit event.
    fn search(&self, org_id: &str, user_id: &str, query: &str, limit: i64, mode: SearchMode) -> Result<Vec<Memory>>;

    /// List memories with optional filters.
    fn list(&self, org_id: &str, filters: &MemoryFilters<'_>) -> Result<Vec<Memory>>;

    /// Fetch a single memory by id, scoped to the org. Returns `None` if not found.
    fn get(&self, org_id: &str, memory_id: &str) -> Result<Option<Memory>>;

    /// Delete a memory. Returns `true` if it existed and was removed.
    /// Implementations should write a `delete` audit event on success.
    fn delete(&self, org_id: &str, user_id: &str, memory_id: &str) -> Result<bool>;

    /// Returns `true` when `session_id` belongs to `org_id`. Used to validate session
    /// references before storing a memory.
    fn validate_session(&self, org_id: &str, session_id: &str) -> Result<bool>;
}
