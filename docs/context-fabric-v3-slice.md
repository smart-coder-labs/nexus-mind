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
| `CONTEXT_FABRIC_BQ_ENABLED` | `off` |
| `CONTEXT_FABRIC_MRL_ENABLED` | `off` |
| `CONTEXT_FABRIC_TOOL_SEARCH_ENABLED` | `false` |

## New contract

`POST /v1/context/assemble` is an authenticated, read-only Compiler v0 boundary.
The legacy `candidates` shape still accepts only backend-verifiable `memory` evidence.
Additive `references` resolve `code` and `sdd` locators inside the backend; clients send
IDs/locators and never content. Code chunk IDs are resolved through `code_projects` and
`code_chunks` after `memory:search` and project visibility checks. SDD references use
`locator.id = artifact_id` and `locator.reference = revision number`, after `sdd:read`
and SDD project visibility checks. Both adapters compare the stored hash and source
generation, and return tenant, snapshot, ACL/policy generation, provenance, and freshness
metadata on the resulting `CandidateEvidence`.
Unverified sources return `unsupported_unverified_source`; missing or cross-tenant/hidden
locators return `evidence_not_found`, without echoing IDs or provider details. Hash and
generation assertions return stable integrity/generation reason codes. The endpoint also
rejects `source_cap=0` deterministically. After
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

## Experimental BQ/MRL shadow

`POST /v1/context/lab/shadow` remains the in-memory laboratory endpoint. The persistent sidecar
path is `POST /v1/context/sidecar/shadow`; it reads only sidecars built from authorized SQLite
`memory_embeddings` rows. `CONTEXT_FABRIC_BQ_ENABLED=shadow` or
`CONTEXT_FABRIC_MRL_ENABLED=shadow` is required; absent, `false`, and every other value are
rejected. The default remains `off`. The endpoint validates tenant, ACL/policy generation,
profile, snapshot, model, preprocessing, normalization, tokenizer, and dimension identity.
It never changes the active profile, ranking lane, or baseline result. Missing or incompatible
sidecars return `sidecar_unavailable` with the dense baseline IDs as fallback.

The real BQ/MRL path requires authorized Float32 embeddings with exactly 768 dimensions and a
compatible profile/generation manifest. The request may provide one query vector or query text;
query text is embedded only by the backend `EmbedService`. Client-supplied document vectors,
arenas, and memory content are rejected. If the service is unavailable, text queries fail closed
with `embedding_service_unavailable`.

BQ stores sign bits in bytes or u64 words and ranks candidates with XOR/popcount. MRL supports
prefixes 768, 512, 256, 128, and 64. Both paths use dense Float32 rescoring for the returned
experimental pool; quantized/prefix ranking is never the final ranking. Metrics report
CandidateRecall@K, alpha, candidate and dense-rescore latency, theoretical payload bytes and
theoretical RSS separately, and quality delta. A payload reduction such as 32x is not an RSS
or end-to-end reduction claim.

Promotion is not automatic. The response remains baseline fallback and reports reason codes;
promotion eligibility requires recall >= .98, alpha <= 8, zero security/freshness violations,
and quality delta <= 1pp, plus the existing immutable-manifest/NX-Gold/operator gates. Alpha
above 8 is diagnostic only. The lab runner uses deterministic synthetic inputs and flags, but
sample data remains `NX-Gold v0: PENDING` and is never reported as a gold pass.

## Migration and follow-up

This slice adds user-applied migration v60 and never auto-applies it at startup. Durable
profile/generation publication, atomic artifact pointers, user-applied migrations,
NX-Gold promotion evidence, Tool Search, persistent/distributed cache, automatic migration,
and large UI remain follow-up work. The v60 BQ/MRL sidecar remains shadow-only. The rollout controls in this slice are operational controls for the
baseline-compatible Context Fabric only, not a replacement for the legacy APIs.

Memory-schema-v2 compatibility limit: Context Fabric does not add columns to
`memories` or create a second memory schema authority. Optional provenance,
generation, freshness, and lifecycle details are carried by the versioned
Context Fabric evidence/diagnostic contracts and derived from the existing
memory row when read. Legacy memory payloads remain unchanged; durable
provenance columns require a separately user-applied additive migration.

Code graph and SDD artifact revisions are now supported only through the
`references` variant of `POST /v1/context/assemble`; no arbitrary symbol/file source,
tool result, document upload, or client content is supported. The existing `GET /v1/code/*`
and `GET /v1/sdd/*` permission surfaces remain the source-of-truth query/policy layers.
Memory legacy requests remain unchanged. BQ/MRL remain shadow-only and the NX-Gold recall, quality,
security, and freshness gate is still pending; no automatic promotion is performed. The pure
deterministic primitive remains covered by unit tests.

RSS, end-to-end, and NX-Gold gates remain pending. This implementation intentionally does not
claim those gates or run large benchmarks.
