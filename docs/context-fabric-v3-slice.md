# Context Fabric v3: first slice

This slice is deployable without changing the legacy `/v1/search`, `/v1/context`,
or memory response contracts.

## Safe defaults

Context Fabric is disabled by default. The baseline remains
`nomic-768-f32-baseline` with `baseline` generation, existing preprocessing, FTS5,
dense Float32, and hybrid/RRF retrieval by default.

Configuration is typed in `apps/backend/src/config.rs` and can be set with:

| Variable | Default |
| --- | --- |
| `CONTEXT_FABRIC_ENABLED` | `false` |
| `CONTEXT_FABRIC_PROFILE` | `nomic-768-f32-baseline` |
| `CONTEXT_FABRIC_GENERATION` | `baseline` |
| `CONTEXT_FABRIC_FRESHNESS_SECONDS` | `86400` |
| `CONTEXT_FABRIC_TOKEN_BUDGET` | `4096` |
| `CONTEXT_FABRIC_SOURCE_CAP` | `20` |
| `CONTEXT_FABRIC_DIAGNOSTICS` | `true` |
| `CONTEXT_FABRIC_BQ_ENABLED` | `false` |
| `CONTEXT_FABRIC_MRL_ENABLED` | `false` |
| `CONTEXT_FABRIC_TOOL_SEARCH_ENABLED` | `false` |

## New contract

`POST /v1/context/assemble` is an authenticated, read-only Compiler v0 boundary.
This first slice accepts only `memory` evidence that the backend can verify. Every
locator/id is resolved through the caller's tenant and project visibility policy;
content must match the backend memory and provenance must be `memory-search`.
Unverified sources return `unsupported_unverified_source` and are reserved for future
backend adapters. The endpoint also rejects `source_cap=0` deterministically. After
verification, the compiler deduplicates complete units, applies source caps and a hard
budget, and returns deterministic diagnostics or abstention. It does not retrieve data
or change profile/generation state.

## Policy-first retrieval

Semantic and hybrid memory retrieval now apply tenant/project visibility while loading
embeddings and FTS candidates, before scoring and truncation. The final visibility
query remains as defense in depth. Keyword fallback behavior is unchanged when the
embedding service is unavailable.

## Cache and rollout operations

The new-fabric cache is an in-memory, read-only acceleration layer. It is disabled unless
`CONTEXT_FABRIC_ENABLED=true` or `NEXUSMIND_CONTEXT_FABRIC_CACHE=true`; legacy memory/search
paths never consult it. Every identity includes tenant, caller scope/user, project, ACL and
policy generations, profile, captured generation, freshness, source type, contract, lane and
stage budget/tokenizer where relevant. Entries are never written unless the backend has already
verified the evidence, and they expire by TTL. Cache values are opaque in diagnostics.

Memory writes, updates, deletes, archive/restore, retention expiry, policy changes, profile
publication, generation changes and rollout rollback emit explicit invalidation events. Events
with an `event_id` are replay-idempotent. A cache miss, expiry, stale evidence, unknown timestamp,
generation mismatch or failed gate is safe: the request falls back to the baseline path or
abstains; it never serves an unknown candidate.

Protected operational endpoints are:

- `POST /v1/context/rollout/shadow`
- `POST /v1/context/rollout/canary`
- `POST /v1/context/rollout/promote`
- `POST /v1/context/rollout/rollback`
- `GET /v1/context/diagnostics`

Rollout requests require `manifest:` and `run:` evidence, an approval operator, and a profile /
generation identity. Promotion additionally requires an active canary and explicit baseline
fallback. Rollback resets the lane to baseline and invalidates derived entries. These APIs reject
BQ, MRL and Tool Search profile names; they do not enable those capabilities.

Diagnostics expose only active profile/generation, rollout state, cache counters and bounded
reason codes. They do not expose cached content, keys, memory text or policy configuration.

## Migration and follow-up

This slice adds no database migration and never auto-applies one at startup. Durable
profile/generation publication, atomic artifact pointers, user-applied migrations,
NX-Gold, BQ/MRL, Tool Search, persistent/distributed cache, automatic migration, and large UI
remain follow-up work. The rollout controls in this slice are operational controls for the
baseline-compatible Context Fabric only, not a replacement for the legacy APIs.
