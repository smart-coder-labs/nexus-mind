# Delta Spec — Secure Retrieval and Compiler v0

## Requirements

### Requirement: Policy-first candidate generation

For every retrieval request, tenant, user, project, ACL, policy generation,
freshness, and source scope MUST be resolved before enumerating, scoring,
caching, or exposing candidates. Post-filtering an unauthorized candidate MUST
NOT count as compliance. Authorization MUST be enforced by the backend for all
backend, admin, MCP, plugin, code, graph, and memory sources.

### Requirement: Retrieval references and evidence

The contract MUST represent FTS5, dense Float32, and hybrid+RRF as comparable
retrieval references, including profile, generation, candidate count, authorized
scope, locators, freshness, and reason codes. Unauthorized or stale evidence
MUST be omitted and MUST NOT be recoverable through cache keys or traces.

### Requirement: Compiler v0 contract

Context assembly MUST accept an explicit consumer tokenizer and hard token
budget. It MUST select complete units, deduplicate, enforce source caps, apply
exclusions, preserve provenance and locators, report coverage and diagnostics,
and abstain when the budget, authorization, freshness, or evidence requirements
cannot be satisfied. It MUST never silently truncate a unit into misleading
evidence.

### Requirement: Permissions and errors

The backend MUST be the authorization authority. Admin diagnostic/configuration
surfaces MUST use versioned APIs and MUST NOT elevate permissions. Missing
authorization, tenant mismatch, policy denial, stale generation, invalid budget,
and unavailable required evidence MUST return stable machine-readable reason
codes; no denied content may appear in an error body or diagnostic trace.

### Requirement: Strict TDD acceptance

The backend and admin contracts MUST have tests covering successful paths,
authorization ordering, org isolation, compiler budgets, abstention, and every
normative error scenario before implementation is considered complete.

## Acceptance scenarios

### Scenario: Unauthorized candidate is never generated

- GIVEN tenant A's query could textually match a document visible only to tenant B
- WHEN tenant A requests retrieval
- THEN the authorization set is resolved before candidate enumeration
- AND the tenant-B locator, score, cache entry, and evidence are absent

### Scenario: Policy change invalidates retrieval

- GIVEN a cached result was authorized under policy generation P
- WHEN the caller's policy or ACL changes to P+1
- THEN the cached result is not served
- AND retrieval evaluates P+1 before producing candidates

### Scenario: Compiler respects hard budget

- GIVEN authorized evidence exceeds the consumer's token budget
- WHEN Compiler v0 assembles context
- THEN it returns only complete permitted units within the hard cap
- AND reports omitted sources, coverage, provenance, and a deterministic diagnostic

### Scenario: Compiler abstains on insufficient evidence

- GIVEN a request requires a source or freshness guarantee that cannot be met
- WHEN assembly is requested
- THEN the result is an explicit abstention with reason code and no unsupported context

### Scenario: Forbidden admin operation fails safely

- GIVEN an admin caller lacks profile or policy promotion permission
- WHEN it requests an operational mutation
- THEN the API returns forbidden
- AND no profile, overlay, generation, or authorization state changes.
