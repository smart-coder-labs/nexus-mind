# Delta Spec — Local Lab, NX-Gold v0, and Rollout

## Requirements

### Requirement: Isolated installable lab

`nexus-context-lab` MUST provide a documented, reproducible local install and
preflight that does not require SQLite internals. It MUST use a separate DB,
overlay, artifacts, credentials, and routes from `nexus-local-qa`; bind to
loopback, use an allowlisted CORS policy, fictitious tenants, and block egress
after verified model prefetch. Every run MUST record snapshot/hash and resource
identity.

### Requirement: NX-Gold v0 corpus and protocol

NX-Gold v0 MUST contain 300 scenarios and 900 executions: 75 docs/SDD, 75 code,
60 memory, 45 Tool Search/MCP, and 45 security/freshness; three fictitious
tenants with cross-tenant canaries; 45% Spanish, 35% English, 20% code-switching;
80% answerable and 20% abstention; 35% exact, 40% semantic/relational, 25%
multihop; 3–5 hard negatives each; 60/20/20 scenario split; two annotators and
adjudication. Tool Search is measured but implemented/promoted only by the
separate `nexusmind-mcp` change.

### Requirement: A0-A6 reproducible measurement

The lab MUST support A0 FTS5, A1 dense Float32, A2 hybrid+RRF, A3 policy-first
chunks/generations, A4 Compiler v0, A5 BQ-768-to-Float32, and A6 isolated MRL
then BQ. Runs MUST use AB/BA ordering, 60-second warmup, five 180-second windows
per concurrency, 20 cold restarts, deterministic separated retrieval/generation,
95/5 read/update load, per-stage metrics, and grouped bootstrap of 10,000 samples
with IC95. M1 MUST measure 10k chunks in the lab at concurrency 1/2/4 before
scaling to 100k and the 32GB promotion machine. Artifacts MUST live under
`runs/<run-id>/`.

### Requirement: Promotion gates

Promotion MUST require zero security or freshness violations; BQ candidate recall
≥0.98, alpha≤8, candidates≥2×, retrieval+compile ≥20% better, vector RSS ≥20×
lower; quality loss ≤1pp; Compiler ≥20% token reduction or ≥10% useful-density
gain with ≤1pp gold loss; and Tool Search schema coverage ≥70%, selection loss
≤1pp, and zero prohibited tools. A failed gate MUST keep baseline active.

### Requirement: Observability and rollout

Metrics MUST include profile/model/generation IDs, snapshot, tenant/policy
generation, stage latency p50/p95, candidate counts, ACL rejects, cache
hit/miss/invalidation, freshness age, tokens, bytes/RSS, abstentions, compiler
diagnostics, generation/verify outcomes, and reason codes without prompts,
documents, or memories unless explicitly allowed by policy. Rollout MUST use
independent capability/profile/tenant flags in order: contract/observability,
lab, shadow, fictitious canary, controlled canary, gradual promotion. Promotion
requires immutable manifest, NX-Gold evidence, health/readiness, and operator
approval. Rollback MUST disable the flag, restore baseline profile/generation,
invalidate derived caches, and retain evidence; it MUST not destroy data or
auto-apply migration rollback.

## Acceptance scenarios

### Scenario: Lab cannot contaminate QA or production

- GIVEN the lab is installed beside `nexus-local-qa`
- WHEN a run starts and attempts to access a production/QA DB or egress endpoint
- THEN preflight or isolation blocks it
- AND the run is marked failed with an auditable reason

### Scenario: NX-Gold failed security gate blocks promotion

- GIVEN one canary returns a cross-tenant locator
- WHEN NX-Gold evaluates the run
- THEN the security gate fails regardless of quality or latency
- AND the baseline remains active and no rollout flag is promoted

### Scenario: Successful M1 evidence can advance rollout

- GIVEN A0-A4 complete the 10k-chunk M1 protocol with all required artifacts and gates passing
- WHEN an authorized operator approves the immutable manifest
- THEN only the selected canary flag is enabled
- AND metrics and rollback metadata are available before wider promotion

### Scenario: Rollback after canary regression

- GIVEN a promoted canary violates a freshness or quality gate
- WHEN the operator invokes rollback
- THEN new behavior is disabled, baseline generation/profile is selected, and derived caches are invalidated
- AND traces, manifests, and migration state remain available for diagnosis.
