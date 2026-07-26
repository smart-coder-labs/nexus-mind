use serde::{Deserialize, Serialize};

use super::{
    profiles::{ExecutionProfileVersion, ExtensionRef, CLAUDE_CODE_PROVIDER},
    provenance::ProfileProvenance,
};

#[derive(Clone, Debug, Deserialize)]
pub struct AuthorizationRequest {
    pub provider: String,
    pub requested_profile: String,
    pub organization_allowed_profiles: Vec<String>,
    pub project_allowed_profiles: Vec<String>,
    pub requested_capabilities: Vec<String>,
    pub extensions: Vec<ExtensionRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationStatus {
    Allowed,
    Denied,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthorizationDecision {
    pub status: AuthorizationStatus,
    pub reason: Option<String>,
    pub provenance: Option<ProfileProvenance>,
}

fn denied(reason: &str) -> AuthorizationDecision {
    AuthorizationDecision {
        status: AuthorizationStatus::Denied,
        reason: Some(reason.to_string()),
        provenance: None,
    }
}

/// Resolves only a trusted control-plane policy snapshot. Worker or repository
/// input must never be used to fill either allowlist.
pub fn resolve_execution(
    request: &AuthorizationRequest,
    profiles: &[ExecutionProfileVersion],
) -> AuthorizationDecision {
    if request.provider != CLAUDE_CODE_PROVIDER {
        return denied("unsupported_provider");
    }

    if !request
        .organization_allowed_profiles
        .contains(&request.requested_profile)
        || !request
            .project_allowed_profiles
            .contains(&request.requested_profile)
    {
        return denied("profile_not_allowed");
    }

    let Some(profile) = profiles.iter().find(|profile| {
        profile.profile == request.requested_profile && profile.provider == CLAUDE_CODE_PROVIDER
    }) else {
        return denied("profile_not_found");
    };

    if let Some(reason) = validate_capabilities(&profile.profile, &request.requested_capabilities) {
        return denied(reason);
    }
    if let Some(reason) = validate_extensions(&profile.extensions, &request.extensions) {
        return denied(reason);
    }

    AuthorizationDecision {
        status: AuthorizationStatus::Allowed,
        reason: None,
        provenance: Some(ProfileProvenance::from(profile)),
    }
}

fn validate_capabilities(profile: &str, capabilities: &[String]) -> Option<&'static str> {
    let has = |capability: &str| capabilities.iter().any(|value| value == capability);
    match profile {
        "read-only"
            if [
                "repository_write",
                "pr_publish",
                "deployment_handoff",
                "write_credentials",
            ]
            .iter()
            .any(|capability| has(capability)) =>
        {
            Some("read_only_write_denied")
        }
        "implementation"
            if ["merge", "deployment_handoff", "production_deploy"]
                .iter()
                .any(|capability| has(capability)) =>
        {
            Some("profile_capability_denied")
        }
        "qa-deploy" if has("merge") || has("production_deploy") => {
            Some("profile_capability_denied")
        }
        _ => None,
    }
}

fn validate_extensions(
    expected: &[ExtensionRef],
    received: &[ExtensionRef],
) -> Option<&'static str> {
    for extension in received {
        if !expected.iter().any(|approved| approved == extension) {
            return Some("unapproved_extension");
        }
    }
    for extension in expected.iter().filter(|extension| extension.required) {
        if !received.iter().any(|provided| provided == extension) {
            return Some("required_extension_unavailable");
        }
    }
    None
}
