# Local autonomous worker protocol v1
The MVP worker is colocated in the backend process. This contract is versioned so it can later become a separate same-host process without changing run semantics.

1. Scheduler creates one durable run for an occurrence key. Misfires are either skipped or collapsed to one run.
2. A worker performs a fresh Claude Code version/auth probe. Only `ready` may continue.
3. Claim atomically changes `queued` to `leased`, creates an automation attempt and a single expiring lease. Expired leases are released and reclaimed.
4. Start verifies organization, attempt, lease expiry and token binding, changes the run to `running`, and appends `run.started`.
5. Events are append-only, monotonically sequenced and sanitized. Artifact metadata must contain a content hash, media type, byte size and trust label; secrets and raw environments are forbidden.
6. The worker periodically observes cancellation. Cancellation drops and kills the Claude process tree, releases the lease and records a terminal event.
7. Results are bounded structured JSON. The orchestrator—not Claude—performs connector writes after an authority and revocation recheck.
8. Finish releases the lease, writes exactly one terminal status and appends `run.finished`. Callback replay must match the original content hash.

Terminal states are `succeeded`, `partial`, `failed`, `cancelled`, `blocked_policy`, `blocked_runtime`, `budget_exhausted`, and `dead_letter`. A runtime-auth failure consumes no new lease while health is `reauth_required`.
