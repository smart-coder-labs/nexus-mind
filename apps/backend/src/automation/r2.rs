//! Minimal Cloudflare R2 (S3-compatible) client for uploading QA screenshot
//! evidence. Implements AWS SigV4 PutObject and presigned GET using the crypto
//! primitives already vendored (sha2/hmac/hex) — no heavyweight AWS SDK.
//!
//! Keys are restricted to a safe character set so the canonical URI needs no
//! percent-encoding (the signed path and the request path must match exactly).

use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct R2Config {
    pub account_id: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    /// Optional permanent public base (r2.dev or a custom domain). When unset we
    /// fall back to time-limited presigned GET URLs.
    pub public_base_url: Option<String>,
}

impl R2Config {
    pub fn from_env() -> Option<Self> {
        let get = |key: &str| std::env::var(key).ok().filter(|value| !value.is_empty());
        let account_id = get("R2_ACCOUNT_ID")?;
        let bucket = get("R2_BUCKET")?;
        let access_key_id = get("R2_ACCESS_KEY_ID")?;
        let secret_access_key = get("R2_SECRET_ACCESS_KEY")?;
        Some(Self {
            account_id,
            bucket,
            access_key_id,
            secret_access_key,
            public_base_url: get("R2_PUBLIC_BASE_URL"),
        })
    }

    fn host(&self) -> String {
        format!("{}.r2.cloudflarestorage.com", self.account_id)
    }
}

/// Restrict a key to a set that needs no URI-encoding in the SigV4 canonical URI.
pub fn safe_key(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn hex_sha256(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let key = hmac(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let key = hmac(&key, region.as_bytes());
    let key = hmac(&key, service.as_bytes());
    hmac(&key, b"aws4_request")
}

const REGION: &str = "auto";
const SERVICE: &str = "s3";

/// Upload bytes to R2 (SigV4 PutObject). `key` is sanitized to a URI-safe form.
pub async fn put_object(
    cfg: &R2Config,
    key: &str,
    body: &[u8],
    content_type: &str,
) -> anyhow::Result<String> {
    let key = safe_key(key);
    let now = Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date = now.format("%Y%m%d").to_string();
    let host = cfg.host();
    let payload_hash = hex_sha256(body);
    let canonical_uri = format!("/{}/{}", cfg.bucket, key);
    let canonical_headers = format!(
        "content-type:{content_type}\nhost:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
    );
    let signed_headers = "content-type;host;x-amz-content-sha256;x-amz-date";
    let canonical_request = format!(
        "PUT\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );
    let scope = format!("{date}/{REGION}/{SERVICE}/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        hex_sha256(canonical_request.as_bytes())
    );
    let signature = hex::encode(hmac(
        &signing_key(&cfg.secret_access_key, &date, REGION, SERVICE),
        string_to_sign.as_bytes(),
    ));
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        cfg.access_key_id
    );
    let url = format!("https://{host}{canonical_uri}");
    let response = reqwest::Client::new()
        .put(&url)
        .header("Content-Type", content_type)
        .header("x-amz-content-sha256", &payload_hash)
        .header("x-amz-date", &amz_date)
        .header("Authorization", authorization)
        .body(body.to_vec())
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "r2_put_failed status={status} body={}",
            text.chars().take(300).collect::<String>()
        );
    }
    Ok(key)
}

/// A viewable URL for a key: the permanent public base when configured, else a
/// time-limited SigV4 presigned GET URL.
pub fn object_url(cfg: &R2Config, key: &str, expiry_secs: u64) -> String {
    let key = safe_key(key);
    if let Some(base) = &cfg.public_base_url {
        return format!("{}/{}", base.trim_end_matches('/'), key);
    }
    let now = Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date = now.format("%Y%m%d").to_string();
    let host = cfg.host();
    let canonical_uri = format!("/{}/{}", cfg.bucket, key);
    let scope = format!("{date}/{REGION}/{SERVICE}/aws4_request");
    let credential = uri_encode(&format!("{}/{scope}", cfg.access_key_id));
    // Query params must be sorted by key for the canonical query string.
    let canonical_query = format!(
        "X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential={credential}&X-Amz-Date={amz_date}&X-Amz-Expires={expiry_secs}&X-Amz-SignedHeaders=host"
    );
    let canonical_headers = format!("host:{host}\n");
    let canonical_request = format!(
        "GET\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\nhost\nUNSIGNED-PAYLOAD"
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        hex_sha256(canonical_request.as_bytes())
    );
    let signature = hex::encode(hmac(
        &signing_key(&cfg.secret_access_key, &date, REGION, SERVICE),
        string_to_sign.as_bytes(),
    ));
    format!("https://{host}{canonical_uri}?{canonical_query}&X-Amz-Signature={signature}")
}

/// Percent-encode for query values per AWS (encode '/' too).
fn uri_encode(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Live round-trip against real R2. Ignored by default; run with:
    //   R2_ACCOUNT_ID=.. R2_BUCKET=.. R2_ACCESS_KEY_ID=.. R2_SECRET_ACCESS_KEY=.. \
    //   cargo test --lib automation::r2::tests::live_round_trip -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_round_trip() {
        let cfg = R2Config::from_env().expect("R2_* env vars set");
        let key = "qa-evidence/_selftest/hello.txt";
        let body = b"nexusmind r2 selftest";
        put_object(&cfg, key, body, "text/plain")
            .await
            .expect("put_object");
        let url = object_url(&cfg, key, 600);
        eprintln!("presigned url: {url}");
        let fetched = reqwest::get(&url).await.expect("get").bytes().await.expect("bytes");
        assert_eq!(&fetched[..], body);
        eprintln!("round-trip OK");
    }
}
