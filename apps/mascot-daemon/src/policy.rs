//! Deterministic Safety Floors + System 1 (Laya/Jev) Autonomy Gate.
//!
//! Deterministic floors ALWAYS execute first and can never be overridden by
//! Laya, Jev, or any System 2 LLM.

use crate::config::{AutonomyConfig, AutonomyLevel, PrivacyConfig};
use serde::{Deserialize, Serialize};
use systemone_client::decisions::{ActionRiskAnswer, ActionRiskInput};
use systemone_client::{DecisionEnvelope, DecisionSource, SystemOneRouter};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum PolicyVerdict {
    Allow {
        reason: String,
        risk: f64,
        source: DecisionSource,
    },
    HoldForHuman {
        reason: String,
        risk: f64,
        source: DecisionSource,
    },
    Block {
        reason: String,
    },
}

#[derive(Clone, Debug)]
pub struct ProposedAction {
    pub action_kind: String,
    pub summary: String,
    pub target_app_or_path: Option<String>,
    pub command_preview: Option<String>,
}

/// Checks inviolable deterministic safety rules before consulting any model.
pub fn evaluate_deterministic_floor(
    action: &ProposedAction,
    privacy: &PrivacyConfig,
    kill_switch_active: bool,
) -> Option<PolicyVerdict> {
    if kill_switch_active {
        return Some(PolicyVerdict::Block {
            reason: "Emergency Kill Switch is active; all actions are paused.".into(),
        });
    }

    // 1. Blocked apps (password managers, Keychain, etc.)
    if let Some(target) = action.target_app_or_path.as_deref() {
        let lower_target = target.to_ascii_lowercase();
        for app in &privacy.blocked_apps {
            if lower_target.contains(&app.to_ascii_lowercase()) {
                return Some(PolicyVerdict::Block {
                    reason: format!("Target '{app}' is protected by the Privacy Blocklist."),
                });
            }
        }
        for path in &privacy.blocked_paths {
            if lower_target.contains(&path.to_ascii_lowercase()) {
                return Some(PolicyVerdict::HoldForHuman {
                    reason: format!("Protected path pattern '{path}' requires explicit human approval."),
                    risk: 1.0,
                    source: DecisionSource::DeterministicFallback,
                });
            }
        }
    }

    // 2. Hard-blocked destructive shell patterns
    let combined = format!(
        "{} {} {}",
        action.summary,
        action.target_app_or_path.as_deref().unwrap_or(""),
        action.command_preview.as_deref().unwrap_or("")
    )
    .to_ascii_lowercase();

    let hard_blocks = [
        ("rm -rf /", "Recursive root deletion is forbidden."),
        ("rm -rf ~", "Recursive home directory deletion is forbidden."),
        ("mkfs", "Filesystem formatting commands are forbidden."),
        ("dd if=", "Raw disk overwrite (`dd`) is forbidden."),
        (":(){ :|:& };:", "Fork bomb detected and blocked."),
        ("/system/library", "Modifying macOS /System/Library is forbidden."),
        ("security dump-keychain", "Dumping macOS Keychain is forbidden."),
    ];
    for (needle, msg) in hard_blocks {
        if combined.contains(needle) {
            return Some(PolicyVerdict::Block {
                reason: msg.into(),
            });
        }
    }

    // 3. Always-hold triggers (privilege escalation, permanent delete, money transfer)
    let always_hold = [
        ("sudo ", "Privilege escalation (`sudo`) always requires your confirmation."),
        ("rm -rf", "Permanent recursive deletion requires explicit approval (prefer macOS Trash)."),
        ("git push --force", "Force-pushing Git history requires your confirmation."),
        ("wire transfer", "Financial transfers always require explicit approval."),
        ("checkout", "Payment/checkout actions require explicit approval."),
    ];
    for (needle, msg) in always_hold {
        if combined.contains(needle) {
            return Some(PolicyVerdict::HoldForHuman {
                reason: msg.into(),
                risk: 0.95,
                source: DecisionSource::DeterministicFallback,
            });
        }
    }

    None
}

