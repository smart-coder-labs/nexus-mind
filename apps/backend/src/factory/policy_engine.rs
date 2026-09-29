//! Per-action policy evaluation (docs/factory/PLAN.md §4). Pure: callers compute
//! floors and milestones, fetch the policies and ask the decision model; this
//! module only decides, in a fixed order where the first deciding rule wins:
//!
//! 1. select the most specific policy (none → `manual`; an unknown project with
//!    a project-scoped policy for the action → hold `project_unresolved`);
//! 2. `never` → deny, `manual` → hold;
//! 3. deterministic floors → hold;
//! 4. `after_fix` / `after_merge` → allow only once the milestone is reached;
//! 5. `criteria` → the decision model, which can only turn an eligible item into
//!    `allow` or keep it held. Not configured, failed, or unsure → hold.

use crate::factory::contracts::{
    Action, ActionPolicy, ActionVerdict, PolicyMode, TaskClass, Verdict, VerdictSource,
};

/// Minimum decision-model confidence for `allow` unless a caller overrides it.
pub const DEFAULT_MIN_CONFIDENCE: f64 = 0.8;

/// What is being decided, and the deterministic facts about it.
#[derive(Clone, Debug, Default)]
pub struct EvaluationInput<'a> {
    pub action: Option<Action>,
    pub project: Option<&'a str>,
    pub task_class: Option<TaskClass>,
    /// Floor rules that fired, e.g. `sensitive_path:.github/workflows/ci.yml`.
    pub floors: Vec<String>,
    pub fix_verified: bool,
    pub merged: bool,
}

/// The decision model's answer for an item that is otherwise eligible.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelDecision {
    NotConfigured,
    Failed(String),
    Allow { confidence: f64 },
    Hold { reason: String },
}

/// The verdict plus which policy produced it, for the decision audit.
#[derive(Clone, Debug, PartialEq)]
pub struct Evaluation {
    pub verdict: ActionVerdict,
    pub policy_version: Option<u32>,
    /// Whether the decision model's answer was used; callers only ask it then.
    pub consulted_model: bool,
}

/// Most specific policy for this action: project + class, then project only,
/// then class only, then any. Scope fields must match exactly when set.
pub fn select_policy<'p>(
    policies: &'p [ActionPolicy],
    action: Action,
    project: Option<&str>,
    task_class: Option<TaskClass>,
) -> Option<&'p ActionPolicy> {
    policies
        .iter()
        .filter(|policy| policy.action == action)
        .filter(|policy| match policy.scope.project.as_deref() {
            Some(scoped) => Some(scoped) == project,
            None => true,
        })
        .filter(|policy| match policy.scope.task_class {
            Some(scoped) => Some(scoped) == task_class,
            None => true,
        })
        .max_by_key(|policy| {
            (
                policy.scope.project.is_some(),
                policy.scope.task_class.is_some(),
            )
        })
}

/// Whether the model must be consulted: only when the policy is `criteria` and
/// nothing earlier in the order already decided. Lets callers skip the call.
pub fn needs_model(input: &EvaluationInput, policies: &[ActionPolicy]) -> bool {
    let Some(action) = input.action else {
        return false;
    };
    let project_unresolved = input.project.is_none()
        && policies
            .iter()
            .any(|p| p.action == action && p.scope.project.is_some());
    input.floors.is_empty()
        && !project_unresolved
        && select_policy(policies, action, input.project, input.task_class)
            .is_some_and(|policy| policy.mode == PolicyMode::Criteria)
}

