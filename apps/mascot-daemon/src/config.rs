//! Mascot daemon configuration (`~/.nexusmind/mascot.yaml`).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use systemone_client::client::EndpointConfig;
use systemone_client::{DecisionMode, HybridPolicy, SystemOneRouter};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MascotConfig {
    #[serde(default = "default_mascot_name")]
    pub mascot_name: String,
    #[serde(default = "default_device_id")]
    pub device_id: String,
    #[serde(default)]
    pub ipc_socket_path: Option<PathBuf>,
    #[serde(default)]
    pub db_path: Option<PathBuf>,
    #[serde(default)]
    pub skills_dir: Option<PathBuf>,
    #[serde(default)]
    pub decision_engine: DecisionEngineConfig,
    #[serde(default)]
    pub autonomy: AutonomyConfig,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub privacy: PrivacyConfig,
    #[serde(default)]
    pub nexusmind: NexusMindSyncConfig,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
    #[serde(default)]
    pub routines: Vec<RoutineConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoutineConfig {
    pub id: String,
    pub title: String,
    pub prompt: String,
    pub every_minutes: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for MascotConfig {
    fn default() -> Self {
        Self {
            mascot_name: default_mascot_name(),
            device_id: default_device_id(),
            ipc_socket_path: None,
            db_path: None,
            skills_dir: None,
            decision_engine: DecisionEngineConfig::default(),
            autonomy: AutonomyConfig::default(),
            llm: LlmConfig::default(),
            privacy: PrivacyConfig::default(),
            nexusmind: NexusMindSyncConfig::default(),
            mcp_servers: Vec::new(),
            routines: Vec::new(),
        }
    }
}

fn default_mascot_name() -> String {
    "Nexi".into()
}

fn default_device_id() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "macos-primary".into())
}

pub fn default_config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".nexusmind")
}

impl MascotConfig {
    pub fn resolved_socket_path(&self) -> PathBuf {
        self.ipc_socket_path
            .clone()
            .unwrap_or_else(|| default_config_dir().join("mascot.sock"))
    }

    pub fn resolved_db_path(&self) -> PathBuf {
        self.db_path
            .clone()
            .unwrap_or_else(|| default_config_dir().join("mascot.db"))
    }

    pub fn resolved_skills_dir(&self) -> PathBuf {
        self.skills_dir
            .clone()
            .unwrap_or_else(|| default_config_dir().join("skills"))
    }

