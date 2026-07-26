## Exploration: Autonomous Loop Engineering integration

### Current State
NexusMind is an org-isolated Rust/Axum control plane with SQLite, API-key/cookie authentication, RBAC, blanket mutation audit logging, code indexing/graph APIs, SDD artifacts, tasks, harnesses, agents, and a React admin dashboard. It already has GitHub OAuth storage, but not GitHub App installation/webhook orchestration. Its task model and SDD links provide the right planning substrate; its code index and memory store provide the context substrate.

The supplied `loop-eng` is a working Go orchestrator, not a reusable library. It owns a Mongo-backed job store, GitHub App client, webhook receiver, channel worker pool, CLI drivers, temporary clones, PR creation, scanning, and a separate React dashboard. It does batch same-repository events over a fixed three-second window, but the batch is not explained by compatibility evidence. Context assembly is keyword search plus raw memory concatenation, not deterministic, provenance-scored compilation. It has useful implementations for webhook validation, GitHub App auth, runner abstraction, secret scanning, queue replay, review iteration, and no-auto-merge policy.

Evidence: `apps/backend/src/api/router.rs`, `apps/backend/src/api/tasks.rs`, `apps/backend/src/api/code.rs`, `apps/backend/src/api/harnesses.rs`, `apps/admin/src/pages/Tasks.tsx`, `loop-eng/cmd/orchestrator/main.go`, `loop-eng/internal/nexusmind/client.go`, `loop-eng/internal/scanner/scanner.go`, and `loop-eng/internal/queue/queue.go`.

### Affected Areas
- `apps/backend/src/db/migrations.rs` and `apps/backend/src/db/queries.rs` — additive org-scoped persistent execution model: repositories, installations/credential references, requirements sources, runs, candidate issues, work packages, PRs, checkpoints, receipts, budgets, and immutable event/audit records.
- `apps/backend/src/models/types.rs`, `apps/backend/src/api/router.rs`, and new `apps/backend/src/api/loops.rs` — typed API for onboarding, requirements ingest, run control, approvals, issue/PR traceability, and live status; handlers must reuse `require_permission`, org derivation, and 404 visibility rules.
- `apps/backend/src/api/code.rs` and `apps/backend/src/indexer/*` — reuse indexed code, symbol graph, repo metadata, encrypted indexing credentials, and reindex state; add a deterministic Context Compiler rather than embedding raw search output.
- `apps/backend/src/api/tasks.rs` and SDD APIs — reuse tasks/spec links for human-planned work; link generated candidate issues and work packages without conflating them with external GitHub issue truth.
- `apps/backend/src/api/harnesses.rs` and `apps/backend/src/api/agents.rs` — reuse permissioned, versioned harness/agent concepts for runner profiles and procedural instructions, but do not let a harness grant GitHub or filesystem authority.
- `apps/backend/src/api/github_auth.rs` — OAuth is user/org connection storage; GitHub App installation tokens, webhook secrets, webhook delivery validation, and repo-scoped permissions need a separate connector boundary.
- `apps/admin/src/pages/{Code,Tasks,Harnesses,Agents}.tsx`, `src/api/client.ts`, `src/types.ts`, `src/App.tsx`, and `src/components/Layout.tsx` — add a Loop Operations area: repositories, requirements, runs, approval inbox, work packages, PR receipts, budgets, and evidence drill-down. Reuse TanStack Query, permission-aware navigation, existing task views, and error handling.
- `loop-eng/internal/github/*`, `internal/agent/*`, `internal/security/secretscan.go`, `internal/scanner/*`, and `cmd/orchestrator/main.go` — candidates to adapt into a separate runner/worker, not to embed in the Rust web process.

### Architecture Required
1. **Control plane in NexusMind.** Persist every state transition outside an LLM context: run, phase, attempt, lease, checkpoint, budget reservation/actuals, GitHub event/delivery, candidate issue, work package, PR, evaluation, receipt, and human approval. All records are org-scoped; connector secrets are references to encrypted storage, never prompt material.
2. **Isolated execution plane.** Run each work package in a disposable sandbox/worktree from a pinned commit SHA with least-privileged, short-lived GitHub App installation tokens. The runner reports structured events and receipts; it cannot directly mutate NexusMind tables. A Go worker may initially adapt loop-eng ports, but it must consume NexusMind-owned jobs and durable leases rather than loop-eng's Mongo/channel state.
3. **Deterministic Context Compiler.** Before generator/evaluator calls, resolve the run snapshot (repo SHA, selected requirements versions, permissions, policies, budgets, approved work package, harness version), retrieve code/docs/memories with stable filters and ranking, expand cited symbols/tests, de-duplicate, label untrusted external content, apply a token budget, and persist a manifest of evidence IDs/hashes. The generator receives the manifest and evidence, not an open-ended search result.
4. **Independent generation and evaluation.** Generator proposes a bounded change and receipt; evaluator receives an independently assembled context and checks requirement coverage, scope/diff limits, test/lint results, secret scan, policy and evidence links. An evaluator may request bounded retry; it may not silently broaden scope or approve a merge.
5. **Explainable grouping.** First create immutable candidate issues with requirement/evidence references. A deterministic grouping service forms a work package only when repository/base SHA, ownership/permissions, dependency set, changed-area overlap, risk class, test plan, and review-line forecast are compatible. Persist `grouping_reasons`, rejected candidates, shared constraints, and issue-to-PR/work-package links. A compatible bounded set may produce one PR; a single oversized issue may produce one PR only with an explicit reviewer-size exception. Do not retain loop-eng's fixed three-second repo batch as the product rule.
6. **Gates and checkpoints.** Human confirmation is mandatory for GitHub connection/onboarding, enabling write mode, issue publication, high-risk work packages, retry beyond policy, budget exceptions, and any destructive scope expansion. PR creation requires hard receipts for sandbox identity, base/head SHA, changed-file/diff limits, formatter/linter/test commands and exit codes, secret scan, evaluator decision, and GitHub API result. Default behavior is draft PRs; merge is never an unattended action.
7. **Operations.** Use durable scheduling and leasing, idempotency keys for webhooks/jobs/GitHub writes, bounded concurrency per org/repo/runner, wall-clock/token/cost caps, cancellation, retry policy with dead-letter terminal states, metrics/tracing, and auditable redacted logs. The SQLite single-writer constraint means long-running work and event streams must stay outside DB lock scopes; scale-out requires a durable job/lease implementation before multiple workers.

