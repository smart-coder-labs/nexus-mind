//! Password hashing and verification backed by bcrypt.

use anyhow::Result;
use bcrypt::{hash, verify, DEFAULT_COST};

/// Hashes `plain` with bcrypt at the library's default cost factor.
///
/// The returned string is a self-contained bcrypt hash that encodes
/// the algorithm, cost, salt, and digest — suitable for storage in the DB.
pub fn hash_password(plain: &str) -> Result<String> {
    Ok(hash(plain, DEFAULT_COST)?)
}

/// Returns `true` if `plain` matches the stored bcrypt `hashed` string.
///
/// Errors only on malformed `hashed` input; a wrong password returns `Ok(false)`.
pub fn verify_password(plain: &str, hashed: &str) -> Result<bool> {
    Ok(verify(plain, hashed)?)
}
