# Autonomous Run Orchestration Specification

## Purpose

Safely execute bounded engineering runs with independently verified outcomes.

## Requirements

### Requirement: Durable Bounded Run Lifecycle

The system MUST track durable run states, signed isolated leases, attempts, retries, schedules, budgets, concurrency limits, cancellation, and receipts. Workers MUST be isolated; expired, cancelled, or budget-exhausted leases MUST NOT continue.

#### Scenario: Scheduled successful run

- GIVEN an eligible scheduled run within its limits
- WHEN an isolated worker completes its lease
- THEN state, costs, attempts, and receipt are durably recorded

#### Scenario: Lease failure or cancellation

- GIVEN a worker loss, retry exhaustion, or cancellation
- WHEN the limit or cancellation applies
- THEN execution stops and no further privileged action occurs

### Requirement: Managed Claude Code Execution

The MVP MUST execute Claude Code only through a non-interactive CLI invocation or the official Agent SDK, using the leased managed profile and structured output. It MUST NOT execute another provider, an interactive terminal, or a repository-supplied authorization or permission-bypass setting. Profile revocation, cancellation, timeout, turn, cost, tool, network, or credential limit exhaustion MUST stop the attempt before another tool or privileged action.

#### Scenario: Bounded structured attempt

- GIVEN an active `implementation` lease with available limits
- WHEN the worker invokes Claude Code non-interactively
- THEN it emits schema-valid structured output and records the attempt boundary

#### Scenario: Revocation or limit during execution

- GIVEN a running Claude Code attempt
- WHEN its profile is revoked or any enforced boundary is reached
- THEN the worker stops and publishes no further tool or GitHub action

### Requirement: Deterministic Governed Context and Gates

The system MUST produce a versioned, reproducible context manifest with provenance, selected policy, inputs, retrieval settings, and feature flags. MVP MUST support typed metadata, chunks/parent expansion, AST skeletons, BM25+dense RRF, rerank/dedupe, compiler/extractive compression, typed memory, tool handles/progressive disclosure, and prompt cache. Adaptive/decomposed retrieval, BQ+rescore, MRL, bitsets/Bloom, GraphRAG, late chunking, and semantic cache MUST remain benchmark-gated experimental flags; RAPTOR, ColBERT, and LLMLingua MUST remain deferred. Hard gates and an independent evaluator MUST block failed output.

#### Scenario: Reproducible gated execution

- GIVEN identical approved inputs and enabled MVP settings
- WHEN two runs are planned
- THEN their manifests are deterministic and only evaluator-passing output advances

#### Scenario: Experimental regression

- GIVEN an experimental method misses its benchmark or latency gate
- WHEN evaluation completes
- THEN it MUST be disabled/rolled back without invalidating retained evidence

### Requirement: Rechecked QA Merge Transition

The control plane MUST be the sole merge actor. It MUST transition an evaluated PR to merge only when deterministic gates and the independent evaluator pass, the run is not cancelled, and the current policy generation and QA target remain eligible immediately before the write. Merge or deployment-handoff failure MUST stop subsequent automation and retain evidence.

#### Scenario: Eligible QA merge

- GIVEN a passing PR, active lease, and eligible QA target
- WHEN the final policy and cancellation recheck passes
- THEN the control plane merges only to that target and records an immutable receipt

#### Scenario: Revocation race or merge failure

- GIVEN revocation, cancellation, a changed generation, or a failed merge
- WHEN the final recheck or write occurs
- THEN no merge or later deployment action proceeds and the stop reason is recorded
