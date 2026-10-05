# NexusMind Software Factory — Implementation Plan

Status: **F0 and F1 in production; F2 built (pending deploy)** · Owner: cesar · Last updated: 2026-10-03

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
- **Status (2026-10-02): in production, one exit item open.** Every template runs in ephemeral k3s task pods by default (`FACTORY_ISOLATION_DEFAULT=sandbox`); the adversarial drill passed in production. As built (see `openspec/changes/archive/2026-10-02-factory-f1-sandbox`):
  - Containers are Kubernetes pods (user namespaces, PSA restricted, read-only root, no ServiceAccount token, `prlimit` process cap) rather than podman/colima; OD-3 is resolved by that.
  - The credential broker is the egress proxy: pods hold only short-lived HMAC run tokens, and the proxy injects the Claude and NexusMind credentials. GitHub writes stay in the worker (the pod never receives a GitHub token) instead of going through the proxy.
  - Repository code (tests, verification, scanners) runs in a separate commands pod with a registry-only token.
  - **Open:** the gate has produced reports for real sandboxed runs, but the golden task set (kept outside the repo, F0) has not yet been replayed through the sandbox to produce reports for all golden tasks.

### F2: Code intelligence, "Bibliotecario" (2–3 weeks)

- Add an FTS5 BM25 lexical index over code and docs.
- Add CodeRankEmbed for code (verify fastembed or ort support first), keeping Nomic/BGE-M3 for prose.
- Add SCIP where an indexer exists (rust-analyzer, scip-typescript, scip-python).
- Implement rank fusion, a reranker and dependency expansion, then the `ContextPack` builder.
- Add the MCP tool `get_context_pack` to `only_context`.
- **Exit:** baseline Recall@K and MRR on retrieval golden questions.
- **Status (2026-10-03): exit met; CodeRankEmbed pending.** F2 absorbed plan #54 (Explorer/Solver): one retrieval engine, and the ContextPack is #54's ContextPacket (ADR d005bdda). Every choice was measured with `factory-retrieval-eval` (`scripts/factory/eval_retrieval.zsh`).
  - **How it is measured.**
    - Questions come from the golden tasks: each PR title without its `type(scope):` prefix.
    - The gold set is the files the change modified. Added files are excluded.
    - Each repository is indexed from a pinned snapshot: nexus-mind `3de6eb3` (26 questions) and kasymir-app-ui `7e93b88` (20 questions).
  - **Baseline (BM25, the production ranker):**

    | Repository | Hit@5 | Hit@10 | Hit@20 | MRR | Recall@10 | Recall@20 |
    |---|---|---|---|---|---|---|
    | nexus-mind | 0.88 | 0.96 | 0.96 | 0.63 | 0.48 | 0.58 |
    | kasymir-app-ui | 0.65 | 0.80 | 0.85 | 0.51 | 0.22 | 0.30 |

  - **What was tried and lost.** None of the alternatives beat BM25 alone, so none ships.
    - The Nomic embedding ranking that `/v1/code/locate` used: MRR 0.60 on nexus-mind and 0.28 on kasymir.
    - RRF with dense weights from 0.1 to 1.0.
    - Cross-encoder rerankers: bge-reranker-base and jina-reranker-v1-turbo. The multilingual Jina reranker was not tried because its licence is non-commercial.
    - Equal-weight RRF lost too: MRR 0.64 and 0.41 against BM25's 0.63 and 0.51.
  - **Built:**
    - FTS5 index over identifier-split terms (migration v82, contentless).
    - Config files (CI workflows, Dockerfiles, manifests, GraphQL) indexed for lexical search only, weighted 0.7. That removed every code-question regression while CI and deploy questions became answerable.
    - The ContextPack builder: ranked files with their best chunks, then up to three files one relative import away, appended after the ranked files. Each artifact carries a reason and a content hash, and the evidence carries the code under a byte budget.
    - Packs are pinned to a commit (`code_projects.indexed_commit`, migration v83).
    - `POST /v1/code/context-pack`, and `/v1/code/locate` switched to BM25 with embeddings as the fallback.
    - The MCP tool `get_context_pack` in `legacy`, the curated registry and `only_context` (nexusmind-mcp 0.19.0).
  - **Import expansion.** Appending neighbours raised Recall@8 from 0.428 to 0.433 on nexus-mind and from 0.19 to 0.21 on kasymir, where components import their hooks and interfaces relatively. Letting neighbours displace ranked files lost recall on nexus-mind.
  - **Hardening from the adversarial review.**
    - Config files under secret/credential-named paths are excluded, and so is any config whose content looks like a secret: a k8s `Secret`, private keys, cloud or registry credentials, or literal password/token values.
    - Queries are capped at 4096 bytes and 64 terms.
    - BM25 scores are returned relative to the best hit, because raw scores carry statistics from other tenants.
    - The budget limits evidence, never artifacts.
    - The lexical index rebuilds itself when it is out of step with `code_chunks`, both at startup and after a backup restore.
  - **SCIP is deferred** (ADR d005bdda). Only relative imports resolve to files today; aliased imports end at external nodes.
  - **Open:**
    - CodeRankEmbed. The weak leg is the embedding model, so it is the next candidate. It has no official ONNX export, so it needs a self-export (torch) and a full re-embed.
    - Questions about infra and gold files outside the index: 29 of 241 nexus-mind gold files are docs or unsupported types.
    - Production indexes were last built on 2026-08-28 and 2026-09-07. Config files and `indexed_commit` appear only after a re-index.

