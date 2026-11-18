use clap::Parser;

#[derive(Parser, Clone, Debug)]
#[command(
    name = "nexusmind",
    about = "NexusMind — enterprise memory control plane"
)]
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

    /// Lifetime of a browser session, in hours. Default 168 (7 days).
    ///
    /// A login mints a `web-session` API key whose `expires_at` is this far
    /// ahead of now — `validate_api_key` refuses the key past that point — and
    /// the session cookie carries the same `Max-Age` so the browser and the
    /// server agree on when the session dies.
    ///
    /// Read through [`Config::session_ttl_hours_clamped`], which bounds the
    /// value to 1 hour .. 1 year.
    #[arg(long, env = "SESSION_TTL_HOURS", default_value_t = 168)]
    pub session_ttl_hours: u64,

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

    #[arg(long, env = "AUTONOMOUS_AGENTS_ENABLED", default_value_t = false)]
    pub autonomous_agents_enabled: bool,

    /// Absolute, operator-pinned Claude Code executable path. The worker never
    /// resolves PATH aliases because that would make the runtime mutable.
    #[arg(long, env = "CLAUDE_CODE_BIN", default_value = "/usr/local/bin/claude")]
    pub claude_code_bin: String,

    /// Nexus harness executable. It connects to an independently managed
    /// OpenShell gateway; the backend never receives the gateway's Docker socket.
    #[arg(long, env = "NEXUS_WORKER_BIN", default_value = "/app/nexus")]
    pub nexus_worker_bin: String,

    #[arg(
        long,
        env = "CLAUDE_CODE_PROBE_INTERVAL_SECONDS",
        default_value_t = 300
    )]
    pub claude_code_probe_interval_seconds: u64,

    #[arg(long, env = "AUTONOMOUS_AGENT_POLL_SECONDS", default_value_t = 15)]
    pub autonomous_agent_poll_seconds: u64,
}

impl Config {
    /// Session lifetime in hours, bounded to 1 hour .. 1 year and narrowed to
    /// the `i64` the session code works in. A `SESSION_TTL_HOURS` of 0 would
    /// otherwise mint a session that is already expired when the login
    /// response reaches the browser, and an absurd value would overflow the
    /// cookie `Max-Age`.
    pub fn session_ttl_hours_clamped(&self) -> i64 {
        self.session_ttl_hours.clamp(1, 24 * 365) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::Config;
    use clap::Parser;

    fn parse(args: &[&str]) -> Config {
        Config::try_parse_from(args).expect("config must parse")
    }

    #[test]
    fn session_ttl_defaults_to_one_week() {
        let config = parse(&["nexusmind"]);
        assert_eq!(config.session_ttl_hours, 168);
        assert_eq!(config.session_ttl_hours_clamped(), 168);
    }

    #[test]
    fn session_ttl_is_clamped_to_a_usable_range() {
        assert_eq!(
            parse(&["nexusmind", "--session-ttl-hours", "0"]).session_ttl_hours_clamped(),
            1,
            "a zero TTL must not mint an already-expired session"
        );
        assert_eq!(
            parse(&["nexusmind", "--session-ttl-hours", "999999999"]).session_ttl_hours_clamped(),
            24 * 365
        );
    }
}
