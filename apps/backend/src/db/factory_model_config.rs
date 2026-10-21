//! Persisted factory model preferences. Provider credentials are deliberately
//! never stored in this table; they remain in the provider-specific secret path.
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::queries::append_audit_chained;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProvider {
    Anthropic,
    OpenaiApi,
    ChatgptPlan,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelAssignment {
    pub provider: ModelProvider,
    pub model: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRoles {
    pub planner: ModelAssignment,
    pub reviewer: ModelAssignment,
    pub implementer: ModelAssignment,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialStatus {
    Unknown,
    NotConnected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CredentialStatuses {
    pub anthropic: CredentialStatus,
    pub openai_api: CredentialStatus,
    pub chatgpt_plan: CredentialStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StoredModelConfig {
    pub roles: ModelRoles,
    pub credentials: CredentialStatuses,
    /// The configured provider preferences are not yet executable through the
    /// factory model gateway. OAuth remains unavailable until explicit consent
    /// and a supported token store are implemented.
    pub execution_ready: bool,
    pub updated_by: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfigWrite {
    pub roles: ModelRoles,
}

impl Default for ModelRoles {
    fn default() -> Self {
        let sol = ModelAssignment {
            provider: ModelProvider::ChatgptPlan,
            model: "gpt-6.1-sol".into(),
        };
        Self {
            planner: sol.clone(),
            reviewer: sol,
            implementer: ModelAssignment {
                provider: ModelProvider::ChatgptPlan,
                model: "gpt-6-luna".into(),
            },
        }
    }
}

impl StoredModelConfig {
    fn new(roles: ModelRoles, updated_by: Option<String>, updated_at: Option<String>) -> Self {
        Self {
            roles,
            credentials: CredentialStatuses {
                // These credentials are only present in the separate egress
                // proxy pod; the control plane must not infer availability.
                anthropic: CredentialStatus::Unknown,
                openai_api: CredentialStatus::Unknown,
                // There is intentionally no ChatGPT OAuth connection yet.
                chatgpt_plan: CredentialStatus::NotConnected,
            },
            execution_ready: false,
            updated_by,
            updated_at,
        }
    }
}

pub fn get(conn: &Connection, org_id: &str) -> Result<StoredModelConfig> {
    let row = conn
        .query_row(
            "SELECT config_json, updated_by, updated_at FROM factory_model_configs WHERE org_id=?1",
            [org_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((raw, updated_by, updated_at)) = row else {
        return Ok(StoredModelConfig::new(ModelRoles::default(), None, None));
    };
    let roles: ModelRoles = serde_json::from_str(&raw)?;
    Ok(StoredModelConfig::new(roles, Some(updated_by), Some(updated_at)))
}

pub fn validate(roles: &ModelRoles) -> Result<(), String> {
    for (role, assignment) in [
        ("planner", &roles.planner),
        ("reviewer", &roles.reviewer),
        ("implementer", &roles.implementer),
    ] {
        let model = assignment.model.trim();
        if model.is_empty()
            || model.len() > 100
            || !model
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:[]".contains(&b))
        {
            return Err(format!("{role} model must be a short provider model identifier"));
        }
        let compatible = match assignment.provider {
            ModelProvider::Anthropic => {
                matches!(model, "haiku" | "sonnet" | "opus" | "opus[1m]")
                    || model.starts_with("claude-")
            }
            ModelProvider::OpenaiApi | ModelProvider::ChatgptPlan => {
                model.starts_with("gpt-")
            }
        };
        if !compatible {
            return Err(format!("{role} model is not compatible with its provider"));
        }
    }
    Ok(())
}

pub fn put(
    conn: &Connection,
    org_id: &str,
    user_id: &str,
    roles: &ModelRoles,
) -> Result<StoredModelConfig> {
    validate(roles).map_err(anyhow::Error::msg)?;
    let serialized = serde_json::to_string(roles)?;
    let tx = conn.unchecked_transaction()?;
    let previous = tx
        .query_row(
            "SELECT config_json FROM factory_model_configs WHERE org_id=?1",
            [org_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    tx.execute(
        "INSERT INTO factory_model_configs(org_id,config_json,updated_by,updated_at)
         VALUES(?1,?2,?3,datetime('now'))
         ON CONFLICT(org_id) DO UPDATE SET config_json=excluded.config_json,
           updated_by=excluded.updated_by, updated_at=datetime('now')",
        params![org_id, serialized, user_id],
    )?;
    append_audit_chained(
        &tx,
        org_id,
        user_id,
        "factory_model_config.update",
        "factory_model_config",
        Some(org_id),
        serde_json::json!({
            "roles_before": previous.and_then(|value| serde_json::from_str::<ModelRoles>(&value).ok()),
            "roles_after": roles,
            "secrets_stored": false
        }),
        None,
    )?;
    let stored = get(&tx, org_id)?;
    tx.commit()?;
    Ok(stored)
}
