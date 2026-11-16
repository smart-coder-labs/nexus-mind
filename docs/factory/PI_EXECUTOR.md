# Pi task executor — Oracle and ChatGPT

## What changed

`executor: "pi"` is available for `github_issue_resolver` and
`github_pr_reviewer`. Existing agents keep their executor. Pi uses
`openai-codex/gpt-6.1-sol` through ChatGPT OAuth, not the Codex executor's
OpenAI API key. The bundled Pi 1.4.0 model catalog includes this model.

Pi is **local worker only** (`isolation: "local"`, also the Pi default).
Explicit sandbox requests and unsupported templates are rejected. Local
execution is unsafe: a resolver can execute repository code and access the
worker filesystem, including persisted identities. Only use trusted repos.
This is not a credential-free sandbox. Reviewer tools are limited to read,
grep, find and ls. Pi does not load extensions, skills, repository settings,
context files or MCP servers for factory runs.

Prompts travel over stdin. Pi JSONL is normalized into the existing result
schema. Incomplete, aborted, failed and nonzero-exit runs are not accepted as
success. Token counts are recorded without claiming a USD API charge for
subscription usage. Existing cancellation and wall-time limits apply; Pi
does not implement Claude's max-turns flag or a subscription USD cost cap.

## Installed on Oracle

- Host: Node 22.23.4 ARM64 under `/home/ubuntu/.local/lib/` (verified against
  the official SHA256 checksum), Pi 1.4.0 under `/home/ubuntu/.local/`.
  `/usr/local/bin/pi` invokes that installation, including in non-login SSH.
- Autonomous worker: Pi 1.4.0 installed with
  `npm install -g --ignore-scripts --prefix /claude-home/.npm-global @earendil-works/pi-coding-agent@1.4.0`.
- Worker prefix and `/claude-home/.pi/agent/auth.json` are on the existing
  persistent PVC. They survive container and pod replacements.
- `AUTONOMOUS_PI_BIN` optionally overrides the worker executable; the Oracle
  manifest sets `/claude-home/.npm-global/bin/pi`. The live worker already
  has this directory on PATH, so installation required no restart.

**The selector/backend changes still need their normal build and deployment.**
Installing Pi alone does not update the running backend or admin.

## Connect ChatGPT (do this in the running container, not just on the host)

The current Oracle deployment has only the `backend` container (verified after
`autonomous-worker` was removed). Pi and its persisted HOME are accessible there.
If the worker sidecar is restored, use its container name instead. Inspect the
live deployment rather than assuming the checked-in manifest matches it:

```bash
ssh oracle 'sudo kubectl -n nexusmind get deployments -o custom-columns=NAME:.metadata.name,CONTAINERS:.spec.template.spec.containers[*].name'
```

1. Open a terminal:

   ```bash
   ssh -t oracle 'sudo kubectl -n nexusmind exec -it deploy/nexusmind-backend -c backend -- /claude-home/.npm-global/bin/pi'
   ```

2. Inside Pi run `/login`, then choose **OpenAI (ChatGPT Plus/Pro)**
   (`openai-codex`). Do not choose the API-key method or the separate
   `OpenAI (ChatGPT subscription)` provider (`openai`); this executor uses
   `openai-codex`.
3. Choose **Device code login (headless)**. Open the displayed URL on your
   own computer, sign in to your ChatGPT account, and enter the displayed
   device code. Follow any account/device-auth enablement instructions shown
   by OpenAI. Your plan/account must permit this access.
4. Wait for Pi to confirm authentication, then exit with Ctrl+C (twice if
   needed). Never paste tokens, auth.json or device codes into chat.
5. Confirm authentication without printing credentials:

   ```bash
   ssh oracle 'sudo kubectl -n nexusmind exec deploy/nexusmind-backend -c backend -- /claude-home/.npm-global/bin/pi auth check --provider openai-codex --json'
   ```

   Expected: `"status":"ready"`. This command may refresh OAuth credentials.
6. Optional smoke test (consumes subscription quota):

   ```bash
   ssh oracle 'sudo kubectl -n nexusmind exec deploy/nexusmind-backend -c backend -- /claude-home/.npm-global/bin/pi --no-session --no-mcp --no-extensions --no-approve --model openai-codex/gpt-6.1-sol --tools read --print "Responde solamente OK"'
   ```

7. After deploying the code, create/edit a compatible agent: **Task executor
   → Pi (ChatGPT account, local worker)**, review the local-execution warning,
   save and run it.

Host `pi` has its own identity (`/home/ubuntu/.pi/agent/auth.json`). Logging
in there does **not** authenticate the Kubernetes worker. Do not copy auth
files into Git or Kubernetes manifests. Use `/logout` inside worker Pi to
remove its saved login; provider-side revocation is separate.

## Verification / session state

- Both installed CLIs report 1.4.0.
- Worker invocation accepts all factory flags and resolves the configured
  model; it currently fails on missing credentials, as expected before login.
- `auth check --provider openai-codex --json`: `not_ready`,
  `credentials_not_configured` before user authorization.
- Admin TypeScript build passed; isolation tests passed (8 tests).
- Rust unit tests added for parsing and configuration validation, not run:
  `cargo` is unavailable in the current development environment.
- No production restart, deployment, commit or credential transfer performed.
- NexusMind MCP tools were unavailable in this session; knowledge could not
  be persisted through `store_memory`. This document records the handoff.
