# Autonomous agents operations

## Single-server setup

Install a reviewed, pinned Claude Code release on the same host as the NexusMind backend. Set `CLAUDE_CODE_BIN` to its absolute executable path and authenticate it interactively as the dedicated backend service account before enabling the worker. Verify as that OS account:

```sh
/absolute/path/to/claude --version
/absolute/path/to/claude auth status --json
```

Apply the normal backend migration first, then set `AUTONOMOUS_AGENTS_ENABLED=true` for both services. Run the API server and `autonomous_worker` as two separately supervised processes under the same dedicated OS account and against the same absolute `DB_PATH`. The worker refuses an in-memory database or schema older than v62 and never runs migrations itself. It uses disposable system-temporary workspaces. Start exactly one worker in the MVP; keep it absent on API-only replicas.

For example, the supervisor commands built from this crate are:

```sh
/opt/nexusmind/nexusmind
/opt/nexusmind/autonomous_worker
```

Give each process an independent restart policy and log stream. Stop the worker before the API during maintenance; restart the API first after migrations, then the worker. A worker restart recovers durable queued work and expired leases from SQLite.

Claude sessions expire. NexusMind detects this before every lease, sets runtime health to `reauth_required`, leaves scheduled work durable and performs no agent external writes. An operator must authenticate Claude Code again directly on the server, then use Admin → Automation → Runtime → Check again. NexusMind never accepts or refreshes Claude credentials.

## GitHub App

Create separate development and production Apps. Grant Metadata read, Contents read/write, Issues read/write, Pull requests read/write and Checks read; subscribe to issues, pull requests, installation and installation-repositories events. Configure the webhook URL as `/v1/autonomous-agents/github/webhook` and use a random webhook secret of at least 16 characters.

In Connections, enter public metadata as `{"app_id":"12345","installation_id":67890}`. Enter the write-only secret as JSON containing `private_key` and `webhook_secret`. NexusMind encrypts it and never returns it. Rotate by saving the same connector name with a new secret; revoke before deleting or suspending the provider installation.

For Slack, store a dedicated incoming-webhook URL. Prefer a channel dedicated to agent reports and rotate it after membership or incident changes.

## Safe rollout and recovery

Create agents disabled, configure targets/connectors, validate, then enable. Pilot QA with NexusMind-only output before adding Slack or GitHub. Issue Resolver always opens a draft PR; PR Reviewer only comments or requests changes.

Alert on `reauth_required`, growing queued runs, expired leases, dead-letter deliveries, connector degradation and repeated policy blocks. During an incident stop the worker, set `AUTONOMOUS_AGENTS_ENABLED=false`, revoke relevant connectors, and disable definitions. Runs and findings remain in SQLite for investigation. Back up SQLite before deployment; rollback both binaries with the worker flag disabled. Migration v62 is additive and should be applied through the normal explicit production migration procedure.

Pilot acceptance: 06:00 timezone schedule fires once; session-expiry drill creates no lease; webhook replay creates no duplicate run/review; revocation-before-publish creates no external write; QA deduplicates findings; resolver creates one bounded draft PR; reviewer never approves; cancel terminates work; no canary secret appears in events, findings, receipts or logs.
