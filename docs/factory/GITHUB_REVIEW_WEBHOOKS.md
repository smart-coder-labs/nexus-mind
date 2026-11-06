# GitHub requested-review integration

## Account and prerequisites

The reviewer account is the account authenticated in **the backend/worker's `gh` CLI**, not the browser user, a model-provider account, or whichever `gh` happens to be on another laptop. Reviews are posted using that CLI account. Its identity is cached briefly with a token fingerprint; changing CLI credentials invalidates the identity cache.

1. Authenticate `gh` on the worker machine/container and verify `gh auth status --hostname github.com` there.
2. Give the token access to the selected repositories, pull-request reviews, and repository webhook administration. Fine-grained tokens can scope these permissions to the required repos; OAuth tokens need the appropriate repository/hook scopes.
3. Configure `NEXUSMIND_TOKEN_ENCRYPTION_KEY` on the backend (32-byte key as 64 hex chars). Shared webhook secrets are encrypted; plaintext fallback is forbidden.
4. Create, validate and enable a `github_pr_reviewer` agent for the repository. Enable the organization's autonomous agents and the actual worker. The reviewer is a read-only, independent invocation over the pinned diff/source; it does not reuse the implementing agent's model conversation.
5. Expose the backend through a public HTTPS URL reachable by GitHub. This may differ from the admin UI origin.

## Connect from the UI

Open **Factory → Workspaces & workflows → GitHub requested reviews**. Select the enabled reviewer (its configuration supplies the repository), enter the public backend URL and connect.

The backend uses `gh`'s token to create/update a repository webhook subscribing to `pull_request`, generates a cryptographic secret, stores it encrypted and registers:

```text
POST https://PUBLIC_BACKEND/v1/autonomous-agents/github/webhook/CONNECTOR_ID
```

Neither the GitHub token nor the shared secret is returned to the UI or sent to a model. Registration reuses an existing repository connector/callback and reconciles its remote hook rather than blindly adding one on each retry. Setup requires explicit `factory_policy:write` and `autonomous_agent:manage_connectors`; read-only identity/status requires `autonomous_agent:read`.

Existing GitHub App webhooks remain supported at `/v1/autonomous-agents/github/webhook` with their installation-scoped encrypted secret. They use the same requested-review routing rules; App transport authentication does not change the account used to post reviews.

## Project reviewers

In **Factory → Agents → Target**, choose **Project** and select a real NexusMind project. The wizard loads active projects from `/v1/projects`, stores `config.project_id` on the target and reviewer revision, and restores it when editing. A project reviewer has no fixed `config.repository`: the two scope modes are mutually exclusive.

The backend resolves permitted GitHub repositories from the active project and its active descendants in the same organization, using `code_projects.repo_url`. Explicit project links take precedence; legacy indexes without a project link may match the exact canonical project name. Archived branches, repositories and other organizations do not grant access. At least one linked GitHub repository is required to validate/enable a reviewer. This does not change the Code UI's existing repository-linking rules.

Manual runs choose one repository/PR; the backend rejects out-of-scope repositories. Requested-review routing and reconciliation resolve the same scope, and the worker rechecks live membership before checkout/model invocation. Project reviewers use explicit PR events/manual PR runs rather than generic timed runs.

A GitHub webhook still belongs to one repository, but one reviewer covers the whole project. Leave **Repositorio del proyecto** empty to connect all authorized repositories in one operation; optionally fill it to connect only one. Registration reuses existing hooks, so retrying after a partial failure is safe. Already connected repositories remain visible if a later registration fails. Reconnect after adding repositories to register their hooks; reconciliation discovers the updated project scope automatically. The optional setup token never grants the agent authority over repositories outside the project. Existing repository-scoped reviewers and hooks remain supported. No new database migration is required.

## Routing

- `opened`, `reopened`, `ready_for_review`, `review_requested`: enqueue only when the CLI account is a requested reviewer (or an active previously requested subscription exists).
- `synchronize`: review the new head for an active subscription, even if GitHub cleared the pending-reviewer list after a completed review.
- `review_request_removed` for the CLI account or `closed`: detach/cancel outstanding automatic review work. If GitHub emits removal after a completed review, that removal is respected; request the account again to reattach.
- Requests for another user, self-authored PRs and forks do not start automatic reviews. Fork exclusion retains the existing safety policy.

The handler verifies HMAC-SHA256 over the raw request body, scopes the repo to its connector, rechecks revocation, reads the live PR and ignores obsolete/out-of-order heads. Delivery persistence plus enqueue is transactional using nested atomic savepoints. Same delivery/different bytes conflicts; different deliveries for the same agent/repo/PR/head deduplicate. A newer head cancels obsolete automatic review leases; the worker checks assignment/subscription again before model execution and publication and refuses a changed head.

A reconciliation poll every 20 worker ticks (about five minutes at the default poll interval, plus execution time) includes CLI-backed agents (no GitHub App connector is required), recovers currently requested PRs if a webhook was missed, and applies the same assignment/subscription filter. It does not turn every new PR in the repo into a review.

Each enabled reviewer definition matching that organization/repository is a separate configured review agent; deduplication is per definition/head. Disable redundant definitions if only one reviewer profile is desired. The selector in webhook setup chooses the repository through an existing reviewer, not a new global review permission.

## Operate and disconnect

The UI displays the current CLI login, repository, callback and connector health. Request that exact GitHub account as reviewer in GitHub. Monitor **Factory → Runs** for its independent run, transcript, findings and published review. Blocking findings request changes; a clean review does not grant deployment or bypass merge policy.

Disconnect revokes the local secret immediately and attempts to delete the remote hook. If the current CLI account cannot delete it, the UI explicitly asks you to remove the old hook in GitHub settings; local events remain rejected. No external hooks are registered merely by compiling/deploying this code: an operator must select a repository and callback URL.

Tests cover HMAC tampering, assignment filtering, explicit removal, same-head deduplication, stale-head ignore/cancellation and transactional enqueue. Actual delivery from GitHub and posting with your live CLI account still require deployment-specific credentials/public URL and are not simulated as live production success.
