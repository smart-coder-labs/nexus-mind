mod discovery;
mod routing;

pub use discovery::{load, ConfigSelection};
pub use routing::{
    DestinationOverride, PlannedGroup, ProjectResolver, ResolutionBasis, ResolutionStatus,
    ResolvedProject, RoutingPlan, Specificity,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;

pub const KNOWN_CAPABILITIES: &[&str] = &[
    "context.read",
    "memory.read",
    "memory.write",
    "convention.read",
    "convention.write",
    "project.read",
    "client.read",
    "task.read",
    "task.write",
    "sdd.read",
    "sdd.write",
    "code.read",
    "usage.read",
    "usage.write",
    "migration.run",
    "migration.review",
    "harness.read",
    "harness.write",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub code: &'static str,
    pub message: String,
}

impl ConfigError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, Clone)]
pub struct ConfigSnapshot {
    pub config: RepositoryConfigV1,
    pub root: PathBuf,
    pub relative_path: String,
    pub sha256: String,
    pub(crate) bytes: Vec<u8>,
}

impl ConfigSnapshot {
    pub fn from_bytes(
        bytes: &[u8],
        root: PathBuf,
        relative_path: String,
    ) -> Result<Self, ConfigError> {
        let config = parse(bytes)?;
        Ok(Self {
            config,
            root,
            relative_path,
            sha256: format!("sha256:{}", hex::encode(Sha256::digest(bytes))),
            bytes: bytes.to_vec(),
        })
    }

    pub fn bytes_unchanged(&self, bytes: &[u8]) -> bool {
        self.bytes == bytes
    }

