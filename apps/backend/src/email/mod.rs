use anyhow::{Context, Result};
use lettre::{
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use lettre::transport::smtp::client::TlsParameters;

#[derive(Clone, Debug)]
pub struct EmailConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub smtp_from: String,
    pub app_base_url: String,
}

pub async fn send_password_setup(config: &EmailConfig, to_email: &str, to_name: &str, reset_token: &str) -> Result<()> {
    let setup_url = format!("{}/set-password?token={}", config.app_base_url, reset_token);

    let body = format!(
        "Hi {to_name},\n\n\
        Your NexusMind organization has been created. Set your password to get started:\n\n\
        {setup_url}\n\n\
        This link expires in 24 hours.\n\n\
        If you did not expect this email, you can ignore it.\n\n\
        — NexusMind"
    );

    let email = Message::builder()
        .from(config.smtp_from.parse().context("invalid SMTP_FROM address")?)
        .to(format!("{to_name} <{to_email}>").parse().context("invalid recipient address")?)
        .subject("Set your NexusMind password")
        .header(ContentType::TEXT_PLAIN)
        .body(body)
        .context("failed to build email message")?;

    let creds = Credentials::new(config.smtp_username.clone(), config.smtp_password.clone());

    let tls = TlsParameters::new(config.smtp_host.clone())
        .context("failed to build TLS parameters")?;

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp_host)
        .port(config.smtp_port)
        .tls(lettre::transport::smtp::client::Tls::Required(tls))
        .credentials(creds)
        .build();

    mailer.send(email).await.context("failed to send email")?;
    Ok(())
}
