use clap::Parser;

#[derive(Parser, Clone, Debug)]
#[command(name = "nexusmind", about = "NexusMind — enterprise memory control plane")]
pub struct Config {
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

    /// Sets the `Secure` attribute on the session cookie. Defaults to true and
    /// should STAY true in any real deployment: with it off the session token
    /// travels in cleartext and anyone on the network path can steal the session.
    ///
    /// It exists as an escape hatch for deployments that are not yet behind TLS,
    /// because browsers silently DROP a `Secure` cookie on an insecure origin —
    /// login returns 200 and the user is bounced straight back to the login page.
    /// Note that `http://localhost` counts as a secure context, so this is only
    /// ever needed when serving over plain HTTP on a real host or IP.
    ///
    /// SECURITY DEBT: the u2s self-host box sets this to false (see
    /// deploy/u2s/docker-compose.yml). Put it behind TLS and drop the override.
    #[arg(long, env = "COOKIE_SECURE", default_value_t = true)]
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