    pub fn verify_current(&self) -> Result<(), ConfigError> {
        let path = self.root.join(&self.relative_path);
        let bytes = std::fs::read(path).map_err(|_| {
            ConfigError::new(
                "CONFIG_CHANGED",
                "repository config disappeared before publication",
            )
        })?;
        if !self.bytes_unchanged(&bytes) {
            return Err(ConfigError::new(
                "CONFIG_CHANGED",
                "repository config changed before publication",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryConfigV1 {
    pub version: u32,
    pub repository: RepositoryIdentity,
    #[serde(default)]
    pub defaults: Defaults,
    pub projects: BTreeMap<String, ProjectConfig>,
    #[serde(default)]
    pub agents: AgentsConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryIdentity {
    pub id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    pub project: Option<String>,
    pub agent_profile: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub project_id: String,
    pub client_id: Option<String>,
    pub paths: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    pub agent_profile: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentsConfig {
    #[serde(default)]
    pub profiles: BTreeMap<String, AgentProfile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentProfile {
    pub extends: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub disable_capabilities: Vec<String>,
}

impl RepositoryConfigV1 {
    pub fn effective_capabilities(&self, profile: &str) -> Result<BTreeSet<String>, ConfigError> {
        fn visit(
            cfg: &RepositoryConfigV1,
            name: &str,
            stack: &mut Vec<String>,
        ) -> Result<BTreeSet<String>, ConfigError> {
            if let Some(pos) = stack.iter().position(|n| n == name) {
                let mut cycle = stack[pos..].to_vec();
                cycle.push(name.to_string());
                return Err(ConfigError::new("CONFIG_PROFILE_CYCLE", cycle.join(" -> ")));
            }
            let profile = cfg.agents.profiles.get(name).ok_or_else(|| {
                ConfigError::new(
                    "CONFIG_INVALID_REFERENCE",
                    format!("unknown profile `{name}`"),
                )
            })?;
            stack.push(name.to_string());
            let mut out = match profile.extends.as_deref() {
                Some(parent) => visit(cfg, parent, stack)?,
                None => BTreeSet::new(),
            };
            stack.pop();
            out.extend(profile.capabilities.iter().cloned());
            for denied in &profile.disable_capabilities {
                out.remove(denied);
            }
            Ok(out)
        }
        visit(self, profile, &mut Vec::new())
    }
}

pub fn parse(bytes: &[u8]) -> Result<RepositoryConfigV1, ConfigError> {
    reject_unsafe_yaml(bytes)?;
    let config: RepositoryConfigV1 = serde_yaml::from_slice(bytes)
        .map_err(|e| ConfigError::new("CONFIG_INVALID_SCHEMA", e.to_string()))?;
    validate(&config)?;
    Ok(config)
}

fn reject_unsafe_yaml(bytes: &[u8]) -> Result<(), ConfigError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ConfigError::new("CONFIG_INVALID_SCHEMA", "config must be UTF-8"))?;
    for line in text.lines() {
        let code = line.split('#').next().unwrap_or_default();
        let trimmed = code.trim_start();
        if trimmed.starts_with("<<:")
            || code
                .split_whitespace()
                .any(|token| token.starts_with('&') || (token.starts_with('*') && token != "**"))
        {
            return Err(ConfigError::new(
                "CONFIG_YAML_ALIAS",
                "YAML aliases and anchors are not supported",
            ));
        }
        if let Some((key, _)) = trimmed.split_once(':') {
            let normalized = key.trim().to_ascii_lowercase().replace('-', "_");
            if [
                "api_key",
                "token",
                "access_token",
                "dsn",
                "password",
                "private_key",
                "command",
                "exec",
            ]
            .contains(&normalized.as_str())
            {
                return Err(ConfigError::new(
                    "CONFIG_SECRET_FIELD",
                    "secrets and executable content do not belong in repository config",
                ));
            }
        }
    }
    Ok(())
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .enumerate()
            .all(|(i, c)| c.is_ascii_lowercase() || c.is_ascii_digit() || (c == b'-' && i > 0))
        && !value.ends_with('-')
}

fn validate(config: &RepositoryConfigV1) -> Result<(), ConfigError> {
    if config.version != 1 {
        return Err(ConfigError::new(
            "CONFIG_UNSUPPORTED_VERSION",
            format!("version {} is not supported", config.version),
        ));
    }
    if !valid_slug(&config.repository.id) {
        return Err(ConfigError::new(
            "CONFIG_INVALID_SCHEMA",
            "repository.id must be a lowercase slug",
        ));
    }
    if config.projects.is_empty() {
        return Err(ConfigError::new(
            "CONFIG_INVALID_SCHEMA",
            "projects must not be empty",
        ));
    }
    let known: BTreeSet<&str> = KNOWN_CAPABILITIES.iter().copied().collect();
    let mut ids = BTreeSet::new();
    for (alias, project) in &config.projects {
        if !valid_slug(alias)
            || project.project_id.trim().is_empty()
            || project.project_id.len() > 255
            || project.paths.is_empty()
        {
            return Err(ConfigError::new(
                "CONFIG_INVALID_SCHEMA",
                format!("invalid project `{alias}`"),
            ));
        }
        if !ids.insert(project.project_id.as_str()) {
            return Err(ConfigError::new(
                "CONFIG_INVALID_SCHEMA",
                "project_id values must be unique",
            ));
        }
        if let Some(profile) = project.agent_profile.as_deref() {
            if !config.agents.profiles.contains_key(profile) {
                return Err(ConfigError::new(
                    "CONFIG_INVALID_REFERENCE",
                    format!("project `{alias}` references unknown profile `{profile}`"),
                ));
            }
        }
    }
    if let Some(default) = config.defaults.project.as_deref() {
        if !config.projects.contains_key(default) {
            return Err(ConfigError::new(
                "CONFIG_INVALID_REFERENCE",
                format!("unknown default project `{default}`"),
            ));
        }
    }
    if let Some(profile) = config.defaults.agent_profile.as_deref() {
        if !config.agents.profiles.contains_key(profile) {
            return Err(ConfigError::new(
                "CONFIG_INVALID_REFERENCE",
                format!("unknown default profile `{profile}`"),
            ));
        }
    }
    for (name, profile) in &config.agents.profiles {
        if !valid_slug(name) {
            return Err(ConfigError::new(
                "CONFIG_INVALID_SCHEMA",
                format!("invalid profile `{name}`"),
            ));
        }
        let mut seen = BTreeSet::new();
        for cap in profile
            .capabilities
            .iter()
            .chain(&profile.disable_capabilities)
        {
            if !known.contains(cap.as_str()) {
                return Err(ConfigError::new(
                    "CONFIG_UNKNOWN_CAPABILITY",
                    format!("unknown capability `{cap}`"),
                ));
            }
            if !seen.insert(cap) {
                return Err(ConfigError::new(
                    "CONFIG_INVALID_SCHEMA",
                    format!("profile `{name}` repeats capability `{cap}`"),
                ));
            }
        }
        if let Some(parent) = profile.extends.as_deref() {
            if !config.agents.profiles.contains_key(parent) {
                return Err(ConfigError::new(
                    "CONFIG_INVALID_REFERENCE",
                    format!("profile `{name}` extends unknown profile `{parent}`"),
                ));
            }
        }
        config.effective_capabilities(name)?;
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct SnapshotAttestation<'a> {
    pub schema_version: u32,
    pub repository_id: &'a str,
    pub path: &'a str,
    pub sha256: &'a str,
}

impl ConfigSnapshot {
    pub fn attestation(&self) -> SnapshotAttestation<'_> {
        SnapshotAttestation {
            schema_version: 1,
            repository_id: &self.config.repository.id,
            path: &self.relative_path,
            sha256: &self.sha256,
        }
    }
}

#[cfg(test)]
mod tests;