pub fn evaluate(
    input: &EvaluationInput,
    policies: &[ActionPolicy],
    model: &ModelDecision,
    min_confidence: f64,
) -> Evaluation {
    let verdict = |verdict: Verdict, source: VerdictSource, reason: String| ActionVerdict {
        verdict,
        reason,
        source,
    };
    let Some(action) = input.action else {
        return Evaluation {
            verdict: verdict(Verdict::Hold, VerdictSource::Floor, "action_missing".into()),
            policy_version: None,
            consulted_model: false,
        };
    };
    // Without the item's project, a project-scoped policy for this action might be
    // the one that applies; never fall back past it to a broader policy.
    if input.project.is_none()
        && policies
            .iter()
            .any(|p| p.action == action && p.scope.project.is_some())
    {
        return Evaluation {
            verdict: verdict(
                Verdict::Hold,
                VerdictSource::Policy,
                "project_unresolved".into(),
            ),
            policy_version: None,
            consulted_model: false,
        };
    }
    let policy = select_policy(policies, action, input.project, input.task_class);
    let policy_version = policy.map(|p| p.version.get());
    let done = |verdict: ActionVerdict| Evaluation {
        verdict,
        policy_version,
        consulted_model: false,
    };
    let mode = policy.map_or(PolicyMode::Manual, |p| p.mode);
    match mode {
        PolicyMode::Never => {
            return done(verdict(
                Verdict::Deny,
                VerdictSource::Policy,
                "policy_never".into(),
            ))
        }
        PolicyMode::Manual => {
            let reason = if policy.is_some() {
                "policy_manual"
            } else {
                "no_policy_defaults_to_manual"
            };
            return done(verdict(Verdict::Hold, VerdictSource::Policy, reason.into()));
        }
        _ => {}
    }
    if let Some(first) = input.floors.first() {
        return done(verdict(
            Verdict::Hold,
            VerdictSource::Floor,
            format!("floor:{first}"),
        ));
    }
    match mode {
        PolicyMode::AfterFix => {
            return done(if input.fix_verified {
                verdict(Verdict::Allow, VerdictSource::Policy, "fix_verified".into())
            } else {
                verdict(
                    Verdict::Hold,
                    VerdictSource::Policy,
                    "awaiting_verified_fix".into(),
                )
            })
        }
        PolicyMode::AfterMerge => {
            return done(if input.merged {
                verdict(Verdict::Allow, VerdictSource::Policy, "merged".into())
            } else {
                verdict(
                    Verdict::Hold,
                    VerdictSource::Policy,
                    "awaiting_merge".into(),
                )
            })
        }
        _ => {}
    }
    // criteria: only the decision model can turn this into allow.
    let decided = match model {
        ModelDecision::NotConfigured => verdict(
            Verdict::Hold,
            VerdictSource::Policy,
            "decision_model_not_configured".into(),
        ),
        ModelDecision::Failed(reason) => verdict(
            Verdict::Hold,
            VerdictSource::DecisionModel,
            format!("decision_model_failed:{reason}"),
        ),
        ModelDecision::Allow { confidence } if *confidence >= min_confidence => verdict(
            Verdict::Allow,
            VerdictSource::DecisionModel,
            format!("decision_model_allow:{confidence:.2}"),
        ),
        ModelDecision::Allow { confidence } => verdict(
            Verdict::Hold,
            VerdictSource::DecisionModel,
            format!("decision_model_low_confidence:{confidence:.2}"),
        ),
        ModelDecision::Hold { reason } => verdict(
            Verdict::Hold,
            VerdictSource::DecisionModel,
            format!("decision_model_hold:{reason}"),
        ),
    };
    Evaluation {
        verdict: decided,
        policy_version,
        consulted_model: !matches!(model, ModelDecision::NotConfigured),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::contracts::{PolicyScope, SchemaV1};
    use std::num::NonZeroU32;

    fn policy(
        action: Action,
        mode: PolicyMode,
        project: Option<&str>,
        task_class: Option<TaskClass>,
    ) -> ActionPolicy {
        ActionPolicy {
            schema_version: SchemaV1,
            action,
            mode,
            scope: PolicyScope {
                project: project.map(str::to_string),
                task_class,
            },
            allow: vec!["docs only".into()],
            stop: vec![],
            version: NonZeroU32::new(7).unwrap(),
        }
    }

    fn merge_docs() -> EvaluationInput<'static> {
        EvaluationInput {
            action: Some(Action::Merge),
            project: Some("web"),
            task_class: Some(TaskClass::Docs),
            ..Default::default()
        }
    }

    fn eval(
        input: &EvaluationInput,
        policies: &[ActionPolicy],
        model: ModelDecision,
    ) -> ActionVerdict {
        evaluate(input, policies, &model, DEFAULT_MIN_CONFIDENCE).verdict
    }

    #[test]
    fn no_policy_means_a_person_decides() {
        let v = eval(&merge_docs(), &[], ModelDecision::Allow { confidence: 1.0 });
        assert_eq!(
            (v.verdict, v.reason.as_str()),
            (Verdict::Hold, "no_policy_defaults_to_manual")
        );
    }

    #[test]
    fn never_denies_and_manual_holds_even_if_the_model_would_allow() {
        let allow = ModelDecision::Allow { confidence: 1.0 };
        let never = [policy(Action::Merge, PolicyMode::Never, None, None)];
        assert_eq!(
            eval(&merge_docs(), &never, allow.clone()).verdict,
            Verdict::Deny
        );
        let manual = [policy(Action::Merge, PolicyMode::Manual, None, None)];
        assert_eq!(eval(&merge_docs(), &manual, allow).verdict, Verdict::Hold);
    }

    #[test]
    fn floors_beat_criteria_and_the_model() {
        let mut input = merge_docs();
        input.floors = vec!["sensitive_path:.github/workflows/ci.yml".into()];
        let criteria = [policy(Action::Merge, PolicyMode::Criteria, None, None)];
        let v = eval(&input, &criteria, ModelDecision::Allow { confidence: 1.0 });
        assert_eq!(v.verdict, Verdict::Hold);
        assert_eq!(v.source, VerdictSource::Floor);
        assert!(v.reason.starts_with("floor:sensitive_path"));
        assert!(!needs_model(&input, &criteria));
    }

    #[test]
    fn criteria_without_a_decision_model_holds() {
        let criteria = [policy(Action::Merge, PolicyMode::Criteria, None, None)];
        let v = eval(&merge_docs(), &criteria, ModelDecision::NotConfigured);
        assert_eq!(
            (v.verdict, v.reason.as_str()),
            (Verdict::Hold, "decision_model_not_configured")
        );
    }

    #[test]
    fn the_model_can_only_allow_with_enough_confidence() {
        let criteria = [policy(Action::Merge, PolicyMode::Criteria, None, None)];
        assert_eq!(
            eval(
                &merge_docs(),
                &criteria,
                ModelDecision::Allow { confidence: 0.95 }
            )
            .verdict,
            Verdict::Allow
        );
        for model in [
            ModelDecision::Allow { confidence: 0.5 },
            ModelDecision::Failed("timeout".into()),
            ModelDecision::Hold {
                reason: "touches auth copy".into(),
            },
        ] {
            let v = eval(&merge_docs(), &criteria, model.clone());
            assert_eq!(v.verdict, Verdict::Hold, "{model:?}");
            assert_eq!(v.source, VerdictSource::DecisionModel);
        }
    }

    #[test]
    fn milestones_are_deterministic() {
        let mut input = EvaluationInput {
            action: Some(Action::Close),
            ..Default::default()
        };
        let after_merge = [policy(Action::Close, PolicyMode::AfterMerge, None, None)];
        assert_eq!(
            eval(&input, &after_merge, ModelDecision::NotConfigured).verdict,
            Verdict::Hold
        );
        input.merged = true;
        assert_eq!(
            eval(&input, &after_merge, ModelDecision::NotConfigured).verdict,
            Verdict::Allow
        );
    }

    #[test]
    fn the_most_specific_policy_wins() {
        let policies = [
            policy(Action::Merge, PolicyMode::Criteria, None, None),
            policy(Action::Merge, PolicyMode::Never, Some("web"), None),
            policy(
                Action::Merge,
                PolicyMode::Manual,
                None,
                Some(TaskClass::Docs),
            ),
            policy(
                Action::Merge,
                PolicyMode::AfterFix,
                Some("web"),
                Some(TaskClass::Docs),
            ),
            policy(
                Action::Fix,
                PolicyMode::Never,
                Some("web"),
                Some(TaskClass::Docs),
            ),
        ];
        let chosen = |project, class| {
            select_policy(&policies, Action::Merge, project, class).map(|p| p.mode)
        };
        assert_eq!(
            chosen(Some("web"), Some(TaskClass::Docs)),
            Some(PolicyMode::AfterFix)
        );
        assert_eq!(
            chosen(Some("web"), Some(TaskClass::Tests)),
            Some(PolicyMode::Never)
        );
        assert_eq!(
            chosen(Some("api"), Some(TaskClass::Docs)),
            Some(PolicyMode::Manual)
        );
        assert_eq!(
            chosen(Some("api"), Some(TaskClass::Tests)),
            Some(PolicyMode::Criteria)
        );
        assert_eq!(chosen(None, None), Some(PolicyMode::Criteria));
    }

    #[test]
    fn an_unknown_project_never_falls_back_past_a_project_scoped_policy() {
        // `web` forbids merges; without knowing the PR's project, the org-wide
        // criteria policy must not be used in its place.
        let policies = [
            policy(Action::Merge, PolicyMode::Criteria, None, None),
            policy(Action::Merge, PolicyMode::Never, Some("web"), None),
        ];
        let input = EvaluationInput {
            action: Some(Action::Merge),
            task_class: Some(TaskClass::Docs),
            ..Default::default()
        };
        let v = eval(&input, &policies, ModelDecision::Allow { confidence: 1.0 });
        assert_eq!(
            (v.verdict, v.reason.as_str()),
            (Verdict::Hold, "project_unresolved")
        );
        assert!(!needs_model(&input, &policies));

        // With no project-scoped policy for this action, the org-wide one applies.
        let fix_scoped = [
            policy(Action::Merge, PolicyMode::Manual, None, None),
            policy(Action::Fix, PolicyMode::Never, Some("web"), None),
        ];
        assert_eq!(
            eval(&input, &fix_scoped, ModelDecision::NotConfigured).reason,
            "policy_manual"
        );
    }

    #[test]
    fn evaluation_reports_the_policy_version_for_the_audit() {
        let criteria = [policy(Action::Merge, PolicyMode::Criteria, None, None)];
        let result = evaluate(
            &merge_docs(),
            &criteria,
            &ModelDecision::NotConfigured,
            DEFAULT_MIN_CONFIDENCE,
        );
        assert_eq!(result.policy_version, Some(7));
        assert!(!result.consulted_model);
    }
}
