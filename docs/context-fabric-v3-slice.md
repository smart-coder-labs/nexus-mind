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

## Migration and follow-up

This slice adds no database migration and never auto-applies one at startup. Durable
profile/generation publication, atomic artifact pointers, user-applied migrations,
generation endpoints, BQ/MRL, Tool Search, and NX-Gold rollout remain follow-up work.
