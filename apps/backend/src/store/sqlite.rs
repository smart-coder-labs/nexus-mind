use anyhow::Result;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use crate::{
    db::queries,
    embed::{self, EmbedService},
    models::types::{Memory, MemoryPage, StoreMemoryRequest},
    store::{MemoryFilters, MemoryStore, SearchMode},
};

/// SQLite-backed memory store.
///
/// Wraps an `Arc<Mutex<Connection>>` so it is cheap to clone (Axum requires `Clone` on state).
/// Non-memory handlers that still call `queries::*` directly can access the raw connection
/// via [`SqliteStore::conn`].
///
/// Pass an `EmbedService` via [`SqliteStore::with_embed`] to enable semantic / hybrid search.
#[derive(Clone)]
pub struct SqliteStore {
    db:    Arc<Mutex<Connection>>,
    embed: Option<Arc<EmbedService>>,
}

impl SqliteStore {
    pub fn new(conn: Connection) -> Self {
        SqliteStore {
            db:    Arc::new(Mutex::new(conn)),
            embed: None,
        }
    }

    /// Attach an embedding service, enabling semantic and hybrid search.
    pub fn with_embed(mut self, svc: EmbedService) -> Self {
        self.embed = Some(Arc::new(svc));
        self
    }

    /// Returns a clone of the inner `Arc<Mutex<Connection>>` for handlers that use raw queries.
    pub fn conn(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.db)
    }

    /// Returns a clone of the optional embed service, for handlers that drive embedding directly.
    pub fn embed_service(&self) -> Option<Arc<EmbedService>> {
        self.embed.clone()
    }
}

// ── MemoryStore impl ──────────────────────────────────────────────────────────

impl MemoryStore for SqliteStore {
    fn store(&self, org_id: &str, user_id: &str, req: &StoreMemoryRequest) -> Result<Memory> {
        let conn = self.db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;

        if let Some(ref sid) = req.session_id {
            let valid = queries::validate_session_ownership(&conn, org_id, sid)?;
            if !valid {
                anyhow::bail!("invalid_session_id:{sid}");
            }
        }

        let memory = queries::upsert_memory(&conn, org_id, user_id, req)?;

        let _ = queries::log_audit(
            &conn,
            org_id,
            user_id,
            "store",
            "memory",
            Some(&memory.id),
            serde_json::json!({
                "tool": memory.tool,
                "project": memory.project,
                "title": memory.title,
                "type": memory.memory_type,
                "tags": memory.tags,
                "preview": memory.content.chars().take(160).collect::<String>(),
            }),
        );

        // Embed the content and persist the vector (best-effort — never fail the store call).
        if let Some(ref svc) = self.embed {
            match svc.embed_one(&memory.content) {
                Ok(vec) => {
                    let blob = embed::serialize(&vec);
                    if let Err(e) = queries::store_embedding(&conn, &memory.id, &blob) {
                        tracing::warn!("Failed to save embedding for memory {}: {e}", memory.id);
                    }
                }
                Err(e) => tracing::warn!("Failed to embed memory {}: {e}", memory.id),
            }
        }

        Ok(memory)
    }

    fn search(&self, org_id: &str, user_id: &str, query: &str, limit: i64, mode: SearchMode) -> Result<Vec<Memory>> {
        // Resolve effective mode: downgrade to Keyword if no embed service.
        let effective_mode = match mode {
            SearchMode::Semantic | SearchMode::Hybrid if self.embed.is_none() => {
                tracing::debug!("No embed service — falling back to Keyword search");
                SearchMode::Keyword
            }
            m => m,
        };

        let memories = match effective_mode {
            SearchMode::Keyword => {
                let conn = self.db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                queries::search_memories(&conn, org_id, query, limit)?
            }
            SearchMode::Semantic => self.search_semantic(org_id, query, limit)?,
            SearchMode::Hybrid   => self.search_hybrid(org_id, query, limit)?,
        };

        let conn = self.db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        let _ = queries::log_audit(
            &conn,
            org_id,
            user_id,
            "search",
            "memory",
            None,
            serde_json::json!({ "query": query, "mode": format!("{effective_mode:?}"), "results": memories.len() }),
        );

        Ok(memories)
    }

