# Delta Spec — Foundation and Index Profiles

## Scope

Context Fabric must expose one versioned contract shared by backend, admin, MCP,
plugins, and current context/search/code/memory consumers. Tool Search discovery,
selection, handles, and host changes are explicitly excluded and belong to the
separate `nexusmind-mcp` change; this change only defines the measured boundary.

## Requirements

### Requirement: Explicit publishable manifests

Every publishable index/model profile MUST have an immutable manifest containing
snapshot/commit, tenant scope, chunker and preprocessing identity, model and
revision/hash, license/origin, prefixes, dimension, dtype, normalization,
tokenizer, generation identifiers, derived artifacts, ACL/policy generation,
and consumer compatibility. Missing or contradictory fields MUST reject
publication with a typed diagnostic and leave the active profile unchanged.

### Requirement: Preserved compatibility baseline

The current Nomic 768-dimensional Float32 pipeline, current text preprocessing,
and reproducible flat dense and hybrid+RRF behavior MUST remain addressable as a
named baseline profile. No deployment, migration, default, or consumer adapter
may silently change it. New profiles MUST be versioned and selected explicitly.
Preprocessing or embedding changes MUST remain shadow-only until benchmark
evidence and promotion approval satisfy the NX-Gold gates.

### Requirement: Generation coherence and atomic publication

Each read MUST capture one generation at start and MUST NOT mix FTS, dense,
quantized, graph, memory, or policy artifacts from other generations. A
publication MUST validate all artifacts before changing the active pointer. A
failure, restart, or incomplete artifact MUST retain the previous active
generation and make incomplete artifacts non-readable.

### Requirement: Consumer compatibility

Versioned retrieval, context assembly, generation, verify, memory, and operational
contracts MUST advertise capability and contract versions. Existing `/v1/search`,
`/v1/context`, code, memory, and graph behavior MUST remain compatible, with an
explicit baseline fallback. Incompatible changes MUST use a new contract version.
Backend owns authorization and data access; admin, MCP, and plugins MUST NOT
implement a second policy engine or read private overlays/databases.

### Requirement: User-applied migrations

Any required migration MUST be additive, versioned, idempotent, documented with
preflight, backup, apply, verification, and restore/rollback steps, and remain
pending until explicitly applied by the user. Runtime startup MUST NOT apply it
silently. Failed preflight or migration MUST leave the prior contract usable.

## Acceptance scenarios

### Scenario: Baseline remains the default

- GIVEN a deployment with the existing Nomic 768 Float32 profile
- WHEN Context Fabric is enabled without an explicit new profile
- THEN retrieval uses the named baseline and existing consumers observe compatible behavior
- AND no embedding, prefix, normalization, or preprocessing value changes implicitly

### Scenario: Invalid manifest cannot publish

- GIVEN a candidate manifest missing its model hash or ACL generation
- WHEN an operator requests publication
- THEN the request fails with a machine-readable manifest diagnostic
- AND the active profile and generation remain unchanged

### Scenario: Cross-generation read is rejected

- GIVEN a reader captured generation G and an artifact is only available in G+1
- WHEN the reader attempts to combine artifacts
- THEN the combination is rejected or abstains with a generation-mismatch reason
- AND it does not return mixed evidence

### Scenario: User applies an additive migration

- GIVEN a migration is required and has passed preflight
- WHEN the user applies it, verifies it, and restarts the service
- THEN the new contract is available and a second application is a no-op
- AND if restore is requested, the prior documented state can be restored without runtime auto-rollback

### Scenario: Legacy consumer falls back

- GIVEN a consumer supports only the baseline contract
- WHEN it calls a deployment with a newer profile available
- THEN the adapter selects the compatible baseline or returns an explicit unsupported-version error
- AND it never receives an unversioned response.
