# Design — Runtime Error Monitoring

## Flow

```
GCP services ──stdout/stderr──> Cloud Logging ──sink (severity>=ERROR, no audit logs)──> Pub/Sub topic
Cloud Monitoring alert policy ──Pub/Sub notification channel──────────────────────────────────┘
                                                        │ push subscription + Google OIDC token
                                                        ▼
                                          POST /v1/monitor/gcp/:source_id   (public)
                                                        │
                    verify token ─> parse LogEntry / incident ─> fingerprint ─> record (one transaction)
                                                        │
                       New / Regressed ─> Slack + GitHub issue      Notify ─> Slack (issue if missing or closed)
                       Quiet ─> count only
```

No npm package or SDK: Next.js apps on Cloud Run are covered by their logs.

## Code

| Piece | File |
|---|---|
| Parse `LogEntry` and incidents, fingerprint, decide | `apps/backend/src/monitor/mod.rs` |
| Google OIDC check (JWKS cached 1 h, refetch on unknown `kid`) | `apps/backend/src/monitor/oidc.rs` |
| Slack alert and GitHub issue | `apps/backend/src/monitor/delivery.rs` |
| Tables (`run_v115` in `db/migrations.rs`), queries | `apps/backend/src/db/monitor.rs` |
| HTTP handlers | `apps/backend/src/api/monitor.rs` |
| Admin page and `gcloud` generator | `apps/admin/src/pages/Monitoring.tsx` |

## Data model (every row scoped by `org_id`)

**`monitor_sources`**: `id, org_id, project, name, service_account, slack_connector_id,
github_connector_id, github_repo, throttle_minutes (30), enabled, created_by, created_at`, with
`UNIQUE(org_id, name)`.

**`monitor_groups`**: `id, org_id, source_id, fingerprint, service, title, severity, count, first_seen,
last_seen, sample (redacted JSON), log_url, issue_number, last_alert_at, status (open|resolved|muted)`,
with `UNIQUE(source_id, fingerprint)`.

Raw events are not stored.

## Endpoints

| Route | Auth |
|---|---|
| `POST /v1/monitor/gcp/:source_id` | Google OIDC: RS256, issuer `accounts.google.com`, `aud = nexusmind-monitor:<source_id>`, `email` = the source's service account, `email_verified` |
| `GET/POST /v1/monitor/sources`, `PUT/DELETE /v1/monitor/sources/:id` | `settings:write` (admins pass) |
| `POST /v1/monitor/sources/:id/test` | `settings:write`. Sends a Slack test message and reads the repo; creates nothing |
| `GET /v1/monitor/groups?source_id=`, `PATCH /v1/monitor/groups/:id {status}` | `settings:write` |

Push responses:
- Unknown source: 404.
- Bad token: 401.
- Paused source, low severity, closed incident or malformed body: 204, so Pub/Sub doesn't retry forever.
- Delivery runs in a background task after the 204.

## Fingerprint

`sha256(source_id | service | key)`. The key is one of:
- For an alert incident: the policy and condition.
- Otherwise: the normalized title plus the top in-app stack frame.

Normalizing the title replaces emails, uuids, long hex strings, quoted strings and numbers with
placeholders. The top frame skips `node_modules`, `node:internal` and `site-packages`, and drops
line and column numbers.

## Decide

| Existing group | Action |
|---|---|
| none | New |
| muted | Quiet |
| resolved | Regressed (status goes back to open) |
| last alert within throttle | Quiet |
| otherwise | Notify |

`record` reads, decides and writes in one transaction under the connection lock, and claims
`last_alert_at` straight away. Concurrent pushes of the same new error therefore produce exactly one New.

## Delivery

- **GitHub:** only through the org's own GitHub App or token connector. It never falls back to the
  server's `gh` identity, so one org can't make NexusMind open issues in another org's repos.
  - New, or a group without an issue: open an issue.
  - Notify, when the issue was closed in GitHub: open a regression issue that links the old one.
  - The issue carries a marker comment, so a timed-out create is found again instead of duplicated.
- **Slack:** through the org's Slack connector, using `send_slack`. Runtime text has `& < >` escaped,
  so it can't post `@channel` or forge links.
- **Issue body:** message and stack go inside a code fence (no mentions, no markdown), with a note
  that they are untrusted.
- **Redaction:** `factory/redact.rs` is applied before storage, so Slack and GitHub only ever get
  redacted text.
- **Failures:** logged. The next alerting occurrence retries; a missing issue is created on the next Notify.

## Admin page

- **Connect GCP project:** name, NexusMind project, push service account, Slack connector, GitHub
  connector and repo, throttle.
- **After saving:** the Setup panel generates the `gcloud` commands from the GCP project ID: topic, sink,
  publisher binding, service account, push subscription with audience, and an optional Monitoring
  channel. An invalid project ID is never pasted into the commands.
- **Test button:** checks Slack and GitHub.
- **Errors table:** count, last seen, issue and log links, Resolve / Mute / Reopen.

## Known limits (shortcuts)

- No per-source ingest rate limit. Each push is one indexed upsert and alerts are throttled. Add a
  limit if one source ever floods the database.
- Repeat alerts are the only retry for a failed Slack or GitHub delivery.
- Permissions reuse `settings:write` instead of new `monitor:*` permissions, which would need a
  role-template migration and role-editor changes. Revisit if non-admin roles need read-only access.

