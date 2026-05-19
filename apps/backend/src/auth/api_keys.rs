use sha2::{Digest, Sha256};

/// Generates a new API key. Returns `(raw_key, sha256_hash)`.
/// The raw key is shown once to the user; only the hash is stored.
pub fn generate() -> (String, String) {
    let random_bytes: [u8; 32] = rand::random();
    let raw = format!("nm_{}", hex::encode(random_bytes));
    let hash = hash_key(&raw);
    (raw, hash)
}

/// Deterministically hashes a raw key. Used for storage and lookup.
pub fn hash_key(key: &str) -> String {
    hex::encode(Sha256::digest(key.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_key_has_nm_prefix() {
        let (raw, _) = generate();
        assert!(raw.starts_with("nm_"), "key must start with 'nm_', got: {raw}");
    }

    #[test]
    fn generated_key_hash_matches() {
        let (raw, hash) = generate();
        assert_eq!(hash_key(&raw), hash, "hash must be deterministic");
    }

    #[test]
    fn two_generated_keys_are_unique() {
        let (raw1, _) = generate();
        let (raw2, _) = generate();
        assert_ne!(raw1, raw2, "keys must be unique");
    }

    #[test]
    fn hash_is_deterministic() {
        let key = "nm_abc123";
        assert_eq!(hash_key(key), hash_key(key));
    }

    #[test]
    fn different_inputs_produce_different_hashes() {
        assert_ne!(hash_key("nm_aaa"), hash_key("nm_bbb"));
    }

    #[test]
    fn hash_is_hex_string_of_64_chars() {
        let (_, hash) = generate();
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "hash must be hex, got: {hash}"
        );
        assert_eq!(hash.len(), 64, "sha256 hex is 64 chars");
    }
}
