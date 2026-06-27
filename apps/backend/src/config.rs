use clap::Parser;

/// Runtime configuration for the NexusMind server.
///
/// Values are read from CLI flags or environment variables (env var names shown
/// in the `env` attribute of each field). All optional fields default to `None`,
/// which disables the corresponding feature (SMTP, GitHub OAuth, etc.).
#[derive(Parser, Clone, Debug)]
#[command(name = "nexusmind", about = "NexusMind — enterprise memory control plane")]
pub struct Config {
    /// TCP port to listen on. Env: `PORT`.
    #[arg(long, env = "PORT", default_value = "8080")]
    pub port: u16,

    /// Path to the SQLite database file. Env: `DB_PATH`.
    #[arg(long, env = "DB_PATH", default_value = "./data/nexusmind.db")]
    pub db_path: String,

    /// Tracing log level (`trace`, `debug`, `info`, `warn`, `error`). Env: `LOG_LEVEL`.
    #[arg(long, env = "LOG_LEVEL", default_value = "info")]
    pub log_level: String,

    /// Comma-separated list of allowed CORS origins, or `*` to allow all. Env: `CORS_ORIGINS`.
    #[arg(long, env = "CORS_ORIGINS", default_value = "*")]
    pub cors_origins: String,

    /// Optional bearer token that grants super-admin access across all orgs. Env: `SUPERUSER_KEY`.
    #[arg(long, env = "SUPERUSER_KEY")]
    pub superuser_key: Option<String>,

    /// SMTP server hostname for outbound email. Env: `SMTP_HOST`.
    #[arg(long, env = "SMTP_HOST", default_value = "smtp.gmail.com")]
    pub smtp_host: String,

    /// SMTP server port. Env: `SMTP_PORT`.
    #[arg(long, env = "SMTP_PORT", default_value_t = 587)]
    pub smtp_port: u16,

    /// SMTP authentication username. When absent, email sending is disabled. Env: `SMTP_USERNAME`.
    #[arg(long, env = "SMTP_USERNAME")]
    pub smtp_username: Option<String>,

    /// SMTP authentication password. Env: `SMTP_PASSWORD`.
    #[arg(long, env = "SMTP_PASSWORD")]
    pub smtp_password: Option<String>,

    /// From address used for outbound emails. Env: `SMTP_FROM`.
    #[arg(long, env = "SMTP_FROM")]
    pub smtp_from: Option<String>,

    /// Public base URL of the frontend app — used in email links. Env: `APP_BASE_URL`.
    #[arg(long, env = "APP_BASE_URL", default_value = "http://localhost:5173")]
    pub app_base_url: String,

    /// Origin of the admin UI — always allowed by CORS. Env: `ADMIN_ORIGIN`.
    #[arg(long, env = "ADMIN_ORIGIN", default_value = "http://localhost:3000")]
    pub admin_origin: String,

    /// Set `Secure` flag on session cookies. Should be `true` in production (HTTPS). Env: `COOKIE_SECURE`.
    #[arg(long, env = "COOKIE_SECURE", default_value_t = false)]
    pub cookie_secure: bool,

    /// GitHub OAuth App client ID. Required for GitHub OAuth flows. Env: `GITHUB_CLIENT_ID`.
    #[arg(long, env = "GITHUB_CLIENT_ID")]
    pub github_client_id: Option<String>,

    /// GitHub OAuth App client secret. Required for GitHub OAuth flows. Env: `GITHUB_CLIENT_SECRET`.
    #[arg(long, env = "GITHUB_CLIENT_SECRET")]
    pub github_client_secret: Option<String>,

    /// OAuth callback URL registered with the GitHub App. Env: `GITHUB_REDIRECT_URI`.
    #[arg(long, env = "GITHUB_REDIRECT_URI")]
    pub github_redirect_uri: Option<String>,
}
