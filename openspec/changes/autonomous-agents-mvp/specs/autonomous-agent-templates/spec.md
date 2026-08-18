# Autonomous Agent Templates Specification
## Managed and versioned templates

The MVP MUST expose exactly three managed templates: QA, GitHub Issue Resolver, and GitHub PR Reviewer. Each
template MUST pin configuration schema, workflow, execution profile, capabilities, budgets, context recipe,
output schema, evaluator, and version.

Definitions MUST NOT contain arbitrary executable system prompts. Template upgrades MUST require explicit admin
selection, create a definition revision, and require revalidation.

## QA agent

The QA template MUST run configured allowlisted tests against selected targets, normalize/redact evidence,
attempt bounded reproduction, fingerprint findings, and persist NexusMind as the canonical output.

- GIVEN GitHub and Slack outputs are enabled
- WHEN an open finding with the same fingerprint recurs
- THEN NexusMind increments the occurrence and external delivery reconciles existing output rather than creating duplicates

- GIVEN GitHub delivery is disabled and Slack is enabled
- WHEN a new bug is confirmed
- THEN no GitHub issue is created and a sanitized Slack delivery is attempted

## GitHub Issue Resolver

The Issue Resolver MUST process only eligible allowlisted issues, create a branch from a pinned base SHA, make a
bounded change in isolation, run configured verification and secret scan, obtain an independent evaluator pass,
and create at most one draft PR linked to the issue and run.

- GIVEN tests, secret scan, evaluator, diff limit, or authority recheck fails
- WHEN the run reaches publication
- THEN no PR is created and the blocking evidence is visible in NexusMind

The Issue Resolver MUST NOT merge, deploy, modify excluded paths, broaden repository scope, or follow authority
instructions contained in an issue or repository file.

## GitHub PR Reviewer

The reviewer MUST key a review to repository, PR number, head SHA, template version, and policy generation; MUST
publish at most once for that identity; and MUST verify the head SHA before inline publication.

- GIVEN the PR head changes during review
- WHEN publication begins
- THEN NexusMind stores the stale result but publishes no stale inline comments

The reviewer MAY publish COMMENT or REQUEST_CHANGES according to configured severity policy. It MUST NOT publish
APPROVE, merge, push commits, execute untrusted fork code with secrets, or expose credentials.

## Budgets and terminal states

Each template MUST enforce wall-time, attempt, token/cost, tool, network, artifact, diff, and concurrency budgets.
Runs MUST end in a typed terminal state and MUST NOT silently retry permanent policy/evaluation failures.