## Server Slack MCP and server `gh` (v116)

A source can use the server's own identities instead of org connectors:

- **GitHub:** `github_connector_id = "server_gh_cli"` opens issues with the server's authenticated `gh`.
- **Slack:** `slack_connector_id = "server_slack_mcp"` plus `slack_channel` (a channel ID like
  `C0123ABCDE`). Each alert runs the Claude CLI with `/app/qa-mcp.json`, allowed only
  `mcp__slack__slack_send_message`, 3 turns at most, a 120 s timeout, and at most 2 runs at a time.
  The message is passed as data between BEGIN and END lines. Untrusted text has `< > @` replaced,
  so it can't mention anyone or carry links.

Both identities belong to the operator. They are allowed only for the org in
`FACTORY_GITHUB_PUBLISH_ORG_ID` (`backend_account_enabled`, the same rule as Factory publication).
The rule is checked when a source is saved (403 `server_integration_not_authorized`) and again at
delivery. `GET /v1/monitor/options` tells the form whether to offer them.

Residual risk: on the MCP path a model reads log text. It can only call the send tool, but it could
still be steered to another channel the Slack account can post to. This is acceptable only because
the path is limited to the operator's own org.

The connect modal now explains the flow. It derives the push service account from the GCP project
ID, and the setup panel shows numbered, explained steps, each with its own Copy button.

## Error audit (v117, v118)

Before a new or regressed error gets its issue, an agent can find why it happens and which repository
causes it. The issue then opens in that repository instead of the source's fixed one.

- **Setup:** the admin picks the executor (Claude, Codex, Pi or nexus) and an optional model in the
  Monitoring modal. NexusMind keeps a managed `error_audit` agent per source (created, updated,
  disabled with it; hidden from the template list). Its runs appear in Factory › Runs.
- **When:** only `New` and `Regressed` groups, at most 10 audits per source per UTC day. Past that,
  or when an audit can't start, the issue opens in the source's default repository as before. While
  an audit is pending, repeats only alert Slack.
- **Candidates:** every repository of the default repository's GitHub owner, scored by:
  - name match with the service and, for database errors, the `db=` database;
  - GitHub code search on the error's literals: the request route, a quoted identifier, the longest
    fixed part of the message, the top stack file.

  The top 5 are cloned: the first as the working directory, the rest read-only as context. The source's
  own credential is used, never the Factory's.
- **Agent:** read-only (`Read,Grep,Glob`, plan mode), 80 turns, $3 and 20 min by default. It returns
  `{summary, root_cause, repository, routing_evidence, suggested_fix, confidence}`.
- **Routing (the trust boundary):** the proposal is used only if it has evidence, sits inside the owner,
  and the credential can read it. Otherwise the default repository gets the issue. Log text is
  attacker-influenced, so the agent never chooses outside that boundary.
- **Result:**
  - The issue carries an "Audit" section (root cause, why this repository, suggested fix, link to the
    run), fenced as untrusted model output.
  - Slack gets a follow-up with the conclusion and the issue link.
  - The group records `issue_repo`, `audit_run_id`, `audit_status` and `audit_summary`.
- **Failure:** a failed or inconclusive run still opens the issue in the default repository, saying
  the audit did not conclude.

## Pause and terminal view

- **Pause:** `PATCH /v1/monitor/sources/:id {enabled}` pauses or resumes a source without
  re-validating it (like Factory intake sources). A paused source acknowledges Pub/Sub pushes and
  drops them, so nothing is recorded, alerted or opened. The Pub/Sub subscription stays as it is,
  so resuming needs no GCP change.
- **Errors view:** a terminal-like list (monospace, `bg-muted`, status tokens only). One line per
  error: last seen, severity, service, title, count, audit state and status. Each line is a
  `<details>` that opens to the source, times, full message, stack, audit conclusion and run, issue
  and log links, and the Resolve, Mute and Reopen actions.

## Supabase and Vercel log drains (v119)

- `monitor_sources.provider` is `gcp` (default), `supabase` or `vercel`, and it is fixed at creation. `provider_ref` holds the Supabase project ref or the Vercel team slug. It is used only to build log links, and only when it is a plain slug.
- Drains post to the public `POST /v1/monitor/drain/:source_id` with `Authorization: Bearer nmd_…`. The token is generated on create and returned once (`ingest_token`). Only its SHA-256 is stored (`ingest_token_hash`, never serialized).
  - The GCP route rejects drain sources, and the drain route rejects GCP sources (404).
- Supabase signs nothing (as of 2026-11), so the token is the only protection. Each source has its own token.
- The body can be a JSON array, a single object or NDJSON, optionally gzip (inflated output capped at 16 MB).
- Once the token checks out, every request gets a 200, because Vercel disables a drain that keeps failing.
- Errors only (`monitor/drains.rs`):
  - **Supabase:** Postgres `error_severity` ERROR/FATAL/PANIC, `error`/`fatal` levels, and 5xx statuses.
  - **Vercel:** `error`/`fatal` levels, 5xx statuses (direct or proxy), and `statusCode -1`, which means the function crashed and is reported as CRITICAL.
  - Request lines drop the query string.
  - At most 50 errors per batch.
- The log link formats (`supabase.com/dashboard/project/<ref>/logs/explorer`, `vercel.com/<team>/<project>/logs?requestIds=<id>`) are not documented by the providers. Shortcut: if one breaks, the alert still works without the link.
