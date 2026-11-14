# Context Fabric

**Status (2026-11-14): not on `main`.** It lives in PR #248 (`feat/context-fabric-v3`, "secure Context Fabric baseline slice"). That PR is open, was last updated 2026-11-08 and is ~460 commits behind `main`. Read its code with `git show origin/feat/context-fabric-v3:<path>` after `git fetch origin feat/context-fabric-v3`. Its migrations are numbered v58–v60, but `main` is already past v111, so they must be renumbered before merge. When #248 merges, move this file's facts into `domain.md`, `modules.md` and `invariants.md` and drop this status note.

## What it is, in business terms

Today an AI tool asks NexusMind for context with `/v1/search` or `/v1/context` and gets ranked memories back. Context Fabric turns that into a **context compiler**: the tool sends what it wants as IDs or locators, and the backend returns a bounded, verified evidence bundle. The bundle covers memories, code chunks and SDD artifact revisions, each with provenance, freshness and a token budget. When the backend can't verify the evidence, it **abstains** instead of guessing.

The value for the customer: agents get less but trustworthy context. No other tenant's or hidden project's data reaches them, no stale or tampered content, and the cost per prompt is predictable.

## Pieces (in PR #248)

| Piece | Where | Notes |
|---|---|---|
| Compiler v0 | `POST /v1/context/assemble` (`api/context_fabric.rs`, `context_fabric.rs`) | Read-only, contract `context-fabric.v0`. Legacy `candidates` accept only `memory`. Additive `references` resolve `code` (chunk id) and `sdd` (artifact id + revision) inside the backend after `memory:search` / `sdd:read` and project-visibility checks. Steps: dedupe, then source cap, then hard budget, then deterministic diagnostics or abstention |
| Generate / verify | `POST /v1/context/generate`, `/v1/context/verify` (`generation_provider.rs`) | The default provider is deterministic, and DeepSeek is optional (key at runtime only). Model output never becomes verified evidence |
| Policy-first retrieval | `api/memory.rs`, `store/sqlite.rs` | Tenant and project visibility apply **while loading** embeddings and FTS candidates, before scoring and truncation. The final visibility query stays as defence in depth |
| Profiles / generations | `/v1/context/profiles/:id/generations`, `/v1/context/generations/active`, `…/rollback` | Immutable manifest publication with an explicit rollback. The baseline is `nomic-768-f32-baseline` |
| Rollout | `/v1/context/rollout/{shadow,canary,promote,rollback}` | Needs `manifest:` and `run:` evidence, an approval operator and a profile/generation identity. Promote needs an active canary and baseline fallback |
| Cache | in-memory (`context_fabric_runtime.rs`) | Keyed by tenant, scope, project, ACL/policy generation, profile, generation and freshness. Writes trigger explicit invalidation events. A miss or anything stale falls back to the baseline or abstains |
| Diagnostics | `GET /v1/context/diagnostics`; admin `getContextFabricDiagnostics()` | Counters and reason codes only, never content, keys or policy config |
| BQ/MRL lab | `/v1/context/lab/shadow`, `/v1/context/sidecar/*` (`context_fabric_sidecar.rs`) | Shadow-only quantized retrieval experiments. Never the final ranking and never auto-promoted |
| Migrations | `/v1/context/migrations[/provenance\|/sidecar][/verify]` | User-applied and never run at startup. Runbook: `docs/runbooks/context-fabric-migration.md` |
| NX-Gold lab | `tools/nexus-context-lab/` (Python) | Synthetic benchmark fixture. `NX-Gold v0: PENDING`, which is not promotion evidence |

Docs in the PR: `docs/context-fabric-v3-slice.md` (start here), `docs/context-fabric-contract-inventory.md` (who owns which contract), `docs/context-fabric-deepseek.md`, plus the SDD specs under `openspec/…/specs/{secure-retrieval-compiler,generation-verify,caches-freshness,memory-lifecycle,foundation-index-profiles,lab-nxgold-rollout}`.

## Flags (`config.rs`, all safe-off)

`CONTEXT_FABRIC_ENABLED=false`, `_PROFILE=nomic-768-f32-baseline`, `_GENERATION=baseline`, `_FRESHNESS_SECONDS=86400`, `_TOKEN_BUDGET=4096`, `_SOURCE_CAP=20`, `_DIAGNOSTICS=true`, `_BQ_ENABLED=off`, `_MRL_ENABLED=off`, `_TOOL_SEARCH_ENABLED=false`. With everything off, the legacy `/v1/search`, `/v1/context` and memory responses are unchanged.

## Invariants (product guarantees, don't trade them for recall)

1. **The backend resolves evidence.** Clients send IDs and locators, never content. Client-supplied content, tool results, uploads and arbitrary documents fail closed.
2. **Policy before enumeration.** Tenant, project, caller scope and ACL/policy generation are resolved before candidates are loaded, scored, truncated or cached.
3. **Denials leak nothing.** Missing, hidden or cross-tenant locators return `evidence_not_found` without echoing IDs. Unverifiable sources return `unsupported_unverified_source`.
4. **Integrity.** The stored hash and source generation are compared on every resolve. A mismatch returns a stable reason code, and the content is never served.
5. **Fail closed.** An unknown provider, version, profile, generation or budget returns a stable reason code. Unknown freshness or a failed gate means falling back to the baseline or abstaining.
6. **Explicit migrations and promotion.** Migrations are user-applied with `settings:write` (no admin bypass) after a backup. Promotion needs operator approval, an evidence manifest and an active canary. BQ/MRL and Tool Search are never promoted by this slice.
7. **Additive contracts.** Legacy callers (MCP tools, plugins) stay on the baseline unless they opt into a versioned capability.

## Permissions

Operational routes (migrations, profiles, generations, rollout, sidecar rebuild/cleanup) require `settings:write`. Read and assemble paths require `memory:read` or `memory:search`, plus `sdd:read` for SDD references.

## Not in the slice (follow-ups)

Tool Search (owned by `nexusmind-mcp`, separate SDD), durable or distributed cache, automatic migrations, NX-Gold promotion evidence, the advanced memory lifecycle and lanes, plugin adapters (`nexusmind-claude-plugin`) and a large admin UI.
