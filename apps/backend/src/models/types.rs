use serde::{Deserialize, Serialize};

fn default_scope() -> String {
    "project".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Org {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct User {
    pub id: String,
    pub org_id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub status: String,
    pub created_at: String,
}

/// Injected by auth middleware into every authenticated request.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AuthContext {
    pub org_id: String,
    pub user_id: String,
    pub role: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Memory {
    pub id: String,
    pub org_id: String,
    pub user_id: String,
    pub project: String,
    pub tool: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: String,
    // v2 fields
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub memory_type: Option<String>,
    #[serde(default = "default_scope")]
    pub scope: String,
    pub topic_key: Option<String>,
    pub session_id: Option<String>,
    #[serde(default = "default_revision_count")]
    pub revision_count: i64,
    pub normalized_hash: Option<String>,
}

fn default_revision_count() -> i64 {
    1
}

/// Request body for `POST /v1/memory/store`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StoreMemoryRequest {
    pub project: Option<String>,
    pub tool: String,
    pub content: String,
    pub tags: Option<Vec<String>>,
    // v2 optional fields
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub memory_type: Option<String>,
    pub scope: Option<String>,
    pub topic_key: Option<String>,
    pub session_id: Option<String>,
}

/// A session groups a set of memories under a logical work unit.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Session {
    pub id: String,
    pub org_id: String,
    pub project: String,
    pub directory: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub summary: Option<String>,
}

/// Request body for `POST /v1/sessions`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateSessionRequest {
    pub project: String,
    pub directory: Option<String>,
    pub summary: Option<String>,
}

/// Request body for `PATCH /v1/sessions/:id`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PatchSessionRequest {
    pub ended_at: Option<String>,
    pub summary: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AuditEntry {
    pub id: String,
    pub org_id: String,
    pub user_id: String,
    pub timestamp: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiError {
    pub error: String,
    pub code: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ToolUsage {
    pub tool: String,
    pub count: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OrgStats {
    pub total_memories: i64,
    pub active_users_24h: i64,
    pub searches_today: i64,
    pub top_tools: Vec<ToolUsage>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn org_roundtrip() {
        let org = Org {
            id: "org1".into(),
            name: "Acme Corp".into(),
            slug: "acme".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&org).unwrap();
        let back: Org = serde_json::from_str(&json).unwrap();
        assert_eq!(org, back);
    }

    #[test]
    fn auth_context_has_required_fields() {
        let ctx = AuthContext {
            org_id: "org1".into(),
            user_id: "u1".into(),
            role: "admin".into(),
        };
        assert_eq!(ctx.org_id, "org1");
        assert_eq!(ctx.role, "admin");
    }

    #[test]
    fn memory_tags_default_empty() {
        let m = Memory {
            id: "m1".into(),
            org_id: "org1".into(),
            user_id: "u1".into(),
            project: "default".into(),
            tool: "claude".into(),
            content: "use snake_case".into(),
            tags: vec![],
            created_at: "2026-01-01T00:00:00Z".into(),
            title: None,
            memory_type: None,
            scope: "project".into(),
            topic_key: None,
            session_id: None,
            revision_count: 1,
            normalized_hash: None,
        };
        assert!(m.tags.is_empty());
        assert_eq!(m.scope, "project");
        assert_eq!(m.revision_count, 1);
    }

    #[test]
    fn audit_entry_optional_resource_id() {
        let entry = AuditEntry {
            id: "a1".into(),
            org_id: "org1".into(),
            user_id: "u1".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            action: "search".into(),
            resource_type: "memory".into(),
            resource_id: None,
            metadata: json!({}),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: AuditEntry = serde_json::from_str(&json).unwrap();
        assert!(back.resource_id.is_none());
    }

    // ── v2 struct tests ───────────────────────────────────────────────────────

    #[test]
    fn store_memory_request_deserializes_v2_optional_fields() {
        // Full v2 request with all new fields
        let json_str = r#"{
            "tool": "claude",
            "content": "use snake_case",
            "title": "Convention: naming",
            "type": "decision",
            "scope": "personal",
            "topic_key": "arch/naming",
            "session_id": "s1"
        }"#;
        let req: StoreMemoryRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(req.title.as_deref(), Some("Convention: naming"));
        assert_eq!(req.memory_type.as_deref(), Some("decision"));
        assert_eq!(req.scope.as_deref(), Some("personal"));
        assert_eq!(req.topic_key.as_deref(), Some("arch/naming"));
        assert_eq!(req.session_id.as_deref(), Some("s1"));
    }

    #[test]
    fn store_memory_request_legacy_fields_still_work() {
        // Legacy request — no v2 fields
        let json_str = r#"{"tool": "claude", "content": "use anyhow"}"#;
        let req: StoreMemoryRequest = serde_json::from_str(json_str).unwrap();
        assert!(req.title.is_none());
        assert!(req.memory_type.is_none());
        assert!(req.topic_key.is_none());
        assert!(req.session_id.is_none());
        // scope should default to None (handler applies the "project" default)
        assert!(req.scope.is_none());
    }

    #[test]
    fn memory_v2_fields_serialize_correctly() {
        let m = Memory {
            id: "m1".into(),
            org_id: "org1".into(),
            user_id: "u1".into(),
            project: "p".into(),
            tool: "claude".into(),
            content: "content".into(),
            tags: vec![],
            created_at: "2026-01-01T00:00:00Z".into(),
            title: Some("My title".into()),
            memory_type: Some("bugfix".into()),
            scope: "project".into(),
            topic_key: Some("k1".into()),
            session_id: None,
            revision_count: 2,
            normalized_hash: Some("abc123".into()),
        };
        let json_val: serde_json::Value = serde_json::to_value(&m).unwrap();
        assert_eq!(json_val["title"], "My title");
        assert_eq!(json_val["type"], "bugfix");
        assert_eq!(json_val["scope"], "project");
        assert_eq!(json_val["revision_count"], 2);
    }

    #[test]
    fn session_struct_roundtrip() {
        let s = Session {
            id: "s1".into(),
            org_id: "org1".into(),
            project: "nexusmind".into(),
            directory: "/home/user".into(),
            started_at: "2026-01-01T00:00:00Z".into(),
            ended_at: Some("2026-01-01T01:00:00Z".into()),
            summary: Some("Done".into()),
        };
        let json_str = serde_json::to_string(&s).unwrap();
        let back: Session = serde_json::from_str(&json_str).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn create_session_request_project_required() {
        let with_project: Result<CreateSessionRequest, _> =
            serde_json::from_str(r#"{"project": "nexusmind"}"#);
        assert!(with_project.is_ok());

        let without_project: Result<CreateSessionRequest, _> =
            serde_json::from_str(r#"{"directory": "/tmp"}"#);
        assert!(without_project.is_err(), "project is required");
    }

    #[test]
    fn patch_session_request_optional_fields() {
        let req: PatchSessionRequest =
            serde_json::from_str(r#"{"ended_at": "2026-01-01T01:00:00Z", "summary": "Done"}"#)
                .unwrap();
        assert_eq!(req.ended_at.as_deref(), Some("2026-01-01T01:00:00Z"));
        assert_eq!(req.summary.as_deref(), Some("Done"));

        let empty: PatchSessionRequest = serde_json::from_str("{}").unwrap();
        assert!(empty.ended_at.is_none());
        assert!(empty.summary.is_none());
    }
}
