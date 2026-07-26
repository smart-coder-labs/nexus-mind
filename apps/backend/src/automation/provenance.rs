use serde::Serialize;

use super::profiles::ExecutionProfileVersion;

/// Immutable profile identity included in every later run receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProfileProvenance {
    pub profile_version_id: String,
    pub profile: String,
    pub version: u32,
    pub provider: String,
    pub model: String,
    pub settings_hash: String,
    pub extension_hashes: Vec<String>,
}

impl From<&ExecutionProfileVersion> for ProfileProvenance {
    fn from(profile: &ExecutionProfileVersion) -> Self {
        Self {
            profile_version_id: profile.id.clone(),
            profile: profile.profile.clone(),
            version: profile.version,
            provider: profile.provider.clone(),
            model: profile.model.clone(),
            settings_hash: profile.settings_hash.clone(),
            extension_hashes: profile
                .extensions
                .iter()
                .map(|extension| extension.hash.clone())
                .collect(),
        }
    }
}
