use crate::model::{DecisionRecord, NextAction, TaskExecutionState};
use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Explicit local fallback. This is not Jev; the TypeSafe adapter lives in jev.rs.
pub fn completion_judge(state: &TaskExecutionState, runtime: Option<String>) -> DecisionRecord {
    let (selected, confidence, reason) = if state.requirements_covered < state.requirements_total
        || !state.unresolved_requirements.is_empty()
    {
        (
            NextAction::RetrieveMore,
            0.98,
            "requirements remain unresolved",
        )
    } else if !state.unresolved_evidence.is_empty() {
        (
            NextAction::RunTests,
            0.92,
            "completion evidence is incomplete",
        )
    } else if state.verification.tests_failed > 0 {
        (NextAction::Retry, 0.99, "verification has failing tests")
    } else if state.verification.lint_passed == Some(false)
        || state.verification.typecheck_passed == Some(false)
    {
        (NextAction::Retry, 0.99, "lint or typecheck failed")
    } else if state.verification.commands.is_empty() {
        (
            NextAction::RunTests,
            0.85,
            "no verification command has been recorded",
        )
    } else if state.tool_errors >= 3 {
        (NextAction::HumanReview, 0.90, "tool error budget exhausted")
    } else {
        (
            NextAction::Finish,
            0.90,
            "requirements and verification evidence are present",
        )
    };
    let encoded = serde_json::to_vec(state).unwrap_or_default();
    let state_hash = format!("sha256:{:x}", Sha256::digest(encoded));
    DecisionRecord {
        id: Uuid::new_v4().to_string(),
        task_id: state.task_id.clone(),
        state_hash,
        decision_type: "task_completion".into(),
        candidates: vec![
            NextAction::Finish,
            NextAction::RetrieveMore,
            NextAction::RunTests,
            NextAction::Retry,
            NextAction::HumanReview,
        ],
        selected,
        confidence,
        reason: reason.into(),
        runtime,
        timestamp: Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Verification;

    fn state() -> TaskExecutionState {
        TaskExecutionState {
            task_id: "task-1".into(),
            objective: "test".into(),
            repository: "/repo".into(),
            requirements_total: 1,
            requirements_covered: 1,
            verification: Verification {
                tests_passed: 1,
                commands: vec!["cargo test".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn blocks_finish_when_verification_is_missing() {
        let mut state = state();
        state.unresolved_evidence.push("tests not run".into());
        assert_eq!(
            completion_judge(&state, None).selected,
            NextAction::RunTests
        );
    }

    #[test]
    fn sends_failed_verification_to_retry() {
        let mut state = state();
        state.verification.tests_failed = 1;
        assert_eq!(completion_judge(&state, None).selected, NextAction::Retry);
    }

    #[test]
    fn accepts_complete_evidence() {
        assert_eq!(
            completion_judge(&state(), Some("runtime".into())).selected,
            NextAction::Finish
        );
    }
}
