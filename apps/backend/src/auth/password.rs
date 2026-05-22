use anyhow::Result;
use bcrypt::{hash, verify, DEFAULT_COST};

pub fn hash_password(plain: &str) -> Result<String> {
    Ok(hash(plain, DEFAULT_COST)?)
}

pub fn verify_password(plain: &str, hashed: &str) -> Result<bool> {
    Ok(verify(plain, hashed)?)
}
