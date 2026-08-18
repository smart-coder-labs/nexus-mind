# Autonomous Agent Connectors Specification
## GitHub App only

Automated GitHub writes MUST use a repository-bound GitHub App installation token and MUST NOT use the existing
user OAuth token or a user-supplied PAT.

- GIVEN a definition targets repositories A and B but its installation grants only A
- WHEN validation or a run requests B
- THEN the operation fails closed before execution

## Webhook verification and replay

The system MUST verify GitHub webhook signatures, unique delivery IDs, accepted event/actions, installation/org
binding, and repository target binding before persisting a trigger.

- GIVEN the same valid delivery is received twice
- WHEN processed
- THEN it causes at most one work item/run

## Slack destinations

Slack connectors MUST be restricted to an explicitly configured destination. Messages MUST contain sanitized
summary data and NexusMind links, never raw secret-bearing logs or credentials.

## Secret handling

Connector and target credentials MUST be encrypted at rest, represented by opaque references, injected only
for the active attempt, redacted from all observable surfaces, and destroyed during teardown.

- GIVEN a canary credential appears in worker input or subprocess output
- WHEN events, artifacts, prompts, findings, deliveries, issues, reviews, PRs, and API responses are inspected
- THEN the plaintext canary appears nowhere

## Independent delivery state

Each configured output MUST have an independent idempotency key, retry state, external identity, and terminal
status. Failure of GitHub or Slack MUST NOT roll back or erase the canonical NexusMind finding.

## Revocation

Connector revocation or installation repository removal MUST increment authority generation, prevent future
writes immediately, pause affected definitions, and surface remediation in the admin UI.
