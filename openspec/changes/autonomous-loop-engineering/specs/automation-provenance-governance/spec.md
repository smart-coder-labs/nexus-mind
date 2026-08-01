# Automation Provenance Governance Specification

## Purpose

Retain auditable authority, usage, and stop evidence for managed runs.

## Requirements

### Requirement: Immutable Run Provenance

The system MUST retain immutable receipts for each run and attempt, including organization/project policy and profile version, input identity, model, pinned extensions, structured output status, tools, network and credential grants, gates, decisions, GitHub actions, cancellation, and final state.

#### Scenario: Review a completed attempt

- GIVEN an authorized reviewer and a completed run
- WHEN the reviewer opens its evidence
- THEN the exact profile, authority, actions, and outcome are traceable

### Requirement: Metering and Enforced Stops

The system MUST record measured usage and cost against profile and run budgets. It MUST cancel or prevent further privileged actions when a cancellation, revocation, turn, timeout, cost, credential, network, or tool boundary wins; retained receipts MUST identify the winning reason.

#### Scenario: Cost-cap cancellation

- GIVEN an active attempt approaching its profile cost cap
- WHEN the recorded usage reaches the cap
- THEN execution stops and the cost and stop receipt are retained

#### Scenario: Explicit cancellation

- GIVEN an active run
- WHEN an authorized user cancels it
- THEN subsequent execution and privileged actions are denied and cancellation provenance is retained
