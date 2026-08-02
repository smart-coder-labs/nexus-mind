# Delta Spec — Caches and Freshness

## Requirements

### Requirement: Complete cache identity

Every retrieval, compile, generation, verify, and memory cache key MUST include
tenant, caller/authorization scope as applicable, project scope, policy/ACL
generation, index/model profile, captured generation, freshness requirements,
and source type. A cache MUST never be shared across tenants or authorization
generations.

### Requirement: Deterministic invalidation

Writes, updates, deletes, TTL expiry, archive/restore, ACL or policy changes,
reindex, profile publication, rollback, and generation changes MUST invalidate or
version-away all dependent cache entries before they can be served. Invalidation
events MUST be observable and repeatable without corrupting unrelated tenants.

### Requirement: Freshness enforcement

A request's explicit freshness window MUST be enforced at candidate, compiler,
generation, and verify stages. If the source age or generation cannot be proven,
the result MUST be excluded or abstain with a freshness reason. A source's
declared fallback freshness policy MUST be conservative and visible in metadata.

### Acceptance scenarios

### Scenario: ACL change prevents stale cache disclosure

- GIVEN tenant A has a cached result authorized under ACL generation 4
- WHEN the ACL changes to generation 5 removing one locator
- THEN the old cache is not served
- AND a fresh query cannot return the removed locator

### Scenario: Delete invalidates all dependent stages

- GIVEN a document is present in retrieval, compile, and generation caches
- WHEN it is deleted
- THEN dependent entries are invalidated before the next read
- AND traces report the invalidation without storing the deleted content

### Scenario: Freshness cannot be proven

- GIVEN a source lacks a trustworthy update timestamp or generation marker
- WHEN a consumer requests a bounded freshness window
- THEN the source is excluded or the operation abstains
- AND the response exposes a freshness reason code.
