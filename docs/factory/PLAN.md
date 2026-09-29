# NexusMind Software Factory — Implementation Plan

Status: **planning** · Owner: cesar · Last updated: 2026-09-29

Source documents:

- [`Arquitectura_Software_Factory_Hibrida_Portatil_2026_ES.md`](../../Arquitectura_Software_Factory_Hibrida_Portatil_2026_ES.md), the target architecture.
- [BuilderIO/skills factory](https://github.com/BuilderIO/skills/tree/main/docs/factory) (commit `0dba9ef`), where the per-action policy model comes from.

Every decision below is also recorded as an ADR in NexusMind (`project: nexus-mind`, tag `software-factory`).

## 1. Goal

Build a software factory where **≥80% of routine work avoids frontier-model inference**, while meeting explicit SLOs for quality, security and latency. The primary metric is **cost per accepted change**. The trust boundary is deterministic verification inside an isolated sandbox. Models are never the trust boundary.

v1 is for **internal use** on our own repos (nexusmind, kasymir). All data is org-scoped from day one so the factory can become a product later.

## 2. Starting point (verified 2026-09-29)

The factory is not greenfield. The Rust backend already contains part of it:

| Capability | Exists today | Gap |
|---|---|---|
| Orchestration | SQLite lease-based worker (`automation/worker.rs`, `autonomous_agent_runs`/`leases`) | Harden; add idempotency and per-action states |
| Sandbox | Worktrees, cleaned env, `gc_stale_sandboxes` | **No container isolation.** Runs as a host process |
| Verification | Allowlisted commands, security scanners, DAST, judge | No structured `VerificationReport` or single gate |
| Code intelligence | Tree-sitter chunker (5 languages), fastembed Nomic v1.5 | BM25/FTS5, CodeRankEmbed, SCIP, fusion, `ContextPack` |
| Policy | Execution profiles `read-only`/`implementation`/`qa-deploy`; `claude-code` is the only provider | Per-action policies, risk floors, Model Gateway |
| Auto-merge | `auto_merge_pull` gated only by config `required_checks` | **Not risk-aware and not bound to the verified head SHA** |
| Intake | GitHub issues (read); Slack is outbound webhook only | IntakeSource contract and new connectors |
| Telemetry | `report_usage` | Cost per accepted change, shadow routing, golden tasks |

## 3. Decisions

| # | Decision | Choice |
|---|---|---|
| D1 | Stack | Extend the **Rust backend** and the existing worker. Python only as a narrow sidecar where an ML library requires it (e.g. GLiNER). No FastAPI and no Temporal for now |
| D2 | Executor | **Hybrid.** Claude Code stays the agent for cloud code tiers, selected via `--model`. A new Rust **Model Gateway** serves decisions, extraction, and tightly scoped patches (docs, tests) |
| D3 | Audience | **Internal first**, with org-scoped data |
| D4 | MCP tools | New profile **`factory_operator`**. `get_context_pack` goes into **`only_context`**. Every new tool is also added to `legacy` |
| D5 | Retrieval storage | Stay on SQLite: FTS5 `bm25()` for lexical search plus fastembed vectors. Postgres/Qdrant only with the enterprise tier *(assumption derived from D1)* |
| D6 | Decision provider | **Rules + Jev** (the user has a key). Jev can only tighten a decision, never loosen it, and failures go to a human (see §4) |
| D7 | Sandbox | **Rootless Podman/Docker plus an egress proxy allowlist.** Claude Code runs inside the container |
| D8 | Golden tasks | Mined from accepted issues/PRs in nexusmind and kasymir |
| D9 | Local Qwen lane | Deferred to F4, and adopted only if it wins on cost per accepted change *(assumption)* |
| D10 | Auto-merge | Interim (F0): **docs/tests allowlist only** plus required checks; everything else opens a PR. From F3 on, Jev decides within the floors |
| D11 | Plan location | `docs/factory/` in this repo, with one SDD change per phase in NexusMind |
| D12 | Autonomy model | **Independent per-action policies** instead of levels A–D (see §4) |
| D13 | Intake | One `IntakeSource` contract. Staged rollout: F0 GitHub + NexusMind tasks; F3 Slack + Sentry; F4 Gmail (label `factory` only) + transcripts (Notion, Drive/Meet, local `.txt`, manual upload in admin). Gmail and transcripts default to `fix: manual` |
| D14 | Policy location | **NexusMind control plane** (DB, edited by admins), with optional YAML import/export. Repository content can never grant itself permissions |
| D15 | Lookback | F5, recommendations only. Uses NexusMind `bugfix` memories to find recurring causes |
| D16 | Policy editing | **Admin UI only, with no MCP tool.** Gated by new permissions `factory_policy:read` / `factory_policy:write`, not by role. Uses the existing `resource:action` convention and `require_permission`. A migration grants them only to `super_user_template`; every other role gets them explicitly |
| D17 | Merge soak and first SLO | Soak window of **10 minutes** of unchanged head and gates. Starting SLO: **≥90% verification pass** on the docs/tests class, to be recalibrated with golden-task data |

## 4. Policy model

**Actions.** Each of these is decided independently:
`fix`, `publish` (push a branch), `open_pr`, `reply`, `close`, `approve`, `merge`, `deploy`, `notify`, `recover`.

**Modes:** `never` · `manual` · `criteria` · `after-fix` · `after-merge`.
Missing, partial, stale or unreadable evidence means **hold for a human**. One permission never implies another.

**Evaluation order for each action:**

1. **Control-plane policy** for org → project → task class, plus the action's mode. `never` or `manual` stops here.
2. **Deterministic floors.** These always force a human:
   - auth/authz semantics, crypto, payments;
   - destructive migrations, dependency or lockfile changes, CI/infra/IaC;
   - any blocking check failing;
   - a second failed repair attempt;
   - the source is Gmail or a transcript and the action is `fix`.
3. **Jev decision.** Only inside the space still eligible after steps 1–2. Jev may only *tighten* the decision. Timeout, schema violation, provider error or confidence below threshold means a human decides.
4. **Merge-specific checks:**
   - the merge is bound to the **verified head SHA**; a new head restarts verification;
   - a soak window must pass with no head or gate change.
5. **Audit.** Every decision is logged: its inputs, Jev's probabilities, the final verdict and the reason. This feeds the *false-low-risk* metric, P(predicted low | actual high).

## 5. Contracts (`schemas/factory/`)

JSON Schema 2020-12. Rust types are generated or hand-mirrored and tested against the schemas:

- `TaskSpec`: normalized intake item, with source, requirements, `privacy_class` and `origin_trust`.
- `RoutingDecision`: as in the architecture doc, plus a per-action verdict map.
- `ContextPack`: symbols over whole files, with `content_hash` and a `reason` for each artifact.
- `CodeChangeProposal`: used only on Model Gateway patch lanes.
- `VerificationReport`: per-check PASS/FAIL/SKIP/ERROR, `blocking_failures`, the verified `head_sha`, and `eligible_for_merge`.
- `ActionPolicy`: the per-action modes and criteria from §4.

## 6. MCP tools (in `nexusmind-mcp`)

| Tool | Profile(s) | Purpose |
|---|---|---|
| `submit_factory_task` | factory_operator, legacy | Create a task from free text, an issue, or a transcript |
| `get_factory_task` | factory_operator, legacy | Status, `RoutingDecision`, `VerificationReport`, per-action verdicts |
| `list_factory_tasks` | factory_operator, legacy | Filter by status, source, repository, or task class |
| `approve_factory_action` | factory_operator, legacy | A human approves or rejects **one action** on one task (per D12) |
| `get_human_digest` | factory_operator, legacy | Read-only queue of everything waiting on a human |
| `get_factory_economics` | factory_operator, legacy | Cost per accepted change, frontier-avoidance rate, per-tier stats |
| `get_context_pack` | only_context, factory_operator, legacy | Read-only context for agents inside the sandbox |

Policy editing is deliberately **not** exposed through MCP (D16). It exists only in the admin UI, behind `factory_policy:write`, so no agent can widen its own autonomy.

`index.ts` needs `factory_operator` added to `KNOWN_PROFILES`. The backend enforces permissions on every call regardless of profile.

## 7. Phases

Each phase is one SDD change. The exit criterion is measured, not asserted.

### F0: Contracts, baseline and immediate risk (1–2 weeks)

- Restrict `auto_merge_pull` to the docs/tests allowlist and bind it to the verified head SHA *(closes the current risk)*.
- Add the `schemas/factory/*` contracts and their Rust types.
- Add the `ActionPolicy` table, admin API and minimal admin UI, and implement the evaluation order in §4 with Jev stubbed.
- Add a migration for the permissions `factory_policy:read` / `factory_policy:write`, granted to `super_user_template` only.
- Implement the `IntakeSource` trait with GitHub and NexusMind tasks adapters that produce a `TaskSpec`.
- Instrument cost and latency on every run, keyed by `task_id`.
- Run a **Jev spike**: validate the real API contract, latency, price, and typed-output schema support.
- Build the first 50 golden tasks from merged PRs (harness only).
- **Exit:** auto-merge is risk-gated; golden tasks are replayable; costs are recorded per run.

### F1: Secure executor and verification gate (1–2 weeks)

- Run each task in a rootless container with: non-root user, read-only base FS, tmpfs, CPU/RAM/PID/time limits, no docker socket, no host mounts.
- Deny egress by default and route through a proxy with a per-task allowlist (model API, approved registries).
- Keep credentials out of the container: use a credential broker issuing short-lived, task-scoped tokens, with GitHub writes going through the proxy only.
- Implement a `VerificationReport` builder that unifies the existing scanners, tests and DAST into one gate.
- Harden `recover`: only resume with the original authorization and an owned worktree, and never repeat completed writes.
- **Exit:** 100% of generated code runs isolated; the gate produces reports for all golden tasks.

### F2: Code intelligence, "Bibliotecario" (2–3 weeks)

- Add an FTS5 BM25 lexical index over code and docs.
- Add CodeRankEmbed for code (verify fastembed or ort support first), keeping Nomic/BGE-M3 for prose.
- Add SCIP where an indexer exists (rust-analyzer, scip-typescript, scip-python).
- Implement rank fusion, a reranker and dependency expansion, then the `ContextPack` builder.
- Add the MCP tool `get_context_pack` to `only_context`.
- **Exit:** baseline Recall@K and MRR on retrieval golden questions.

### F3: Router and Model Gateway (1–2 weeks)

- Build the Model Gateway: provider credentials, budgets, fallbacks, cache-token accounting, and OTel GenAI spans.
- Map tiers: `DETERMINISTIC` → tools; `DECISION_MODEL` → Jev; `CHEAP_CLOUD` → Haiku/Flash; `FRONTIER` → Sonnet/Opus via Claude Code `--model`.
- Run Jev live inside the floors, in **shadow mode first** against human outcomes.
- Add the `factory_operator` MCP profile and tools, the human digest in admin, and a watchdog that notifies only on meaningful change.
- Add Slack (read) and Sentry intake adapters.
- **Exit:** a measured false-low-risk rate below threshold (OD-5) before automatic routing is enabled.

### F4: Specialists (2–4 weeks)

Workflows, in order:

1. Documentation.
2. Test generation, with mutation or seeded-fault checks.
3. Small UI work.

Also in this phase:

- Apply the "efficient-frontier" pattern inside a task: a frontier model plans and reviews, cheaper tiers implement.
- Benchmark the local Qwen2.5-Coder-1.5B (llama.cpp Q4_K_M) lane against Haiku/Flash on cost per accepted change.
- Add Gmail (label `factory`) and transcript intake (Notion, Drive/Meet, local `.txt`, admin upload), with `fix: manual`.
- **Exit:** each specialist beats baseline on the frozen eval.

### F5: Controlled autonomy and learning (ongoing)

- Widen per-action `criteria` policies class by class, backed by the measured SLOs.
- Build a trajectory store of accepted and rejected runs, with verification data and human edits.
- Add a lookback workflow (recommendations only) that clusters recurrences against NexusMind `bugfix` memories.
- LoRA/QLoRA only if the evals show a persistent behavioral gap.

**Horizon:** 2026-10-04 → 2026-12-13, per the architecture doc. No GPU is needed before F4.

## 8. KPIs, in priority order

1. Production regression rate.
2. Security policy violation rate.
3. Verified task success.
4. Cost per accepted change.
5. Time to green.
6. Human edit burden.
7. Frontier escalation rate.
8. Local execution percentage.

## 9. Open decisions (resolve before the phase that needs them)

| # | Question | Needed by |
|---|---|---|
| OD-3 | Container runtime on macOS dev machines (podman machine vs colima) and on the prod host (Fly.io machine limits for rootless) | F1 |
| OD-4 | Exact model per tier, and a monthly budget cap per org | F3 |
| OD-5 | False-low-risk threshold that enables automatic routing | F3 |
| OD-6 | Where the Model Gateway credentials live (existing `crypto.rs` secret store vs external) | F3 |
| OD-7 | PII redaction policy for Gmail and transcripts before they reach any model | F4 |
