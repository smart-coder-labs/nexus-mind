//! Runtime configuration for the NexusMind server.
//!
//! Values are parsed from CLI flags or environment variables via [`clap`].
//! All fields with a `default_value` are optional at runtime; fields typed
//! `Option<…>` are disabled when absent (e.g. SMTP, GitHub OAuth).

use clap::Parser;

/// Top-level server configuration.
///
/// Populated from CLI flags or environment variables at startup. The clap
/// derive macro generates both a `--flag` argument and an `ENV_VAR` binding
/// for every field. Environment variables take precedence over defaults;
/// explicit CLI flags take precedence over environment variables.
#[derive(Parser, Clone, Debug)]
#[command(name = "nexusmind", about = "NexusMind — enterprise memory control plane")]
pub struct Config {
    /// TCP port the HTTP server listens on. Env: `PORT`.
    #[arg(long, env = "PORT", default_value = "8080")]
    pub port: u16,

    /// Path to the SQLite database file. Use `:memory:` for in-process tests.
    /// Env: `DB_PATH`.
    #[arg(long, env = "DB_PATH", default_value = "./data/nexusmind.db")]
    pub db_path: String,

    /// Tracing log level passed to `tracing_subscriber` (e.g. `info`, `debug`).
    /// Env: `LOG_LEVEL`.
    #[arg(long, env = "LOG_LEVEL", default_value = "info")]
    pub log_level: String,

    /// Comma-separated list of allowed CORS origins, or `*` to allow any origin.
    /// Env: `CORS_ORIGINS`.
    #[arg(long, env = "CORS_ORIGINS", default_value = "*")]
    pub cors_origins: String,

    /// Optional bearer token that grants cross-org superuser access.
    /// When absent, no superuser endpoint is available. Env: `SUPERUSER_KEY`.
    #[arg(long, env = "SUPERUSER_KEY")]
    pub superuser_key: Option<String>,

    /// SMTP server hostname used for transactional email. Env: `SMTP_HOST`.
    #[arg(long, env = "SMTP_HOST", default_value = "smtp.gmail.com")]
    pub smtp_host: String,

    /// SMTP server port (typically 587 for STARTTLS or 465 for SSL).
    /// Env: `SMTP_PORT`.
    #[arg(long, env = "SMTP_PORT", default_value_t = 587)]
    pub smtp_port: u16,

    /// SMTP authentication username. Email sending is disabled when absent.
    /// Env: `SMTP_USERNAME`.
    #[arg(long, env = "SMTP_USERNAME")]
    pub smtp_username: Option<String>,

    /// SMTP authentication password. Email sending is disabled when absent.
    /// Env: `SMTP_PASSWORD`.
    #[arg(long, env = "SMTP_PASSWORD")]
    pub smtp_password: Option<String>,

    /// `From:` address used in outgoing emails. Email sending is disabled when absent.
    /// Env: `SMTP_FROM`.
    #[arg(long, env = "SMTP_FROM")]
    pub smtp_from: Option<String>,

    /// Public base URL of the web app, used in email links (e.g. password resets).
    /// Env: `APP_BASE_URL`.
    #[arg(long, env = "APP_BASE_URL", default_value = "http://localhost:5173")]
    pub app_base_url: String,

    /// Origin of the admin frontend, added to CORS allow-list automatically.
    /// Env: `ADMIN_ORIGIN`.
    #[arg(long, env = "ADMIN_ORIGIN", default_value = "http://localhost:3000")]
    pub admin_origin: String,

    /// Whether session cookies should have the `Secure` attribute set.
    /// Must be `true` in production (HTTPS); set to `false` for local HTTP dev.
    /// Env: `COOKIE_SECURE`.
    #[arg(long, env = "COOKIE_SECURE", default_value_t = false)]
    pub cookie_secure: bool,

    /// GitHub OAuth app client ID, required for GitHub repository indexing.
    /// When absent the `/v1/github/*` endpoints return 501. Env: `GITHUB_CLIENT_ID`.
    #[arg(long, env = "GITHUB_CLIENT_ID")]
    pub github_client_id: Option<String>,

    /// GitHub OAuth app client secret. Required alongside `github_client_id`.
    /// Env: `GITHUB_CLIENT_SECRET`.
    #[arg(long, env = "GITHUB_CLIENT_SECRET")]
    pub github_client_secret: Option<String>,

    /// OAuth redirect URI registered in the GitHub app settings.
    /// Env: `GITHUB_REDIRECT_URI`.
    #[arg(long, env = "GITHUB_REDIRECT_URI")]
    pub github_redirect_uri: Option<String>,
}
