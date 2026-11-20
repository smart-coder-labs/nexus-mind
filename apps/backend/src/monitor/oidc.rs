//! Verifies the OIDC token Pub/Sub attaches to each push request, so only the
//! service account an org registered can post to its monitor source.

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use serde_json::Value;

const GOOGLE_CERTS: &str = "https://www.googleapis.com/oauth2/v3/certs";
const ISSUERS: [&str; 2] = ["accounts.google.com", "https://accounts.google.com"];
const CACHE_SECONDS: u64 = 3600;
/// An unknown `kid` refetches the keys at most this often (Google rotates them).
const REFETCH_SECONDS: u64 = 60;

#[derive(Deserialize)]
struct Claims {
    email: Option<String>,
    email_verified: Option<bool>,
}

/// The audience a source's push subscription must use.
pub fn audience(source_id: &str) -> String {
    format!("nexusmind-monitor:{source_id}")
}

/// Checks signature, issuer, audience, expiry and the signing service account
/// against a JWKS document.
pub fn verify_with_keys(token: &str, jwks: &Value, audience: &str, service_account: &str) -> Result<(), &'static str> {
    let header = decode_header(token).map_err(|_| "token_malformed")?;
    if header.alg != Algorithm::RS256 {
        return Err("token_algorithm");
    }
    let kid = header.kid.ok_or("token_kid_missing")?;
    let key = jwks
        .get("keys")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|key| key.get("kid").and_then(Value::as_str) == Some(kid.as_str()))
        .ok_or("token_kid_unknown")?;
    let (Some(n), Some(e)) = (key.get("n").and_then(Value::as_str), key.get("e").and_then(Value::as_str)) else {
        return Err("token_key_invalid");
    };
    let key = DecodingKey::from_rsa_components(n, e).map_err(|_| "token_key_invalid")?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[audience]);
    validation.set_issuer(&ISSUERS);
    let claims = decode::<Claims>(token, &key, &validation).map_err(|_| "token_invalid")?.claims;
    let email_ok = claims.email.is_some_and(|email| email.eq_ignore_ascii_case(service_account));
    if !email_ok || claims.email_verified != Some(true) {
        return Err("token_wrong_service_account");
    }
    Ok(())
}

struct Cache {
    keys: Value,
    fetched: std::time::Instant,
}

static CACHE: tokio::sync::Mutex<Option<Cache>> = tokio::sync::Mutex::const_new(None);

async fn fetch_keys() -> anyhow::Result<Value> {
    Ok(reqwest::Client::new()
        .get(GOOGLE_CERTS)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

/// Verifies a push token against Google's current keys (cached for an hour).
pub async fn verify(token: &str, audience: &str, service_account: &str) -> Result<(), &'static str> {
    let mut cache = CACHE.lock().await;
    let age = cache.as_ref().map(|c| c.fetched.elapsed().as_secs());
    if age.is_none_or(|age| age >= CACHE_SECONDS) {
        let keys = fetch_keys().await.map_err(|_| "google_keys_unavailable")?;
        *cache = Some(Cache { keys, fetched: std::time::Instant::now() });
    }
    let result = verify_with_keys(token, &cache.as_ref().expect("filled above").keys, audience, service_account);
    if result == Err("token_kid_unknown") && age.is_some_and(|age| age >= REFETCH_SECONDS) {
        let keys = fetch_keys().await.map_err(|_| "google_keys_unavailable")?;
        *cache = Some(Cache { keys, fetched: std::time::Instant::now() });
        return verify_with_keys(token, &cache.as_ref().expect("filled above").keys, audience, service_account);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::json;

    // Throwaway key made for these tests only.
    const PRIVATE_KEY: &str = include_str!("test_key.pem");
    const N: &str = "4YNgIPJrOHJYF2a2KUNgoYNoB9p6yyUuLiQ6-bZ4cIq3q1szEJmXV0D0SL4MRdxP7Tfp6cUh66y0edt4JOrTVYTVUfeVCK_vNN9U61rptIvyjIQizV6vie1w3I_onyhfeRCwTaBvKVnAnwA8FlwI6sCkQjz8_QSZHRtjSdVOghwkpAbYvGrbtqeLHIs5d7pdKqX8cDfSQ9PocOLKhRChxZnq08bjNOGFcMgBcmGfU4_gl-njHShpUGQk3u4le9gu4D2UP9702_LXD2U_f3ijq7X8nn5Q5mdRPrVBoVyuAG_ZAUBWhMYZgnMTml8Kf5KSVFuMb5qK3hJe_wsJf4Nl7Q";
    const SA: &str = "push@my-proj.iam.gserviceaccount.com";

    fn jwks() -> Value {
        json!({"keys": [{"kid": "k1", "kty": "RSA", "alg": "RS256", "n": N, "e": "AQAB"}]})
    }

    fn token(claims: Value, kid: &str) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.into());
        encode(&header, &claims, &EncodingKey::from_rsa_pem(PRIVATE_KEY.as_bytes()).unwrap()).unwrap()
    }

    fn claims() -> Value {
        json!({"iss": "https://accounts.google.com", "aud": audience("src1"), "email": SA,
               "email_verified": true, "exp": chrono::Utc::now().timestamp() + 600})
    }

    #[test]
    fn a_valid_push_token_passes() {
        assert_eq!(verify_with_keys(&token(claims(), "k1"), &jwks(), &audience("src1"), SA), Ok(()));
    }

    #[test]
    fn wrong_audience_account_expiry_or_key_fail() {
        let aud = audience("src1");
        assert!(verify_with_keys(&token(claims(), "k1"), &jwks(), &audience("other"), SA).is_err());
        assert_eq!(
            verify_with_keys(&token(claims(), "k1"), &jwks(), &aud, "attacker@x.iam.gserviceaccount.com"),
            Err("token_wrong_service_account")
        );
        let mut unverified = claims();
        unverified["email_verified"] = json!(false);
        assert!(verify_with_keys(&token(unverified, "k1"), &jwks(), &aud, SA).is_err());
        let mut expired = claims();
        expired["exp"] = json!(chrono::Utc::now().timestamp() - 3600);
        assert!(verify_with_keys(&token(expired, "k1"), &jwks(), &aud, SA).is_err());
        let mut foreign = claims();
        foreign["iss"] = json!("https://evil.example");
        assert!(verify_with_keys(&token(foreign, "k1"), &jwks(), &aud, SA).is_err());
        assert_eq!(verify_with_keys(&token(claims(), "k2"), &jwks(), &aud, SA), Err("token_kid_unknown"));
        assert_eq!(verify_with_keys("not.a.token", &jwks(), &aud, SA), Err("token_malformed"));
    }

    #[test]
    fn a_tampered_token_fails() {
        let good = token(claims(), "k1");
        let mut parts: Vec<&str> = good.split('.').collect();
        let mut forged = claims();
        forged["email"] = json!("other@x.iam.gserviceaccount.com");
        let body = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, forged.to_string());
        parts[1] = &body;
        assert_eq!(verify_with_keys(&parts.join("."), &jwks(), &audience("src1"), SA), Err("token_invalid"));
    }
}
