# Delta Spec — Generation and Verify

## Requirements

### Requirement: Separate generation contract

Generation MUST consume a versioned compiled context, explicit model/provider
profile, output budget, permitted sources/tools, generation ID, and freshness
policy. Retrieval and compile metadata MUST remain distinguishable from model
output. Benchmark mode MUST be deterministic and record the complete model
manifest and run parameters.

### Requirement: Verifiable response

Generation responses MUST carry provenance for supported claims, source locators,
generation/profile IDs, abstention state, and reason codes. A response MUST NOT
claim evidence excluded by policy, outside the captured generation, or beyond
freshness requirements.

### Requirement: Verify contract

Verify MUST independently check claims against permitted evidence, freshness,
policy, and generation coherence. It MUST distinguish verified, contradicted,
unsupported, stale, unauthorized, and abstained outcomes. Failure to verify MUST
be safe by abstaining or marking the claim unverified; it MUST NOT promote it to
verified.

### Requirement: Tool Search boundary

This change MUST NOT modify Tool Search registration, progressive disclosure,
handles, discovery, refresh, execution, or MCP host behavior. Generation and
verify MAY report Tool Search evidence/metrics only through the separate agreed
contract, and promotion depends on the separate `nexusmind-mcp` SDD.

## Acceptance scenarios

### Scenario: Generated answer cites only permitted evidence

- GIVEN compiled context contains permitted evidence from one captured generation
- WHEN generation returns an answer
- THEN each supported claim points to an allowed locator and matching generation
- AND the response records model, budget, provenance, and freshness metadata

### Scenario: Verify rejects stale evidence

- GIVEN a claim cites evidence older than the requested freshness window
- WHEN verify evaluates the response
- THEN the claim is marked stale or the response abstains
- AND it is not marked verified

### Scenario: Verify rejects unauthorized citation

- GIVEN a generated response cites a locator not authorized for the caller
- WHEN verify runs
- THEN the outcome is unauthorized
- AND the locator/content is not returned to the caller

### Scenario: Generation failure preserves retrieval safety

- GIVEN the provider fails or exceeds the output budget
- WHEN generation is requested
- THEN the API returns a typed failure or abstention
- AND it does not retry with an unauthorized source or silently alter policy.
