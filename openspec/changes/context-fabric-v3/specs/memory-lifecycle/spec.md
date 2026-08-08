# Delta Spec — Typed Memory, Write Gate, and Lifecycle

## Requirements

### Requirement: Typed memory compatibility

Memory operations MUST preserve the existing memory schema-v2 contract and add
Context Fabric provenance where needed without breaking legacy callers. Memory
types, scope, project, tenant, session, source, generation, freshness, and
retention metadata MUST be explicit in versioned responses when present.

### Requirement: Write gate and permissions

Every memory create, update, consolidation, archive, restore, and delete MUST
pass the backend write gate for authenticated tenant, scope, permission,
content/type validity, retention policy, and provenance. Admin, MCP, and plugins
MUST use the gate and MUST NOT write directly to storage. Denied writes MUST be
audited with a reason code and MUST not create partial rows or embeddings.

### Requirement: Lifecycle and retention

Lifecycle states MUST distinguish active, archived, expired, consolidated, and
deleted/tombstoned according to the existing retention contract. Search and
retrieval MUST exclude non-readable states. TTL, archive, consolidation, and
delete MUST update generation/freshness dependencies and preserve required audit
records. Restoration MUST re-check current authorization and freshness.

### Requirement: Org isolation and migration safety

All memory reads, writes, lifecycle operations, indexes, and caches MUST be
tenant-scoped. Additive migrations MUST be user-applied and idempotent. A
partial migration or invalid typed payload MUST not expose or mutate another
tenant's memory.

## Acceptance scenarios

### Scenario: Valid typed memory passes the gate

- GIVEN an authenticated caller with memory-write permission and valid type/scope
- WHEN it stores a memory through the versioned API
- THEN the memory is persisted with tenant, provenance, and lifecycle metadata
- AND an audit event and invalidation dependency are recorded

### Scenario: Unauthorized memory write is atomic

- GIVEN a caller lacks write permission or targets another tenant's session
- WHEN it submits a memory write
- THEN the API returns forbidden or validation failure
- AND no memory, embedding, audit-success record, or partial index entry is created

### Scenario: Archived memory is excluded

- GIVEN a memory is archived or expired
- WHEN search, retrieval, compile, or generation requests context
- THEN the memory is absent from candidates and caches
- AND a permitted restore requires a fresh gate evaluation

### Scenario: Legacy memory caller remains compatible

- GIVEN a client sends the existing memory payload without new optional metadata
- WHEN it stores or searches memory
- THEN the request succeeds with existing defaults and response semantics
- AND Context Fabric metadata does not become a required legacy field.