/// Evaluates an action using Deterministic Floors first, then Autonomy Level + System 1 (Laya/Jev).
/// Fails CLOSED (HoldForHuman) if System 1 is unreachable or violates schema.
pub async fn evaluate_action(
    action: &ProposedAction,
    autonomy: &AutonomyConfig,
    privacy: &PrivacyConfig,
    router: &SystemOneRouter,
    kill_switch_active: bool,
    is_private_context: bool,
) -> PolicyVerdict {
    if let Some(floor) = evaluate_deterministic_floor(action, privacy, kill_switch_active) {
        return floor;
    }

    if autonomy.level == AutonomyLevel::Observer {
        return PolicyVerdict::Block {
            reason: "Autonomy level is set to Observer (read-only).".into(),
        };
    }

    if autonomy.level == AutonomyLevel::Copilot {
        return PolicyVerdict::HoldForHuman {
            reason: "Copilot mode requires confirmation before acting.".into(),
            risk: 0.5,
            source: DecisionSource::DeterministicFallback,
        };
    }

    let input = ActionRiskInput {
        action_kind: action.action_kind.clone(),
        summary: action.summary.clone(),
        target_app_or_path: action.target_app_or_path.clone(),
        command_preview: action.command_preview.clone(),
        is_private_context,
    };

    match router.evaluate_action_risk(&input).await {
        Ok(env) => apply_autonomy_thresholds(autonomy, env),
        Err(err) => PolicyVerdict::HoldForHuman {
            reason: format!(
                "Decision engine unavailable ({err}); holding action for human review (fail-closed)."
            ),
            risk: 0.75,
            source: DecisionSource::DeterministicFallback,
        },
    }
}

fn apply_autonomy_thresholds(
    autonomy: &AutonomyConfig,
    env: DecisionEnvelope<ActionRiskAnswer>,
) -> PolicyVerdict {
    let ans = env.answer;
    let max_allowed_risk = match autonomy.level {
        AutonomyLevel::SupervisedAutonomous => autonomy.auto_approve_max_risk,
        AutonomyLevel::FullAutonomous => 0.65_f64.max(autonomy.auto_approve_max_risk),
        AutonomyLevel::Observer | AutonomyLevel::Copilot => 0.0,
    };

    let irreversible = ans.reversibility == "irreversible";
    let destructive = ans.action_class == "destructive_or_financial";

    if destructive {
        return PolicyVerdict::HoldForHuman {
            reason: format!(
                "Classified as destructive/financial (risk {:.2}, model {}).",
                ans.risk, env.model
            ),
            risk: ans.risk,
            source: env.source,
        };
    }

    if irreversible && autonomy.level != AutonomyLevel::FullAutonomous {
        return PolicyVerdict::HoldForHuman {
            reason: format!(
                "Irreversible {} action held for approval (risk {:.2}, model {}).",
                ans.action_class, ans.risk, env.model
            ),
            risk: ans.risk,
            source: env.source,
        };
    }

    if ans.risk <= max_allowed_risk && ans.risk_confidence >= autonomy.min_confidence {
        PolicyVerdict::Allow {
            reason: format!(
                "Auto-approved by {} ({}, risk {:.2}, confidence {:.2}, {} ms).",
                env.model, ans.action_class, ans.risk, ans.risk_confidence, env.latency_ms
            ),
            risk: ans.risk,
            source: env.source,
        }
    } else {
        PolicyVerdict::HoldForHuman {
            reason: format!(
                "Held for confirmation: {} (risk {:.2}, confidence {:.2}, model {}).",
                ans.action_class, ans.risk, ans.risk_confidence, env.model
            ),
            risk: ans.risk,
            source: env.source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_password_manager_and_destructive_rm() {
        let privacy = PrivacyConfig::default();

        let v1 = evaluate_deterministic_floor(
            &ProposedAction {
                action_kind: "ui".into(),
                summary: "Read vault item".into(),
                target_app_or_path: Some("1Password".into()),
                command_preview: None,
            },
            &privacy,
            false,
        );
        assert!(matches!(v1, Some(PolicyVerdict::Block { .. })));

        let v2 = evaluate_deterministic_floor(
            &ProposedAction {
                action_kind: "shell".into(),
                summary: "Clean disk".into(),
                target_app_or_path: None,
                command_preview: Some("rm -rf /".into()),
            },
            &privacy,
            false,
        );
        assert!(matches!(v2, Some(PolicyVerdict::Block { .. })));
    }

    #[test]
    fn holds_sudo_and_env_paths() {
        let privacy = PrivacyConfig::default();

        let v = evaluate_deterministic_floor(
            &ProposedAction {
                action_kind: "shell".into(),
                summary: "Install service".into(),
                target_app_or_path: None,
                command_preview: Some("sudo launchctl load /Library/LaunchDaemons/x.plist".into()),
            },
            &privacy,
            false,
        );
        assert!(matches!(v, Some(PolicyVerdict::HoldForHuman { .. })));
    }
}