    fn list(&self, org_id: &str, filters: &MemoryFilters<'_>) -> Result<MemoryPage> {
        let conn = self.db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        let total = queries::count_memories(
            &conn,
            org_id,
            filters.user_id,
            filters.tool,
            filters.project,
            filters.memory_type,
            filters.scope,
            filters.session_id,
            filters.include_archived,
            filters.from_date,
            filters.to_date,
            filters.collection_id,
        )?;
        let memories = queries::list_memories(
            &conn,
            org_id,
            filters.user_id,
            filters.tool,
            filters.project,
            filters.memory_type,
            filters.scope,
            filters.session_id,
            filters.limit,
            filters.offset,
            filters.include_archived,
            filters.from_date,
            filters.to_date,
            filters.collection_id,
        )?;
        Ok(MemoryPage {
            memories,
            total,
            limit: filters.limit,
            offset: filters.offset,
        })
    }

    fn get(&self, org_id: &str, memory_id: &str) -> Result<Option<Memory>> {
        let conn = self.db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        let result = conn.query_row(
            "SELECT id, org_id, user_id, project, tool, content, tags, created_at,
                    title, type, scope, topic_key, session_id, revision_count, normalized_hash, project_id,
                    archived_at, pinned, collection_id, admin_note, delete_after
             FROM memories WHERE id = ?1 AND org_id = ?2",
            rusqlite::params![memory_id, org_id],
            |row| {
                let tags_str: String = row.get(6)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    tags_str,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, Option<String>>(12)?,
                    row.get::<_, Option<i64>>(13)?,
                    row.get::<_, Option<String>>(14)?,
                    row.get::<_, Option<String>>(15)?,
                    row.get::<_, Option<String>>(16)?,
                    row.get::<_, i64>(17).unwrap_or(0),
                    row.get::<_, Option<String>>(18)?,
                    row.get::<_, Option<String>>(19)?,
                    row.get::<_, Option<String>>(20)?,
                ))
            },
        );

        match result {
            Ok((id, org_id, user_id, project, tool, content, tags_str, created_at,
                title, memory_type, scope, topic_key, session_id, revision_count, normalized_hash, project_id,
                archived_at, pinned_i64, collection_id, admin_note, delete_after)) => {
                let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
                let status = if archived_at.is_some() { "archived".to_string() } else { "active".to_string() };
                Ok(Some(Memory {
                    id,
                    org_id,
                    user_id,
                    project,
                    tool,
                    content,
                    tags,
                    created_at,
                    title,
                    memory_type,
                    scope: scope.unwrap_or_else(|| "project".to_string()),
                    topic_key,
                    session_id,
                    revision_count: revision_count.unwrap_or(1),
                    normalized_hash,
                    project_id,
                    archived_at,
                    pinned: pinned_i64 != 0,
                    collection_id,
                    admin_note,
                    delete_after,
                    status,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn delete(&self, org_id: &str, user_id: &str, memory_id: &str) -> Result<bool> {
        let conn = self.db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        let deleted = queries::delete_memory(&conn, org_id, memory_id)?;

        if deleted {
            let _ = queries::log_audit(
                &conn,
                org_id,
                user_id,
                "delete",
                "memory",
                Some(memory_id),
                serde_json::json!({}),
            );
        }

        Ok(deleted)
    }

    fn validate_session(&self, org_id: &str, session_id: &str) -> Result<bool> {
        let conn = self.db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        queries::validate_session_ownership(&conn, org_id, session_id)
    }
}

// ── Private search helpers ────────────────────────────────────────────────────

impl SqliteStore {
    /// Pure semantic search: embed the query, cosine-rank all org embeddings, return top-K.
    fn search_semantic(&self, org_id: &str, query: &str, limit: i64) -> Result<Vec<Memory>> {
        let svc = self.embed.as_ref().expect("caller verified embed is Some");
        let q_vec = svc.embed_one(query)?;

        let pairs = {
            let conn = self.db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
            queries::get_embeddings_for_org(&conn, org_id)?
        };

        let mut scored: Vec<(String, f32)> = pairs
            .into_iter()
            .map(|(id, blob)| {
                let v = embed::deserialize(&blob);
                let score = embed::cosine(&q_vec, &v);
                (id, score)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit as usize);

        let ids: Vec<String> = scored.into_iter().map(|(id, _)| id).collect();
        let conn = self.db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        queries::get_memories_by_ids(&conn, org_id, &ids)
    }

    /// Hybrid search: merge FTS5 ranks and cosine ranks via Reciprocal Rank Fusion (k=60).
    fn search_hybrid(&self, org_id: &str, query: &str, limit: i64) -> Result<Vec<Memory>> {
        let svc = self.embed.as_ref().expect("caller verified embed is Some");
        let q_vec = svc.embed_one(query)?;

        // Fetch more candidates than needed before merging
        let fetch_n = (limit * 3).max(30);

        // FTS5 results (rank = position in result list, 1-based)
        let fts_ids: Vec<String> = {
            let conn = self.db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
            queries::search_memories(&conn, org_id, query, fetch_n)?
                .into_iter()
                .map(|m| m.id)
                .collect()
        };

        // Semantic KNN results
        let pairs = {
            let conn = self.db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
            queries::get_embeddings_for_org(&conn, org_id)?
        };

        let mut sem_scored: Vec<(String, f32)> = pairs
            .into_iter()
            .map(|(id, blob)| {
                let v = embed::deserialize(&blob);
                (id, embed::cosine(&q_vec, &v))
            })
            .collect();
        sem_scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        sem_scored.truncate(fetch_n as usize);
        let sem_ids: Vec<String> = sem_scored.into_iter().map(|(id, _)| id).collect();

        // RRF merge: score(id) = Σ 1 / (60 + rank_i + 1)  for each list that contains id
        let k = 60.0_f64;
        let mut rrf: std::collections::HashMap<String, f64> = std::collections::HashMap::new();

        for (rank, id) in fts_ids.iter().enumerate() {
            *rrf.entry(id.clone()).or_insert(0.0) += 1.0 / (k + rank as f64 + 1.0);
        }
        for (rank, id) in sem_ids.iter().enumerate() {
            *rrf.entry(id.clone()).or_insert(0.0) += 1.0 / (k + rank as f64 + 1.0);
        }

        let mut ranked: Vec<(String, f64)> = rrf.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(limit as usize);

        let ids: Vec<String> = ranked.into_iter().map(|(id, _)| id).collect();
        let conn = self.db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        queries::get_memories_by_ids(&conn, org_id, &ids)
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::connect, migrations};
    use crate::models::types::StoreMemoryRequest;
    use crate::store::SearchMode;

    fn make_store() -> (SqliteStore, String, String) {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        let store = SqliteStore::new(conn);
        let (org, user, _key) = {
            let c = store.conn();
            let c = c.lock().unwrap();
            queries::bootstrap(&c, "Acme", "acme", "a@acme.com", "Admin").unwrap()
        };
        (store, org.id, user.id)
    }

    fn req(content: &str) -> StoreMemoryRequest {
        StoreMemoryRequest {
            project: Some("proj".into()),
            tool: "claude".into(),
            content: content.into(),
            tags: None,
            title: None,
            memory_type: None,
            scope: None,
            topic_key: None,
            session_id: None,
        }
    }

    #[test]
    fn store_returns_memory() {
        let (store, org_id, user_id) = make_store();
        let mem = store.store(&org_id, &user_id, &req("hello")).unwrap();
        assert_eq!(mem.content, "hello");
        assert_eq!(mem.revision_count, 1);
    }

    #[test]
    fn search_returns_matches() {
        let (store, org_id, user_id) = make_store();
        store.store(&org_id, &user_id, &req("use snake_case")).unwrap();
        store.store(&org_id, &user_id, &req("unrelated content")).unwrap();
        let results = store.search(&org_id, &user_id, "snake_case", 10, SearchMode::Keyword).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn list_returns_all() {
        let (store, org_id, user_id) = make_store();
        store.store(&org_id, &user_id, &req("a")).unwrap();
        store.store(&org_id, &user_id, &req("b")).unwrap();
        let filters = MemoryFilters {
            user_id: None, tool: None, project: None,
            memory_type: None, scope: None, session_id: None, limit: 50, offset: 0, include_archived: false,
            from_date: None, to_date: None, collection_id: None,
        };
        let page = store.list(&org_id, &filters).unwrap();
        assert_eq!(page.memories.len(), 2);
        assert_eq!(page.total, 2);
        assert_eq!(page.limit, 50);
        assert_eq!(page.offset, 0);
    }

    #[test]
    fn get_returns_memory_by_id() {
        let (store, org_id, user_id) = make_store();
        let created = store.store(&org_id, &user_id, &req("findme")).unwrap();
        let found = store.get(&org_id, &created.id).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().content, "findme");
    }

    #[test]
    fn get_returns_none_for_wrong_org() {
        let (store, org_id, user_id) = make_store();
        let created = store.store(&org_id, &user_id, &req("secret")).unwrap();
        let found = store.get("wrong-org", &created.id).unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn delete_returns_true_then_false() {
        let (store, org_id, user_id) = make_store();
        let created = store.store(&org_id, &user_id, &req("to delete")).unwrap();
        assert!(store.delete(&org_id, &user_id, &created.id).unwrap());
        assert!(!store.delete(&org_id, &user_id, &created.id).unwrap());
    }

    #[test]
    fn list_date_range_filter_returns_only_matching_memories() {
        use crate::db::{connection::connect, migrations};
        use crate::db::queries;

        // Use direct DB access so we can control created_at timestamps precisely.
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        let (org, user, _) = queries::bootstrap(&conn, "TestOrg", "testorg", "t@t.com", "Admin").unwrap();

        // Insert two memories with known dates via raw SQL.
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, project, tool, content, tags, created_at, scope, revision_count)
             VALUES ('m1', ?1, ?2, 'proj', 'claude', 'old memory', '[]', '2025-01-10T10:00:00Z', 'project', 1)",
            rusqlite::params![org.id, user.id],
        ).unwrap();
        conn.execute(
            "INSERT INTO memories (id, org_id, user_id, project, tool, content, tags, created_at, scope, revision_count)
             VALUES ('m2', ?1, ?2, 'proj', 'claude', 'new memory', '[]', '2025-03-15T10:00:00Z', 'project', 1)",
            rusqlite::params![org.id, user.id],
        ).unwrap();

        // Filter: from 2025-02-01 to 2025-12-31 — should return only m2.
        let results = queries::list_memories(
            &conn, &org.id,
            None, None, None, None, None, None,
            50, 0, false,
            Some("2025-02-01"), Some("2025-12-31"), None,
        ).unwrap();
        assert_eq!(results.len(), 1, "expected 1 memory in range, got {}", results.len());
        assert_eq!(results[0].id, "m2");

        // Filter: from 2025-01-01 to 2025-01-31 — should return only m1.
        let results = queries::list_memories(
            &conn, &org.id,
            None, None, None, None, None, None,
            50, 0, false,
            Some("2025-01-01"), Some("2025-01-31"), None,
        ).unwrap();
        assert_eq!(results.len(), 1, "expected 1 memory in range, got {}", results.len());
        assert_eq!(results[0].id, "m1");

        // No date filter — should return both.
        let results = queries::list_memories(
            &conn, &org.id,
            None, None, None, None, None, None,
            50, 0, false,
            None, None, None,
        ).unwrap();
        assert_eq!(results.len(), 2);
    }
}
