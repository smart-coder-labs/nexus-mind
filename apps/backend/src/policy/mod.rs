use crate::models::types::{Policy, PolicyCheckRequest, PolicyCheckResponse, PolicyViolation};

/// Evaluates a slice of enabled policies against an incoming request and
/// current daily usage counters. Pure function — no I/O, no panics.
pub fn evaluate(
    policies: &[Policy],
    req: &PolicyCheckRequest,
    tokens_used: u64,
    requests_used: u64,
) -> PolicyCheckResponse {
    let mut violations = Vec::new();

    for p in policies {
        if !p.enabled {
            continue;
        }

        match p.rule_type.as_str() {
            "model_whitelist" => {
                let allowed: Vec<String> = p
                    .config
                    .get("allowed_models")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                if !allowed.iter().any(|m| m == &req.model) {
                    violations.push(PolicyViolation {
                        policy_id: p.id.clone(),
                        policy_name: p.name.clone(),
                        rule_type: p.rule_type.clone(),
                        reason: format!("Model '{}' is not in the allowed list", req.model),
                    });
                }
            }

            "budget_limit" => {
                let max_req: Option<u64> = p
                    .config
                    .get("max_requests_per_day")
                    .and_then(|v| v.as_u64());
                let max_tok: Option<u64> = p
                    .config
                    .get("max_tokens_per_day")
                    .and_then(|v| v.as_u64());

                if let Some(max) = max_req {
                    if requests_used >= max {
                        violations.push(PolicyViolation {
                            policy_id: p.id.clone(),
                            policy_name: p.name.clone(),
                            rule_type: p.rule_type.clone(),
                            reason: format!("Daily request cap ({}) reached", max),
                        });
                        continue; // request cap takes precedence; skip token check
                    }
                }

                if let Some(max) = max_tok {
                    if tokens_used >= max {
                        violations.push(PolicyViolation {
                            policy_id: p.id.clone(),
                            policy_name: p.name.clone(),
                            rule_type: p.rule_type.clone(),
                            reason: format!(
                                "Daily token cap ({}) reached (used: {})",
                                max, tokens_used
                            ),
                        });
                    }
                }
            }

            "pii_redact" => {
                // Skip if no prompt to inspect
                let Some(prompt) = req.prompt_preview.as_deref() else {
                    continue;
                };

                let patterns: Vec<String> = p
                    .config
                    .get("patterns")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                'patterns: for pat in &patterns {
                    match regex::Regex::new(pat) {
                        Ok(re) if re.is_match(prompt) => {
                            let trunc: String = pat.chars().take(40).collect();
                            violations.push(PolicyViolation {
                                policy_id: p.id.clone(),
                                policy_name: p.name.clone(),
                                rule_type: p.rule_type.clone(),
                                reason: format!("Prompt matches PII pattern: {}", trunc),
                            });
                            break 'patterns; // one violation per policy
                        }
                        Ok(_) => {}
                        Err(e) => {
                            tracing::warn!(
                                policy_id = %p.id,
                                pattern = %pat,
                                error = %e,
                                "skipping malformed PII pattern"
                            );
                        }
                    }
                }
            }

            other => {
                tracing::warn!(
                    policy_id = %p.id,
                    rule_type = %other,
                    "unknown rule_type — skipping"
                );
            }
        }
    }

    PolicyCheckResponse {
        allowed: violations.is_empty(),
        violations,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_policy(id: &str, rule_type: &str, config: serde_json::Value, enabled: bool) -> Policy {
        Policy {
            id: id.to_string(),
            org_id: "org1".to_string(),
            name: format!("Policy {}", id),
            rule_type: rule_type.to_string(),
            config,
            enabled,
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
            updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        }
    }

    fn make_req(model: &str) -> PolicyCheckRequest {
        PolicyCheckRequest {
            model: model.to_string(),
            prompt_tokens: None,
            prompt_preview: None,
            user_id: None,
            project: None,
        }
    }

    #[test]
    fn evaluate_no_policies_allows_everything() {
        let resp = evaluate(&[], &make_req("gpt-4o"), 0, 0);
        assert!(resp.allowed);
        assert!(resp.violations.is_empty());
    }

    #[test]
    fn model_whitelist_denies_unlisted_model() {
        let policy = make_policy(
            "p1",
            "model_whitelist",
            json!({"allowed_models": ["claude-3-5-sonnet"]}),
            true,
        );
        let resp = evaluate(&[policy], &make_req("gpt-4o"), 0, 0);
        assert!(!resp.allowed);
        assert_eq!(resp.violations.len(), 1);
        assert!(resp.violations[0].reason.contains("gpt-4o"));
    }

    #[test]
    fn model_whitelist_allows_listed_model() {
        let policy = make_policy(
            "p1",
            "model_whitelist",
            json!({"allowed_models": ["claude-3-5-sonnet", "gpt-4o"]}),
            true,
        );
        let resp = evaluate(&[policy], &make_req("gpt-4o"), 0, 0);
        assert!(resp.allowed);
        assert!(resp.violations.is_empty());
    }

    #[test]
    fn budget_limit_request_cap_triggers() {
        let policy = make_policy(
            "p1",
            "budget_limit",
            json!({"max_requests_per_day": 100}),
            true,
        );
        let resp = evaluate(&[policy], &make_req("gpt-4o"), 0, 100);
        assert!(!resp.allowed);
        assert_eq!(resp.violations.len(), 1);
        assert!(resp.violations[0].reason.contains("request cap"));
    }

    #[test]
    fn budget_limit_token_cap_triggers_when_no_request_cap() {
        let policy = make_policy(
            "p1",
            "budget_limit",
            json!({"max_tokens_per_day": 50000}),
            true,
        );
        let resp = evaluate(&[policy], &make_req("gpt-4o"), 50000, 0);
        assert!(!resp.allowed);
        assert_eq!(resp.violations.len(), 1);
        assert!(resp.violations[0].reason.contains("token cap"));
    }

    #[test]
    fn budget_limit_request_cap_takes_precedence() {
        // Both caps exceeded — only 1 violation for request cap (continue skips token check)
        let policy = make_policy(
            "p1",
            "budget_limit",
            json!({"max_requests_per_day": 100, "max_tokens_per_day": 50000}),
            true,
        );
        let resp = evaluate(&[policy], &make_req("gpt-4o"), 60000, 100);
        assert!(!resp.allowed);
        assert_eq!(resp.violations.len(), 1, "request cap takes precedence → 1 violation");
        assert!(resp.violations[0].reason.contains("request cap"));
    }

    #[test]
    fn pii_redact_matches_pattern() {
        let policy = make_policy(
            "p1",
            "pii_redact",
            json!({"patterns": [r"\d{3}-\d{2}-\d{4}"]}),
            true,
        );
        let mut req = make_req("gpt-4o");
        req.prompt_preview = Some("My SSN is 123-45-6789".to_string());
        let resp = evaluate(&[policy], &req, 0, 0);
        assert!(!resp.allowed);
        assert_eq!(resp.violations.len(), 1);
        assert!(resp.violations[0].reason.contains("PII pattern"));
    }

    #[test]
    fn pii_redact_skips_when_no_prompt() {
        let policy = make_policy(
            "p1",
            "pii_redact",
            json!({"patterns": [r"\d{3}-\d{2}-\d{4}"]}),
            true,
        );
        // prompt_preview is None → no violation
        let resp = evaluate(&[policy], &make_req("gpt-4o"), 0, 0);
        assert!(resp.allowed);
        assert!(resp.violations.is_empty());
    }

    #[test]
    fn pii_redact_skips_malformed_pattern() {
        let policy = make_policy(
            "p1",
            "pii_redact",
            json!({"patterns": ["[invalid-regex("]}),
            true,
        );
        let mut req = make_req("gpt-4o");
        req.prompt_preview = Some("some text".to_string());
        // Malformed regex must not panic and must produce no violation
        let resp = evaluate(&[policy], &req, 0, 0);
        assert!(resp.allowed, "malformed regex must not cause a violation");
        assert!(resp.violations.is_empty());
    }

    #[test]
    fn disabled_policy_is_skipped() {
        let policy = make_policy(
            "p1",
            "model_whitelist",
            json!({"allowed_models": ["claude-3-5-sonnet"]}),
            false, // disabled
        );
        let resp = evaluate(&[policy], &make_req("gpt-4o"), 0, 0);
        assert!(resp.allowed, "disabled policy must be skipped");
    }

    #[test]
    fn multiple_violations_all_returned() {
        let whitelist = make_policy(
            "p1",
            "model_whitelist",
            json!({"allowed_models": ["claude-3-5-sonnet"]}),
            true,
        );
        let budget = make_policy(
            "p2",
            "budget_limit",
            json!({"max_requests_per_day": 100}),
            true,
        );
        // gpt-4o is not whitelisted AND request cap is hit
        let resp = evaluate(&[whitelist, budget], &make_req("gpt-4o"), 0, 100);
        assert!(!resp.allowed);
        assert_eq!(resp.violations.len(), 2, "both policies must fire");
    }
}