    pub fn resolved_secrets_path(&self) -> PathBuf {
        self.db_path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.join("secrets.env")))
            .unwrap_or_else(|| default_config_dir().join("secrets.env"))
    }

    pub fn resolved_config_path(&self) -> PathBuf {
        self.db_path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.join("mascot.yaml")))
            .unwrap_or_else(|| default_config_dir().join("mascot.yaml"))
    }

    pub fn load_or_default(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config at {}", path.display()))?;
        let parsed: Self = serde_yaml::from_str(&raw)
            .with_context(|| format!("failed to parse YAML config at {}", path.display()))?;
        Ok(parsed)
    }

    pub fn write_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let yaml = serde_yaml::to_string(self)?;
        std::fs::write(path, yaml)?;
        Ok(())
    }

    pub fn build_decision_router(&self) -> Result<SystemOneRouter> {
        self.build_decision_router_with_key(None)
    }

    pub fn build_decision_router_with_key(
        &self,
        key_override: Option<String>,
    ) -> Result<SystemOneRouter> {
        let jev_key = key_override
            .filter(|k| !k.trim().is_empty())
            .or_else(|| std::env::var(&self.decision_engine.jev.api_key_env).ok())
            .or_else(|| std::env::var("JEV_API_KEY").ok())
            .filter(|k| !k.trim().is_empty());

        SystemOneRouter::new(
            self.decision_engine.mode,
            EndpointConfig {
                endpoint: self.decision_engine.laya.endpoint.clone(),
                model: self.decision_engine.laya.model.clone(),
                api_key: None,
                timeout: Duration::from_millis(self.decision_engine.laya.timeout_ms),
            },
            EndpointConfig {
                endpoint: self.decision_engine.jev.endpoint.clone(),
                model: self.decision_engine.jev.model.clone(),
                api_key: jev_key,
                timeout: Duration::from_millis(self.decision_engine.jev.timeout_ms),
            },
            HybridPolicy {
                escalate_below_confidence: self.decision_engine.hybrid.escalate_below_confidence,
                allow_cloud_for_private_context: self
                    .decision_engine
                    .hybrid
                    .allow_cloud_for_private_context,
            },
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionEngineConfig {
    #[serde(default)]
    pub mode: DecisionMode,
    #[serde(default)]
    pub laya: LayaConfig,
    #[serde(default)]
    pub jev: JevConfig,
    #[serde(default)]
    pub hybrid: HybridConfig,
}

impl Default for DecisionEngineConfig {
    fn default() -> Self {
        Self {
            mode: DecisionMode::Hybrid,
            laya: LayaConfig::default(),
            jev: JevConfig::default(),
            hybrid: HybridConfig::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayaConfig {
    pub endpoint: String,
    pub runtime: String,
    pub model: String,
    pub timeout_ms: u64,
    pub auto_start_sidecar: bool,
}

impl Default for LayaConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8787/v1/systemone".into(),
            runtime: "mlx".into(),
            model: "laya-latest".into(),
            timeout_ms: 600,
            auto_start_sidecar: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JevConfig {
    pub endpoint: String,
    pub model: String,
    pub api_key_env: String,
    pub timeout_ms: u64,
}

impl Default for JevConfig {
    fn default() -> Self {
        Self {
            endpoint: "https://api.typesafe.ai/v1/systemone".into(),
            model: "jev-latest".into(),
            api_key_env: "TYPESAFE_API_KEY".into(),
            timeout_ms: 4500,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HybridConfig {
    pub escalate_below_confidence: f64,
    pub allow_cloud_for_private_context: bool,
}

impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            escalate_below_confidence: 0.80,
            allow_cloud_for_private_context: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyLevel {
    Observer,
    Copilot,
    #[default]
    SupervisedAutonomous,
    FullAutonomous,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutonomyConfig {
    #[serde(default)]
    pub level: AutonomyLevel,
    #[serde(default = "default_max_steps")]
    pub max_steps_per_task: u32,
    #[serde(default = "default_auto_risk")]
    pub auto_approve_max_risk: f64,
    #[serde(default = "default_min_conf")]
    pub min_confidence: f64,
}

fn default_max_steps() -> u32 {
    25
}
fn default_auto_risk() -> f64 {
    0.25
}
fn default_min_conf() -> f64 {
    0.85
}

impl Default for AutonomyConfig {
    fn default() -> Self {
        Self {
            level: AutonomyLevel::SupervisedAutonomous,
            max_steps_per_task: default_max_steps(),
            auto_approve_max_risk: default_auto_risk(),
            min_confidence: default_min_conf(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmProviderKind {
    #[default]
    Claude,
    Gemini,
    OpenAi,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default)]
    pub provider: LlmProviderKind,
    #[serde(default = "default_llm_model")]
    pub model: String,
    #[serde(default = "default_llm_key_env")]
    pub api_key_env: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(skip)]
    pub api_key_override: Option<String>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_llm_model() -> String {
    "claude-sonnet-4-5".into()
}
fn default_llm_key_env() -> String {
    "ANTHROPIC_API_KEY".into()
}
fn default_max_tokens() -> u32 {
    4096
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProviderKind::Claude,
            model: default_llm_model(),
            api_key_env: default_llm_key_env(),
            base_url: None,
            api_key_override: None,
            max_tokens: default_max_tokens(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrivacyConfig {
    #[serde(default)]
    pub privacy_shield_enabled: bool,
    #[serde(default = "default_true")]
    pub observe_active_window: bool,
    #[serde(default = "default_true")]
    pub observe_terminal_history: bool,
    #[serde(default)]
    pub observe_clipboard: bool,
    #[serde(default = "default_blocked_apps")]
    pub blocked_apps: Vec<String>,
    #[serde(default = "default_blocked_paths")]
    pub blocked_paths: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_blocked_apps() -> Vec<String> {
    vec![
        "1Password".into(),
        "Bitwarden".into(),
        "Keychain Access".into(),
        "Passwords".into(),
        "KeePassXC".into(),
    ]
}

fn default_blocked_paths() -> Vec<String> {
    vec![
        "/.ssh/".into(),
        "/.aws/".into(),
        "/.gnupg/".into(),
        "/Library/Keychains/".into(),
        ".env".into(),
    ]
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            privacy_shield_enabled: false,
            observe_active_window: true,
            observe_terminal_history: true,
            observe_clipboard: false,
            blocked_apps: default_blocked_apps(),
            blocked_paths: default_blocked_paths(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NexusMindSyncConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_nm_url")]
    pub base_url: String,
    #[serde(default = "default_nm_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_nm_project")]
    pub project: String,
}

fn default_nm_url() -> String {
    "http://localhost:8080".into()
}
fn default_nm_key_env() -> String {
    "NEXUSMIND_API_KEY".into()
}
fn default_nm_project() -> String {
    "personal-mascot".into()
}

impl Default for NexusMindSyncConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_url: default_nm_url(),
            api_key_env: default_nm_key_env(),
            project: default_nm_project(),
        }
    }
}
