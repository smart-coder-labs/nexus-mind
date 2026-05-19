use serde::{Deserialize, Serialize};

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
        };
        assert!(m.tags.is_empty());
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
}
