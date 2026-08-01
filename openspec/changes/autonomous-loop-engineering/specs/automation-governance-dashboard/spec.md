# Automation Governance Dashboard Specification

## Purpose

Expose governance, evidence, and human control over automation.

## Requirements

### Requirement: Policy and Checkpoint Governance

The dashboard MUST expose organization policy, stricter project/repository policy, approvals, pilot gates, schedules, budgets, limits, and human checkpoints to authorized users only. Authorized users MUST be able to configure or revoke QA environments only with explicit project/repository and branch allowlists; protected, default, and production branches MUST be rejected.

#### Scenario: Authorized governance action

- GIVEN an authorized administrator or super user
- WHEN viewing or deciding a governed action
- THEN applicable policy hierarchy and required checkpoint are visible

#### Scenario: Unauthorized access

- GIVEN a user outside the organization or role scope
- WHEN accessing governance data or actions
- THEN the system MUST deny access without leaking evidence

#### Scenario: Invalid QA environment

- GIVEN an authorized administrator configuring a QA environment
- WHEN the target includes a protected, default, or production branch
- THEN the system MUST reject the configuration and record the decision

### Requirement: Auditable Run Evidence

The dashboard MUST show run state, context manifest, provenance, policy version/generation, receipts, evaluator/gate results, costs, retries, cancellations, merge target/actor/decision, deployment handoff and status evidence, human validation, rollback/stop status, and PR/issue mappings; records MUST remain available after rollback.

#### Scenario: Failure and rollback review

- GIVEN a failed or revoked run
- WHEN an authorized reviewer inspects it
- THEN the reviewer can trace evidence and confirm no further writes occurred

### Requirement: Profile and Emergency Controls

The dashboard MUST let only authorized organization administrators view approved profile versions, project allowlists, extension status, and usage. It MUST provide an auditable revoke or cancel control; it MUST NOT allow a repository to grant itself a profile, extension, credential, or broader permission.

#### Scenario: Revoke an approved profile

- GIVEN an authorized administrator and an active profile version
- WHEN the administrator revokes that version
- THEN affected runs are stopped according to policy and the action is recorded

#### Scenario: Repository self-authorization attempt

- GIVEN a repository configuration requesting an unapproved capability
- WHEN it is submitted or discovered
- THEN the dashboard shows it as denied without applying it