### Approaches
1. **Embed loop-eng as the NexusMind backend runtime** — Port Go orchestration into Rust/Axum and reuse its behavior.
   - Pros: one deployment and one persistence/auth surface.
   - Cons: rewrites a substantial working system, risks blocking the control-plane process, and forces an unsafe coupling between web API and sandbox lifecycle.
   - Effort: High.

2. **Adopt loop-eng as an external product and synchronize with NexusMind** — Keep its Mongo API/dashboard and add integrations.
   - Pros: quickest apparent delivery; preserves existing runner UI.
   - Cons: duplicate tenants, auth, repositories, dashboards, tasks, audit, configuration, and source of truth; context and grouping would remain weakly traceable.
   - Effort: Medium initially, High operationally.

3. **NexusMind-owned control plane with an adapted loop-eng runner** — Build the new domain/API/dashboard in NexusMind and evolve loop-eng into an isolated worker using a versioned runner protocol.
   - Pros: reuses proven GitHub/CLI/sandbox primitives while retaining NexusMind RBAC, audit, tasks, code graph, SDD, harnesses, and a single dashboard; permits incremental rollout and later runner replacement.
   - Cons: requires a clear protocol, durable job/lease layer, and a migration away from loop-eng Mongo ownership.
   - Effort: Medium-High.

### Recommendation
Choose **Approach 3**. Treat loop-eng as a reference implementation and adapter source, not a product dependency or database owner. Start read-only: onboard a repository, ingest/version requirements, index it, compile deterministic context, generate candidate issues and explainable groupings, and show them in NexusMind. Add GitHub issue creation behind an approval. Then enable isolated draft-PR execution only after durable state, receipt gates, and connector authorization are proven.

Incremental rollout: (0) threat model, RBAC matrix, evaluation corpus, and runner protocol; (1) repository/requirements/candidate/work-package data model plus dashboard read surfaces; (2) deterministic context manifests and evaluator-only dry runs; (3) GitHub App connector with webhook ingestion and approved issue creation; (4) one-runner, one-repo, draft-PR sandbox pilot with mandatory receipts; (5) scheduling, concurrency/cost caps, retries, review feedback, and controlled multi-repo expansion. Keep the review budget at 800 lines: slices should separately deliver schema/control-plane, context/evaluation, connector, runner, and dashboard.

### Risks
- The loop-eng context client calls memory search with concatenated repo URL/keywords and injects full results (`internal/nexusmind/client.go:80-121`, `cmd/orchestrator/main.go:915-941`); it cannot meet deterministic, provenance-preserving context requirements without replacement.
- Its channel queue can drop when full and its batcher groups only by repo/time (`internal/queue/queue.go`); durable, idempotent work-package leasing is mandatory before production automation.
- Current loop-eng Mongo records expose agent prompt/output as job logs; prompts, outputs, webhook bodies, and requirements may contain secrets or untrusted instructions. Redaction, retention, trust labels, and access controls are mandatory.
- `loop-eng` hardcodes/assumes `main` in some reactive PR paths and has differing controls between reactive and scanner flows; base branch, branch protection, and worktree lifecycle must be repository snapshots.
- NexusMind GitHub OAuth currently requests broad `repo,user:email` and uses `memory:write` as the permission gate (`apps/backend/src/api/github_auth.rs`). GitHub App installation, minimal permissions, token rotation, webhook verification/replay controls, and a distinct `loop:*` permission family are required.
- Existing admin task UI includes hard-coded colors and an AdminRoute outer gate; Loop UI must follow current product conventions while backend RBAC remains authoritative.
- Advanced retrieval (BQ/MRL, GraphRAG, late chunking, autonomous memory) is not a v1 dependency. The supplied research supports a measured hybrid retrieval/context compiler baseline first; add advanced techniques only after ablation/evaluation proves a gap.

### Open Product Decisions
- Who may approve connector installation, issue publication, write-mode enablement, high-risk work, and cost/retry exceptions: org admin, project maintainer, or CODEOWNERS-derived reviewer?
- Are GitHub issues the canonical planning object, or are NexusMind candidate issues canonical until explicitly published? The latter is recommended for safety and traceability.
- What makes a PR reviewable beyond the 800-line budget: permitted path sets, required ownership intersection, test-plan compatibility, severity, and whether issues can be independently reverted?
- Which requirements formats and trust levels are accepted, how are document updates versioned, and who can mark an external requirement authoritative?
- What is the initial runner trust model: NexusMind-managed sandbox only, customer-hosted runner, or both? Customer-hosted runners require attestation and a stricter credential boundary.

### Ready for Proposal
Yes — after the proposal explicitly locks the canonical planning identity, approval/RBAC matrix, GitHub App permission set, runner deployment model, v1 hard gates, and measurable pilot success criteria. Evidence supports adaptation of loop-eng's execution primitives but not direct adoption of its persistence, context assembly, batching, or dashboard.
