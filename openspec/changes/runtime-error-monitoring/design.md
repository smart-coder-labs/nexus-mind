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
