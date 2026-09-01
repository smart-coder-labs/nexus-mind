//! Token cipher (AES-256-GCM).
//!
//! `NEXUSMIND_TOKEN_ENCRYPTION_KEY` must be a 64-char hex string (32 bytes).
//! When it is unset or malformed, [`encrypt`] returns `None` and the caller
//! decides what that means — the code-index path treats it as "do not persist
//! the token", while migration `v58` treats it as a hard failure, because
//! copying a plaintext credential forward would defeat the migration.
//!
//! This lived inside `api::code` until the client model needed it from
//! `db::migrations` and `db::queries` as well. Credentials are not an HTTP
//! concern, so it now sits on its own.

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm,
};

const KEY_ENV: &str = "NEXUSMIND_TOKEN_ENCRYPTION_KEY";

fn cipher() -> Option<Aes256Gcm> {
    let key_hex = std::env::var(KEY_ENV).ok()?;
    let key_bytes = hex::decode(key_hex.trim()).ok()?;
    if key_bytes.len() != 32 {
        tracing::warn!("{KEY_ENV} must be 64 hex chars (32 bytes); token will not be persisted");
        return None;
    }
    Aes256Gcm::new_from_slice(&key_bytes).ok()
}

/// True when a usable encryption key is configured.
///
/// Callers that must refuse to proceed without encryption (migrations moving
/// stored credentials) check this instead of inferring it from an `encrypt`
/// that returned `None`, which cannot distinguish "no key" from "cipher error".
pub fn is_configured() -> bool {
    cipher().is_some()
}

/// Encrypt `plaintext` with AES-256-GCM. Returns hex(nonce || ciphertext).
/// Returns None if `NEXUSMIND_TOKEN_ENCRYPTION_KEY` is not configured or invalid.
pub fn encrypt(plaintext: &str) -> Option<String> {
    let c = cipher()?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ct = c.encrypt(&nonce, plaintext.as_bytes()).ok()?;
    let mut blob = nonce.to_vec();
    blob.extend_from_slice(&ct);
    Some(hex::encode(blob))
}

/// Decrypt a blob produced by [`encrypt`]. Returns None on any failure.
pub fn decrypt(blob: &str) -> Option<String> {
    let c = cipher()?;
    let bytes = hex::decode(blob).ok()?;
    if bytes.len() < 12 {
        return None;
    }
    let (nonce_bytes, ct) = bytes.split_at(12);
    let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
    let plain = c.decrypt(nonce, ct).ok()?;
    String::from_utf8(plain).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 32 bytes of hex, deterministic so the test is reproducible.
    const TEST_KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    fn with_key<T>(f: impl FnOnce() -> T) -> T {
        std::env::set_var(KEY_ENV, TEST_KEY);
        let out = f();
        std::env::remove_var(KEY_ENV);
        out
    }

    #[test]
    fn roundtrip_recovers_plaintext() {
        with_key(|| {
            let blob = encrypt("ghp_secret_token").expect("encryption must succeed with a key");
            assert_ne!(blob, "ghp_secret_token", "stored value must not be plaintext");
            assert_eq!(decrypt(&blob).as_deref(), Some("ghp_secret_token"));
        });
    }

    /// Two encryptions of the same plaintext must differ — a fresh nonce each
    /// time. Equal ciphertexts would leak which orgs share a token.
    #[test]
    fn same_plaintext_encrypts_differently() {
        with_key(|| {
            let a = encrypt("same").unwrap();
            let b = encrypt("same").unwrap();
            assert_ne!(a, b);
        });
    }

    #[test]
    fn decrypt_rejects_tampered_blob() {
        with_key(|| {
            let mut blob = encrypt("secret").unwrap();
            // Flip the last character to a GUARANTEED-different one. Popping and
            // pushing a fixed char was flaky: when the last char already equaled it
            // the blob was unchanged and decryption (correctly) succeeded.
            let last = blob.pop().unwrap();
            blob.push(if last == '0' { '1' } else { '0' });
            assert!(decrypt(&blob).is_none(), "GCM must reject a tampered blob");
        });
    }
}
