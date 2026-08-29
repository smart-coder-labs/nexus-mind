//! LinkedIn integration for the AI Content Manager: OAuth 2.0 (authorization
//! code) to connect a member/organization, and the Posts API to publish an
//! approved text post. One LinkedIn Developer App per NexusMind instance,
//! configured via env: `LINKEDIN_CLIENT_ID`, `LINKEDIN_CLIENT_SECRET`, and
//! `PUBLIC_API_BASE_URL` (used to derive the OAuth redirect URI). Access tokens
//! are stored encrypted as `linkedin` connectors; nothing here persists secrets.

use anyhow::Result;
use serde::Deserialize;
use serde_json::json;

/// LinkedIn API version sent via the `LinkedIn-Version` header (format YYYYMM).
/// LinkedIn retires each monthly version ~12 months after release, so it is
/// overridable via `LINKEDIN_API_VERSION` to bump it without a redeploy when the
/// default is retired (a stale value returns HTTP 426 NONEXISTENT_VERSION).
fn api_version() -> String {
    std::env::var("LINKEDIN_API_VERSION")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| value.len() == 6 && value.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or_else(|| "202606".to_string())
}

/// OAuth scopes per destination. Personal profile posting needs `w_member_social`;
/// company-page posting needs `w_organization_social` plus admin read to resolve
/// which organizations the member administers.
pub fn scopes_for(destination: &str) -> &'static str {
    match destination {
        "organization" => "openid profile w_organization_social r_organization_admin",
        _ => "openid profile w_member_social",
    }
}

pub struct LinkedInApp {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

/// Load the instance's LinkedIn app config from the environment, or bail with a
/// clear code the API surfaces so the operator knows what to configure.
pub fn app_from_env() -> Result<LinkedInApp> {
    let client_id = std::env::var("LINKEDIN_CLIENT_ID")
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("linkedin_not_configured"))?;
    let client_secret = std::env::var("LINKEDIN_CLIENT_SECRET")
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("linkedin_not_configured"))?;
    let base = std::env::var("PUBLIC_API_BASE_URL")
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("public_api_base_url_missing"))?;
    let redirect_uri = format!(
        "{}/v1/autonomous-agents/linkedin/callback",
        base.trim_end_matches('/')
    );
    Ok(LinkedInApp {
        client_id,
        client_secret,
        redirect_uri,
    })
}

/// Build the LinkedIn authorization URL the user is redirected to.
pub fn authorize_url(app: &LinkedInApp, scope: &str, state: &str) -> Result<String> {
    let url = reqwest::Url::parse_with_params(
        "https://www.linkedin.com/oauth/v2/authorization",
        &[
            ("response_type", "code"),
            ("client_id", app.client_id.as_str()),
            ("redirect_uri", app.redirect_uri.as_str()),
            ("state", state),
            ("scope", scope),
        ],
    )?;
    Ok(url.to_string())
}

#[derive(Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
}

/// Exchange an authorization code for an access token.
pub async fn exchange_code(app: &LinkedInApp, code: &str) -> Result<TokenResponse> {
    token_request(
        app,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", app.redirect_uri.as_str()),
        ],
    )
    .await
}

/// Refresh an access token using a stored refresh token.
pub async fn refresh(app: &LinkedInApp, refresh_token: &str) -> Result<TokenResponse> {
    token_request(
        app,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ],
    )
    .await
}

async fn token_request(app: &LinkedInApp, extra: &[(&str, &str)]) -> Result<TokenResponse> {
    let mut form: Vec<(&str, &str)> = extra.to_vec();
    form.push(("client_id", app.client_id.as_str()));
    form.push(("client_secret", app.client_secret.as_str()));
    let response = reqwest::Client::new()
        .post("https://www.linkedin.com/oauth/v2/accessToken")
        .form(&form)
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!(
            "linkedin_oauth_{}: {}",
            status.as_u16(),
            text.chars().take(300).collect::<String>()
        );
    }
    Ok(serde_json::from_str(&text)?)
}

/// The authenticated member's URN (`urn:li:person:{sub}`) and display name via
/// the OpenID Connect userinfo endpoint.
pub async fn member_identity(access_token: &str) -> Result<(String, String)> {
    let info: serde_json::Value = reqwest::Client::new()
        .get("https://api.linkedin.com/v2/userinfo")
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let sub = info
        .get("sub")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("linkedin_userinfo_missing_sub"))?;
    let name = info
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("LinkedIn member")
        .to_string();
    Ok((format!("urn:li:person:{sub}"), name))
}

/// Organizations the member administers (URN + best-effort name). Requires the
/// `r_organization_admin` scope; returns an empty list if none are found.
pub async fn admin_organizations(access_token: &str) -> Result<Vec<(String, String)>> {
    let acls: serde_json::Value = reqwest::Client::new()
        .get("https://api.linkedin.com/rest/organizationAcls?q=roleAssignee&role=ADMINISTRATOR&state=APPROVED")
        .bearer_auth(access_token)
        .header("LinkedIn-Version", api_version())
        .header("X-Restli-Protocol-Version", "2.0.0")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let mut orgs = Vec::new();
    if let Some(elements) = acls.get("elements").and_then(|v| v.as_array()) {
        for element in elements {
            if let Some(urn) = element.get("organization").and_then(|v| v.as_str()) {
                orgs.push((urn.to_string(), urn.to_string()));
            }
        }
    }
    Ok(orgs)
}

/// Publish a text post as `author_urn` (a person or organization URN). Returns
/// the created post's URN and a best-effort public URL.
pub async fn create_text_post(
    access_token: &str,
    author_urn: &str,
    text: &str,
) -> Result<(String, String)> {
    let body = json!({
        "author": author_urn,
        "commentary": text,
        "visibility": "PUBLIC",
        "distribution": {
            "feedDistribution": "MAIN_FEED",
            "targetEntities": [],
            "thirdPartyDistributionChannels": []
        },
        "lifecycleState": "PUBLISHED",
        "isReshareDisabledByAuthor": false
    });
    let response = reqwest::Client::new()
        .post("https://api.linkedin.com/rest/posts")
        .bearer_auth(access_token)
        .header("LinkedIn-Version", api_version())
        .header("X-Restli-Protocol-Version", "2.0.0")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;
    let status = response.status();
    // The created post's URN comes back in the `x-restli-id` header.
    let post_urn = response
        .headers()
        .get("x-restli-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let text_body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!(
            "linkedin_post_{}: {}",
            status.as_u16(),
            text_body.chars().take(300).collect::<String>()
        );
    }
    let urn = post_urn
        .or_else(|| {
            serde_json::from_str::<serde_json::Value>(&text_body)
                .ok()
                .and_then(|v| v.get("id").and_then(|id| id.as_str()).map(str::to_string))
        })
        .ok_or_else(|| anyhow::anyhow!("linkedin_post_id_missing"))?;
    let url = format!("https://www.linkedin.com/feed/update/{urn}");
    Ok((urn, url))
}
