use anyhow::{Context, Result};
use lettre::{
    message::{MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    transport::smtp::client::TlsParameters,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

#[derive(Clone, Debug)]
pub struct EmailConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub smtp_from: String,
    pub app_base_url: String,
}

impl EmailConfig {
    /// Returns base URL with any trailing slash removed.
    pub fn base_url(&self) -> &str {
        self.app_base_url.trim_end_matches('/')
    }
}

// ── Shared mailer builder ─────────────────────────────────────────────────────

fn build_mailer(config: &EmailConfig) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
    let creds = Credentials::new(config.smtp_username.clone(), config.smtp_password.clone());
    let tls = TlsParameters::new(config.smtp_host.clone())
        .context("failed to build TLS parameters")?;
    Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp_host)
        .port(config.smtp_port)
        .tls(lettre::transport::smtp::client::Tls::Required(tls))
        .credentials(creds)
        .build())
}

// ── HTML template ─────────────────────────────────────────────────────────────

struct EmailTemplate<'a> {
    name: &'a str,
    heading: &'a str,
    body_text: &'a str,
    cta_label: &'a str,
    cta_url: &'a str,
    footer_note: &'a str,
}

fn render_html(t: &EmailTemplate<'_>) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>{heading}</title>
</head>
<body style="margin:0;padding:0;background-color:#050810;font-family:-apple-system,BlinkMacSystemFont,'Inter','Segoe UI',Roboto,sans-serif;">
  <table width="100%" cellpadding="0" cellspacing="0" role="presentation"
         style="background-color:#050810;padding:48px 20px;">
    <tr>
      <td align="center">

        <!-- Card -->
        <table width="560" cellpadding="0" cellspacing="0" role="presentation"
               style="background-color:#0d1117;border:1px solid #1a2d3a;border-radius:16px;overflow:hidden;width:100%;max-width:560px;">

          <!-- Accent bar -->
          <tr>
            <td style="background:linear-gradient(90deg,#22d3ee 0%,#06b6d4 100%);height:3px;font-size:0;line-height:0;">&nbsp;</td>
          </tr>

          <!-- Body -->
          <tr>
            <td style="padding:40px 40px 32px;">

              <!-- Brand -->
              <table cellpadding="0" cellspacing="0" role="presentation" style="margin-bottom:36px;">
                <tr>
                  <td>
                    <span style="font-size:18px;font-weight:700;color:#f0f9ff;letter-spacing:-0.03em;">Nexus</span><span style="font-size:18px;font-weight:700;color:#22d3ee;letter-spacing:-0.03em;">Mind</span>
                  </td>
                </tr>
              </table>

              <!-- Heading -->
              <h1 style="margin:0 0 10px;font-size:22px;font-weight:600;color:#f0f9ff;letter-spacing:-0.02em;line-height:1.3;">{heading}</h1>

              <!-- Greeting + body -->
              <p style="margin:0 0 6px;font-size:15px;color:#94a3b8;line-height:1.7;">Hi {name},</p>
              <p style="margin:0 0 32px;font-size:15px;color:#94a3b8;line-height:1.7;">{body_text}</p>

              <!-- CTA button -->
              <table cellpadding="0" cellspacing="0" role="presentation" style="margin-bottom:32px;">
                <tr>
                  <td style="border-radius:10px;background-color:#22d3ee;">
                    <a href="{cta_url}"
                       style="display:inline-block;padding:13px 32px;font-size:15px;font-weight:600;color:#050810;text-decoration:none;letter-spacing:-0.01em;border-radius:10px;">
                      {cta_label}
                    </a>
                  </td>
                </tr>
              </table>

              <!-- Fallback URL -->
              <p style="margin:0 0 24px;font-size:12px;color:#475569;line-height:1.6;">
                Or copy this link into your browser:<br>
                <a href="{cta_url}" style="color:#22d3ee;text-decoration:none;word-break:break-all;">{cta_url}</a>
              </p>

              <!-- Footer note -->
              <p style="margin:0;font-size:13px;color:#475569;line-height:1.6;">{footer_note}</p>

            </td>
          </tr>

          <!-- Divider + footer -->
          <tr>
            <td style="border-top:1px solid #111827;padding:20px 40px;">
              <p style="margin:0;font-size:12px;color:#334155;line-height:1.5;">
                © 2025 NexusMind &nbsp;·&nbsp; Enterprise Memory Control Plane
              </p>
            </td>
          </tr>

        </table>
        <!-- /Card -->

      </td>
    </tr>
  </table>
</body>
</html>"#,
        heading = t.heading,
        name = t.name,
        body_text = t.body_text,
        cta_label = t.cta_label,
        cta_url = t.cta_url,
        footer_note = t.footer_note,
    )
}

fn render_plain(t: &EmailTemplate<'_>) -> String {
    format!(
        "Hi {name},\n\n{body_text}\n\n{cta_label}:\n{cta_url}\n\n{footer_note}\n\n— NexusMind",
        name = t.name,
        body_text = t.body_text,
        cta_label = t.cta_label,
        cta_url = t.cta_url,
        footer_note = t.footer_note,
    )
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Sent when a new org is created and the admin needs to set their initial password.
pub async fn send_password_setup(
    config: &EmailConfig,
    to_email: &str,
    to_name: &str,
    reset_token: &str,
) -> Result<()> {
    let url = format!("{}/set-password?token={}", config.base_url(), reset_token);
    let t = EmailTemplate {
        name: to_name,
        heading: "Set your NexusMind password",
        body_text: "Your NexusMind organization has been created. Click the button below to set your password and get started.",
        cta_label: "Set password",
        cta_url: &url,
        footer_note: "This link expires in 24 hours. If you did not expect this email, you can safely ignore it.",
    };
    send_email(config, to_email, to_name, "Set your NexusMind password", &t).await
}

/// Sent when an existing admin requests a password reset.
pub async fn send_password_reset(
    config: &EmailConfig,
    to_email: &str,
    to_name: &str,
    reset_token: &str,
) -> Result<()> {
    let url = format!("{}/set-password?token={}", config.base_url(), reset_token);
    let t = EmailTemplate {
        name: to_name,
        heading: "Reset your NexusMind password",
        body_text: "We received a request to reset the password for your NexusMind account. Click the button below to choose a new password.",
        cta_label: "Reset password",
        cta_url: &url,
        footer_note: "This link expires in 24 hours. If you did not request a password reset, you can safely ignore this email.",
    };
    send_email(config, to_email, to_name, "Reset your NexusMind password", &t).await
}

async fn send_email(
    config: &EmailConfig,
    to_email: &str,
    to_name: &str,
    subject: &str,
    t: &EmailTemplate<'_>,
) -> Result<()> {
    let email = Message::builder()
        .from(config.smtp_from.parse().context("invalid SMTP_FROM address")?)
        .to(format!("{to_name} <{to_email}>").parse().context("invalid recipient address")?)
        .subject(subject)
        .multipart(
            MultiPart::alternative()
                .singlepart(SinglePart::plain(render_plain(t)))
                .singlepart(SinglePart::html(render_html(t))),
        )
        .context("failed to build email message")?;

    build_mailer(config)?.send(email).await.context("failed to send email")?;
    Ok(())
}
