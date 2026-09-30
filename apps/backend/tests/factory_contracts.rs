//! Conformance of the factory wire contracts: every fixture listed in
//! `schemas/fixtures/factory/cases.json` must be accepted or rejected identically
//! by the JSON Schema and by the Rust types, so the two cannot drift apart.

use nexusmind::factory::contracts::{
    ActionPolicy, CodeChangeProposal, ContextPack, Contract, RoutingDecision, TaskSpec,
    VerificationReport,
};
use serde_json::Value;
use std::path::PathBuf;

fn schemas_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas")
}

fn read_json(path: &PathBuf) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

struct Case {
    contract: String,
    fixture: PathBuf,
    valid: bool,
}

fn cases() -> Vec<Case> {
    let root = schemas_root().join("fixtures/factory");
    read_json(&root.join("cases.json"))["cases"]
        .as_array()
        .expect("cases array")
        .iter()
        .map(|case| Case {
            contract: case["contract"].as_str().unwrap().to_string(),
            fixture: root.join(case["fixture"].as_str().unwrap()),
            valid: case["valid"].as_bool().unwrap(),
        })
        .collect()
}

/// Typed parse plus cross-field validation, dispatched by contract name.
fn rust_accepts(contract: &str, text: &str) -> Result<(), String> {
    fn check<T: Contract>(text: &str) -> Result<(), String> {
        let value: T = serde_json::from_str(text).map_err(|e| e.to_string())?;
        value.validate()
    }
    match contract {
        "task-spec" => check::<TaskSpec>(text),
        "routing-decision" => check::<RoutingDecision>(text),
        "context-pack" => check::<ContextPack>(text),
        "code-change-proposal" => check::<CodeChangeProposal>(text),
        "verification-report" => check::<VerificationReport>(text),
        "action-policy" => check::<ActionPolicy>(text),
        other => Err(format!("unknown contract {other}")),
    }
}

fn round_trips(contract: &str, text: &str) -> bool {
    fn check<T: Contract>(text: &str) -> bool {
        let first: T = serde_json::from_str(text).unwrap();
        let again: T = serde_json::from_str(&serde_json::to_string(&first).unwrap()).unwrap();
        first == again
    }
    match contract {
        "task-spec" => check::<TaskSpec>(text),
        "routing-decision" => check::<RoutingDecision>(text),
        "context-pack" => check::<ContextPack>(text),
        "code-change-proposal" => check::<CodeChangeProposal>(text),
        "verification-report" => check::<VerificationReport>(text),
        "action-policy" => check::<ActionPolicy>(text),
        _ => false,
    }
}

#[test]
fn schema_and_rust_types_agree_on_every_fixture() {
    let mut failures = Vec::new();
    for case in cases() {
        let schema_path = schemas_root().join(format!("factory/{}-v1.schema.json", case.contract));
        let schema = read_json(&schema_path);
        let validator = jsonschema::options()
            .should_validate_formats(true)
            .build(&schema)
            .unwrap_or_else(|e| panic!("{}: invalid schema: {e}", schema_path.display()));
        let instance = read_json(&case.fixture);
        let text = std::fs::read_to_string(&case.fixture).unwrap();
        let name = case.fixture.display().to_string();

        if validator.is_valid(&instance) != case.valid {
            failures.push(format!("schema expected valid={} for {name}", case.valid));
        }
        match (rust_accepts(&case.contract, &text), case.valid) {
            (Err(reason), true) => failures.push(format!("rust rejected {name}: {reason}")),
            (Ok(()), false) => failures.push(format!("rust accepted invalid {name}")),
            (Ok(()), true) if !round_trips(&case.contract, &text) => {
                failures.push(format!("round trip changed {name}"))
            }
            _ => {}
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn every_contract_has_a_schema_and_both_valid_and_invalid_fixtures() {
    let all = cases();
    for contract in [
        "task-spec",
        "routing-decision",
        "context-pack",
        "code-change-proposal",
        "verification-report",
        "action-policy",
    ] {
        assert!(
            schemas_root()
                .join(format!("factory/{contract}-v1.schema.json"))
                .is_file(),
            "{contract} schema missing"
        );
        assert!(
            all.iter().any(|c| c.contract == contract && c.valid),
            "{contract}: no valid fixture"
        );
        assert!(
            all.iter().any(|c| c.contract == contract && !c.valid),
            "{contract}: no invalid fixture"
        );
    }
}