### F3: Router and Model Gateway (1–2 weeks)

- Build the Model Gateway: provider credentials, budgets, fallbacks, cache-token accounting, and OTel GenAI spans.
- Map tiers: `DETERMINISTIC` → tools; `DECISION_MODEL` → Jev; `CHEAP_CLOUD` → Haiku/Flash; `FRONTIER` → Sonnet/Opus via Claude Code `--model`.
- Run Jev live inside the floors, in **shadow mode first** against human outcomes.
- Add the `factory_operator` MCP profile and tools, the human digest in admin, and a watchdog that notifies only on meaningful change.
- Add Slack (read) and Sentry intake adapters.
- **Exit:** a measured false-low-risk rate below threshold (OD-5) before automatic routing is enabled.
- **Status (2026-10-05): shadow, gateway tiers, operator surface, Slack/Sentry intake with live Jev starts, the watchdog and the Codex executor built. OTel is open; the merge exit needs human labels.**
  - **Decided:**
    - Shadow comes first, so the measurement clock starts before the gateway exists.
    - Ground truth combines post-merge signals with a human label, and the label wins.
    - **OD-5:** at most 2% false-low over at least 50 settled `allow` decisions per class.
    - **Allow rule:** normalized risk ≤ 0.15 with confidence ≥ 0.9, in any class.
    - **Scope:** every repository, Kasymir included. The user accepted that PR metadata goes to TypeSafe.
    - **OD-4/OD-6:** the gateway must support model API keys and the Claude Code / Codex CLIs on subscription.
  - **Built:**
    - `factory/jev.rs`, the provider. It validates answers against the questions asked.
    - Migration v84, `factory_shadow_decisions`.
    - A shadow verdict after every PR review, best effort. `FACTORY_JEV_SHADOW=off` disables it.
    - Outcome signals within 7 days: a revert, a conventional fix touching the same files, or red CI on the merge commit.
    - An hourly refresh in the worker.
    - The `factory-shadow backfill|refresh|report` binary.
    - `GET /v1/factory/shadow/report`, `GET /v1/factory/shadow/decisions` and `POST …/:id/label`.
    - The admin panel on the factory policies page.
  - **Volume:** no PR reviewer is active in production, so the measurement starts from a backfill of already-merged PRs, whose outcome is known at once.
  - **First results (2026-10-04, 153 merged PRs: 63 nexus-mind, 90 kasymir-app-ui).**
    - Jev allows 5 (3%): 4 clean, 1 pending.
    - Jev holds 148: 121 clean, 27 pending.
    - There are 0 high-risk outcomes.
  - **Signals recalibrated (#295, #296).** The first signal set ("any fix touching the same files", "any failing check") flagged 74% of merged PRs. Signals are now strict: a revert, a fix naming the PR, or a failed *required* check. Neither repository requires checks today.
  - **Reading.** With no positive cases, the false-low rate cannot be validated yet. Human labels in the admin are the missing ground truth. Jev is very conservative (every settled hold was clean), so automating anything beyond docs will need a threshold recalibrated against those labels.
  - **Model Gateway (#297):**
    - Each run gets a model (`--model`), picked in this order: pin, then tier, then label class, then the template default.
    - Reviewer, judge and security templates run on Opus.
    - Frontier calls are capped at 20 per day, counted from `model.selected` events.
    - Claude Code runs on the subscription token.
    - **Codex (#302, #303, ADR 74e25f77):** runs on an OpenAI API key, never on a subscription, which rotates refresh tokens.
      - **Scope:** the issue resolver and the PR reviewer, sandbox only.
      - **Credentials and model:** the key exists only in the egress proxy. Each run's token carries its model tier, and the proxy accepts only that model.
      - **What the shell can reach:** Codex has a shell in its pod, so its token reaches only `openai` and `nexusmind`, never tunnels.
      - **Request checks:** the proxy checks every request:
        - known fields only;
        - only local tools and inline content;
        - `store:false`;
        - bounded output;
        - per-org prompt-cache keys;
        - at most 300 requests per run.
      - **Cost:** a run does not start unless its model has a price (`FACTORY_OPENAI_PRICES`, long-context rates on purpose).
      - **Residual risk:** spend is measured from the pod's own output. An OpenAI project spend limit is the backstop.
      - **To enable:** add `FACTORY_OPENAI_API_KEY` to the proxy secret.
    - **Repository config is untrusted (#302, #304):** agent Claude starts with `--setting-sources user --strict-mcp-config`, and `.codex/` never reaches a pod. Repo hooks and MCP servers otherwise run before the model acts; this was verified for both CLIs.
  - **Operator surface (#298, nexusmind-mcp#19):**
    - `GET /v1/factory/digest` lists:
      - merges held for a person (latest head per PR);
      - approved merges in soak;
      - runs that stopped short (only for readers of autonomous runs);
      - unstarted factory tasks;
      - unlabelled shadow decisions.
    - `POST /v1/factory/decisions`: a person approves or rejects one held merge.
      - The policy is evaluated first, so a person lifts only a policy or decision-model *hold*, never a deny.
      - Approval starts the 600 s soak, and `merge_after_soak` re-runs every gate.
    - `GET /v1/factory/economics` reports:
      - cost by model and by tier;
      - frontier avoidance, as distinct runs;
      - cost per proposed change. Accepted changes are not tracked yet.
    - `POST /v1/factory/tasks` creates a factory task. It is a NexusMind backlog task labelled `factory` plus its class.
    - The admin page "Needs a human" (`/factory-digest`).
    - The MCP `--profile factory_operator`.
  - **Intake (ADR 55497338, migration v86):**
    - Slack (a top-level human message with a `:factory:` reaction from an allowed person) and Sentry (an unresolved error or fatal issue matching a query) are configured per source in the admin ("Factory intake").
    - Each source points at an issue-resolver agent, which sets the repository.
    - Each source's token is a connector bound to it: the binding holds the kind and, for Sentry, the host. Changing the binding requires a new secret.
    - GitHub intake stays with the issue resolver.
    - The worker polls every 15 min. Each new item becomes a backlog factory task, labelled `untrusted`.
    - Jev decides **live** whether to start the task, inside floors it cannot lift:
      - the org is allowlisted for the model;
      - at most 5 starts per org per day;
      - the source's class and Jev's class are both docs, tests, ui or bugfix;
      - the data is not restricted;
      - the repository is private;
      - the resolver is ready.
    - A start opens a GitHub issue and an explicit resolver run.
    - A PR whose resolver branch names an untrusted intake issue is never merged without a person: an allow becomes a policy hold.
    - **Watchdog:** the org's Slack webhook hears about new held merges, blocked runs and factory tasks (at most once every 15 min), plus a daily summary.
  - **Open:**
    - OTel GenAI spans: in progress, inert until a collector exists.
    - The OpenAI API key for Codex (and a spend limit on its project).
    - Credentials for the first Slack and Sentry sources.
    - Human labels for OD-5.

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
| OD-3 | ~~Container runtime on macOS dev machines (podman machine vs colima) and on the prod host (Fly.io machine limits for rootless)~~ Resolved in F1: ephemeral pods on the production k3s cluster | F1 |
| OD-4 | Exact model per tier, and a monthly budget cap per org | F3 |
| OD-5 | False-low-risk threshold that enables automatic routing | F3 |
| OD-6 | Where the Model Gateway credentials live (existing `crypto.rs` secret store vs external) | F3 |
| OD-7 | PII redaction policy for Gmail and transcripts before they reach any model | F4 |
