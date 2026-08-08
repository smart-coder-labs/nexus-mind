use clap::{Args, Parser};

#[derive(Args, Clone, Debug, PartialEq)]
pub struct ContextFabricConfig {
    #[arg(long, env = "CONTEXT_FABRIC_ENABLED", default_value_t = false)]
    pub enabled: bool,
    #[arg(long, env = "CONTEXT_FABRIC_BQ_ENABLED", default_value = "off")]
    pub bq_enabled: String,
    #[arg(long, env = "CONTEXT_FABRIC_MRL_ENABLED", default_value = "off")]
    pub mrl_enabled: String,
    #[arg(
        long,
        env = "CONTEXT_FABRIC_TOOL_SEARCH_ENABLED",
        default_value_t = false
    )]
    pub tool_search_enabled: bool,
    #[arg(
        long,
        env = "CONTEXT_FABRIC_PROFILE",
        default_value = "nomic-768-f32-baseline"
    )]
    pub profile: String,
    #[arg(long, env = "CONTEXT_FABRIC_GENERATION", default_value = "baseline")]
    pub generation: String,
    #[arg(
        long,
        env = "CONTEXT_FABRIC_FRESHNESS_SECONDS",
        default_value_t = 86_400
    )]
    pub freshness_seconds: u64,
    #[arg(long, env = "CONTEXT_FABRIC_TOKEN_BUDGET", default_value_t = 4_096)]
    pub token_budget: usize,
    #[arg(long, env = "CONTEXT_FABRIC_SOURCE_CAP", default_value_t = 20)]
    pub source_cap: usize,
    #[arg(long, env = "CONTEXT_FABRIC_DIAGNOSTICS", default_value_t = true)]
    pub diagnostics: bool,
}

impl Default for ContextFabricConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bq_enabled: "off".into(),
            mrl_enabled: "off".into(),
            tool_search_enabled: false,
            profile: "nomic-768-f32-baseline".into(),
            generation: "baseline".into(),
            freshness_seconds: 86_400,
            token_budget: 4_096,
            source_cap: 20,
            diagnostics: true,
        }
    }
}

#[derive(Parser, Clone, Debug)]
#[command(
    name = "nexusmind",
    about = "NexusMind — enterprise memory control plane"
)]
pub struct Config {
    #[command(flatten)]
    pub context_fabric: ContextFabricConfig,

    #[arg(long, env = "PORT", default_value = "8080")]
    pub port: u16,

    #[arg(long, env = "DB_PATH", default_value = "./data/nexusmind.db")]
    pub db_path: String,

    #[arg(long, env = "LOG_LEVEL", default_value = "info")]
    pub log_level: String,

    #[arg(long, env = "CORS_ORIGINS", default_value = "*")]
    pub cors_origins: String,

    #[arg(long, env = "SUPERUSER_KEY")]
    pub superuser_key: Option<String>,

    #[arg(long, env = "SMTP_HOST", default_value = "smtp.gmail.com")]
    pub smtp_host: String,

    #[arg(long, env = "SMTP_PORT", default_value_t = 587)]
    pub smtp_port: u16,

    #[arg(long, env = "SMTP_USERNAME")]
    pub smtp_username: Option<String>,

    #[arg(long, env = "SMTP_PASSWORD")]
    pub smtp_password: Option<String>,

    #[arg(long, env = "SMTP_FROM")]
    pub smtp_from: Option<String>,

    #[arg(long, env = "APP_BASE_URL", default_value = "http://localhost:5173")]
    pub app_base_url: String,

    #[arg(long, env = "ADMIN_ORIGIN", default_value = "http://localhost:3000")]
    pub admin_origin: String,

    #[arg(long, env = "COOKIE_SECURE", default_value_t = false)]
    pub cookie_secure: bool,

    #[arg(long, env = "GITHUB_CLIENT_ID")]
    pub github_client_id: Option<String>,

    #[arg(long, env = "GITHUB_CLIENT_SECRET")]
    pub github_client_secret: Option<String>,

    #[arg(long, env = "GITHUB_REDIRECT_URI")]
    pub github_redirect_uri: Option<String>,

    /// Connection string for the Postgres backup layer. When unset, the backup
    /// subsystem is disabled (background job is skipped, API endpoints return
    /// 503). Treat this value as a credential — never commit it.
    #[arg(long, env = "BACKUP_DATABASE_URL")]
    pub backup_database_url: Option<String>,

    /// Hours between automatic backups. Default 6. Manual backups via the API
    /// are unaffected by this setting.
    #[arg(long, env = "BACKUP_INTERVAL_HOURS", default_value_t = 6)]
    pub backup_interval_hours: u64,
}

#[cfg(test)]
mod tests {
    use super::ContextFabricConfig;

    #[test]
    fn context_fabric_defaults_are_safe_and_baseline_compatible() {
        let config = ContextFabricConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.bq_enabled, "off");
        assert_eq!(config.mrl_enabled, "off");
        assert!(!config.tool_search_enabled);
        assert_eq!(config.profile, "nomic-768-f32-baseline");
        assert_eq!(config.generation, "baseline");
        assert_eq!(config.token_budget, 4096);
        assert!(config.diagnostics);
    }
}
