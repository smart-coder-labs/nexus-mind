# Context Fabric v3 Contract Inventory

This document is the ownership and compatibility fence for the first deployable
Context Fabric v3 slice. It is additive: legacy callers remain on the named
baseline unless they explicitly opt into a versioned capability.

## Contract ownership

| Contract | Owner | Version | Compatibility |
| --- | --- | --- | --- |
| `/v1/search` and legacy memory/context APIs | `nexusmind` backend | legacy | unchanged; baseline Nomic Float32 semantics |
| `/v1/context/assemble` | `nexusmind` backend | `context-fabric.v0` | optional references and compiler fields; legacy memory path preserved |
| `/v1/context/generate` | `nexusmind` backend | `context-generation.request.v1` / `context-generation.response.v1` | deterministic provider remains default; new metadata is additive |
| `/v1/context/verify` | `nexusmind` backend | `context-fabric.v0` | independent verification; model output never creates verified claims |
| Profiles/manifests/generations | `nexusmind` backend | user-applied v58+ | immutable publication and explicit rollback |
| Provenance/sidecar migrations | `nexusmind` backend | user-applied v59/v60 | additive, idempotent, startup never auto-applies |
| Typed memory lifecycle | `memory-schema-v2` contract consumed by `nexusmind` | existing + optional provenance | legacy payloads remain valid |
| ACL/policy decisions | `policy-engine` | existing policy generation | backend resolves policy before enumeration, scoring or caching |
| Code/graph locators | `code-knowledge-graph` | existing locator/snapshot contract | backend-owned resolver adds tenant, ACL, hash and generation checks |
| SDD artifacts/specs | `nexusmind` SDD store | artifact revision contract | backend resolves visible revisions; client content is never evidence |
| Tool Search discovery/handles/hosts | `nexusmind-mcp` | separate SDD | measured boundary only; not implemented or promoted here |
| Consumer/plugin adapters | `nexusmind-claude-plugin` and consumers | external owner | adapters negotiate capabilities; they never authorize or access private stores |
| NX-Gold corpus/runner | `nexus-context-lab` | `NX-Gold v0` | isolated synthetic fixture is structural test data, not promotion evidence |

## Request and response compatibility

- `GenerateRequest.request_version`, `output_byte_budget` and `timeout_ms` are
  optional for legacy JSON callers.
- `GenerateResponse` carries separate retrieval, compile and run metadata. The
  model output is not merged into provenance verification.
- Unknown provider, request version, profile, generation or budget fails closed
  with a stable reason code.
- Unknown or unauthorized source locators are rejected without returning denied
  content, locators or cross-tenant details.
- BQ/MRL flags are independent and remain `off` by default; a failed gate keeps
  `baseline-nomic-768-f32-v1` active.

## Security and data boundaries

1. Authentication, tenant, project, caller scope, policy/ACL generation and
   freshness are resolved by the backend before candidate enumeration.
2. Cache identities include tenant, caller scope, project, policy generation,
   profile, captured generation, freshness, source and stage.
3. Code and SDD evidence is resolved from backend-owned identifiers. Client
   supplied content cannot become trusted evidence.
4. DeepSeek is an optional generation provider. Its key is runtime-only and
   never appears in request bodies, metadata, logs or run manifests.
5. Tool results, arbitrary documents and uploads remain unsupported and fail
   closed until a separate contract is approved.

## Verification references

- Backend Context Fabric unit tests cover generation, compiler, policy, cache,
  provenance, sidecars and evidence resolution.
- `apps/backend/tests/context_fabric_test.rs` covers version compatibility,
  evidence tampering, HTTP assembly and generation failure behavior.
- `tools/nexus-context-lab` validates clean-room isolation, run artifacts,
  protocol metadata and synthetic NX-Gold structure.
