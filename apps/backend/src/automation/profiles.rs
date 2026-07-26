use serde::{Deserialize, Serialize};

pub const CLAUDE_CODE_PROVIDER: &str = "claude-code";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ExtensionRef {
    pub name: String,
    pub version: String,
    pub hash: String,
    pub required: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ExecutionProfileVersion {
    pub id: String,
    pub profile: String,
    pub version: u32,
    pub provider: String,
    pub model: String,
    pub settings_hash: String,
    pub extensions: Vec<ExtensionRef>,
}

pub fn managed_profiles() -> Vec<ExecutionProfileVersion> {
    vec![
        managed_profile("read-only", 1),
        managed_profile("implementation", 2),
        managed_profile("qa-deploy", 3),
    ]
}

pub fn managed_profile(name: &str, version: u32) -> ExecutionProfileVersion {
    ExecutionProfileVersion {
        id: format!("managed-{name}-v{version}"),
        profile: name.to_string(),
        version,
        provider: CLAUDE_CODE_PROVIDER.to_string(),
        model: "claude-sonnet".to_string(),
        settings_hash: "settings-sha256".to_string(),
        extensions: vec![ExtensionRef {
            name: "approved-mcp".to_string(),
            version: "1.0.0".to_string(),
            hash: "extension-sha256".to_string(),
            required: true,
        }],
    }
}
