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

    #[arg(long, env = "COOKIE_SECURE", default_value_t = false)]
    pub cookie_secure: bool,
}
