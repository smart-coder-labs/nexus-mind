//! Private local secret store (`~/.nexusmind/secrets.env` with `0600` permissions).
//!
//! Solves macOS `launchd` environments that do not inherit interactive shell exports
//! while keeping credentials out of `~/.nexusmind/mascot.yaml` and logs.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct SecretsStore {
    path: PathBuf,
    values: HashMap<String, String>,
}

impl SecretsStore {
    pub fn load(path: PathBuf) -> Self {
        let mut values = HashMap::new();
        if let Ok(raw) = std::fs::read_to_string(&path) {
            for line in raw.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = trimmed.split_once('=') {
                    let key = k.trim().to_string();
                    let val = v.trim().trim_matches('"').trim_matches('\'').to_string();
                    if !key.is_empty() && !val.is_empty() {
                        values.insert(key, val);
                    }
                }
            }
        }
        Self { path, values }
    }

    /// Reads from local secrets store first, then falls back to process environment variable.
    pub fn get(&self, env_name: &str) -> Option<String> {
        if let Some(v) = self.values.get(env_name).filter(|s| !s.trim().is_empty()) {
            return Some(v.clone());
        }
        std::env::var(env_name)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    pub fn has(&self, env_name: &str) -> bool {
        self.get(env_name).is_some()
    }

    pub fn set_and_persist(&mut self, env_name: &str, value: &str) -> Result<()> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        self.values.insert(env_name.to_string(), trimmed.to_string());
        self.save_to_disk()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn save_to_disk(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut keys: Vec<&String> = self.values.keys().collect();
        keys.sort();
        let mut content = String::from("# NexusMind Mascot private credentials (mode 0600)\n");
        for k in keys {
            if let Some(v) = self.values.get(k) {
                content.push_str(&format!("{k}={v}\n"));
            }
        }
        std::fs::write(&self.path, content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}
