# Pi task executor — Oracle and ChatGPT

## What changed

`executor: "pi"` supports `qa`, `github_issue_resolver` and
`github_pr_reviewer`. Existing agents keep their executor. The selector always
shows Pi and Codex, disabling unsupported choices with an explanation. Pi QA
requires the QA integration changes in this branch to be deployed; the earlier
installation (#351) and visibility fix (#352) alone do not enable QA.
`TaskExecutorSelect.test.tsx` covers selecting Pi for QA while Codex remains
disabled there. Judge remains unsupported. Pi uses
`openai-codex/gpt-6.1-sol` through ChatGPT OAuth, not the Codex executor's
OpenAI API key. The bundled Pi 1.4.0 model catalog includes this model.

Pi is **local worker only** (`isolation: "local"`, also the Pi default).
Explicit sandbox requests and unsupported templates are rejected. Pi is an
explicit opt-in with its own fixed model, independent of Factory per-role
preferences (those remain unwired). A fresh `auth check` must confirm OAuth
before each run; there is no API-key or Claude fallback. Pi runs can be leased
even when host Claude health is not ready. Local execution is unsafe: a resolver can execute repository code and access the
worker filesystem, including persisted identities. Only use trusted repos.
This is not a credential-free sandbox. Reviewer tools are limited to read,
grep, find and ls. Pi does not load discovered extensions, skills, repository
settings or context files. Only QA loads a worker-owned extension and Playwright
MCP; issue-resolver/reviewer runs still disable all MCP servers.

### QA Playwright integration

- QA gets read/grep/find/ls and `mcp__playwright__*`, never bash/edit/write.
- The Rust binary embeds `pi_qa_extension.ts`, writing it outside the checkout
  alongside a per-run MCP config derived only from `/app/qa-mcp.json`
  (`AUTONOMOUS_QA_MCP_CONFIG` override). Only its `playwright` entry is kept.
- The trusted extension uses Pi 1.4.0 `createMcpExtension({loadConfig})` to
  **replace discovery**, not append to personal/repository MCP servers. Tools
  are exposed directly; codemode auto-activation is off.
- Pi retains its ordinary persisted OAuth store and refresh locks. No OAuth
  token copy, isolated auth-file symlink or global config mutation is needed.
- Screenshots use the worker's existing per-run output directory and fallback
  collection from the checkout. The same evidence upload, evaluation and
  worker-controlled deliveries apply. The model does not publish findings itself.
- Missing/malformed Playwright config blocks the run as `pi_qa_mcp_unavailable`.
  Pi QA is still local, not a network or credential-free sandbox.

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
- Backend images now also bundle Pi 1.4.0 with `--ignore-scripts`, so new
  installations can use `pi` from PATH without a runtime npm download.
- Worker prefix and `/claude-home/.pi/agent/auth.json` are on the existing
  persistent PVC. They survive container and pod replacements.
- `AUTONOMOUS_PI_BIN` optionally overrides the worker executable; the Oracle
  manifest sets `/claude-home/.npm-global/bin/pi`. The live worker already
  has this directory on PATH, so installation required no restart.

**QA support requires its normal build and deployment.** Initial Pi support
(#351) already deployed successfully; installing Pi alone does not update the
running backend or admin.

## Connect ChatGPT (do this in the running container, not just on the host)

Oracle briefly had only `backend`; by the continuation of this session the
`autonomous-worker` sidecar was restored and the pod is 2/2 Ready. Both containers
share the persisted HOME. Prefer `autonomous-worker` when present; if absent,
use `backend` for login. Inspect the live deployment instead of assuming its
container names:

```bash
ssh oracle 'sudo kubectl -n nexusmind get deployments -o custom-columns=NAME:.metadata.name,CONTAINERS:.spec.template.spec.containers[*].name'
```

1. Open a terminal:

   ```bash
   ssh -t oracle 'sudo kubectl -n nexusmind exec -it deploy/nexusmind-backend -c autonomous-worker -- /claude-home/.npm-global/bin/pi'
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
   ssh oracle 'sudo kubectl -n nexusmind exec deploy/nexusmind-backend -c autonomous-worker -- /claude-home/.npm-global/bin/pi auth check --provider openai-codex --json'
   ```

   Expected: `"status":"ready"`. This command may refresh OAuth credentials.
6. Optional smoke test (consumes subscription quota):

   ```bash
   ssh oracle 'sudo kubectl -n nexusmind exec deploy/nexusmind-backend -c autonomous-worker -- /claude-home/.npm-global/bin/pi --no-session --no-mcp --no-extensions --no-approve --model openai-codex/gpt-6.1-sol --tools read --print "Responde solamente OK"'
   ```

7. After deploying the code, create/edit a compatible agent: **Task executor
   → Pi (ChatGPT account, local worker)**, review the local-execution warning,
   save and run it.

Host `pi` has its own identity (`/home/ubuntu/.pi/agent/auth.json`). Logging
in there does **not** authenticate the Kubernetes worker. Do not copy auth
files into Git or Kubernetes manifests. Use `/logout` inside worker Pi to
remove its saved login; provider-side revocation is separate.

## Troubleshoot issue-resolver provider mismatch

`factory_chatgpt_oauth_not_connected` can be returned by the unwired Factory
role-preference gate when the **agent executor is not Pi**, even though Pi OAuth
is ready. The Codex executor uses an OpenAI API key in the sandbox; it does not
use Pi's ChatGPT OAuth. Do not bypass the gate or silently switch providers.

The reported `Kasymir Issues Resolver` failure was confirmed on run
`bac61bec-85e4-46fe-a6eb-1d0c65d5a50b`: revision 5 saved `executor: codex`,
`isolation: sandbox`. OAuth was still ready for `openai-codex` in Pi. To use the
connected ChatGPT account, edit **Task executor → Pi**, save, validate and start
**a new run**. Existing runs retain their revision snapshot. The agent was not
modified or rerun during diagnosis.

## Conversation transcript

Pi stores raw JSONL in `autonomous_agent_run_transcript`, just like Claude,
but its schema differs. The original UI accepted only Claude `assistant` and
`user` events. A live Pi issue-resolver run had 1,842 stored events (1,667 were
`message_update`, plus messages and tool executions), yet showed “No readable
messages yet.” The run was healthy; the renderer did not understand Pi.

`Transcript.tsx` now joins `text_delta` updates by assistant message/content
index, replaces them with authoritative `text_end`/`message_end` content, and
renders each message once. `turn_end` and `agent_end` duplicates are ignored.
Tool executions show their names and errors, while system/user prompts,
reasoning and successful tool-output dumps remain hidden. Existing stored runs
need no rerun or migration. The UI count says **events**, not model turns, and
autoscroll responds to text updates without a new row. Decoding runs once per
message, not once per streaming token. Claude rendering remains unchanged.

## Run logs

The run page previously fetched lifecycle events only once during execution and
refreshed them on termination. Its conversation view intentionally discarded
successful tool outputs, so neither surface provided usable command logs.

The Event log now polls every 2.5 seconds while active, shows expandable lifecycle
payloads and includes per-tool input/output for both Pi and Claude. Pi partial
outputs update the same tool row; completed/failed/incomplete states are distinct.
Long output keeps the newest 16,000 characters rather than freezing on a truncated
prefix. Live following can be disabled, and showing older tool logs disables it.
Only the latest 100 tool rows render initially, with older rows loadable. Binary
image data is omitted; logs use the worker's already-sanitized stored payloads.
Prompts and reasoning are not included. These are **recorded tool/runtime events**,
not a new stream of Kubernetes container stderr. Fetch failures are visible and
retryable instead of looking like empty logs. Existing stored runs need no rerun.
Each finished event uses its own code, and the result card uses the latest finish
when a run has multiple attempts.

## Verification / session state

- Both installed CLIs report 1.4.0.
- User completed login; `auth check` confirms `ready`, `authType: oauth` for
  `openai-codex`. Initial login was to `openai`, which is a separate provider.
- A real worker invocation with all factory flags returned `OK` using
  `gpt-6.1-sol`; JSONL ends with `agent_settled`, `aborted: false`.
- Real QA smoke test on Oracle used the trusted extension, navigated to
  `https://example.com` and took `pi-qa-smoke.png` successfully. Both Playwright
  calls ended without errors; the 69,657-byte screenshot was found in the
  checkout fallback location, which the worker's existing collector covers.
- Initial Admin build and focused tests passed; new QA tests cover option
  selection, isolation, browser-only command/config and independent leasing.
- Rust tests cover parsing, configuration validation, API errors and leasing
  independently of Claude. Local `cargo` is unavailable; CI must validate them.
- Deploy via the normal PR/CI/main workflow. Do not copy or commit credentials.
- NexusMind MCP tools were unavailable in this session; knowledge could not
  be persisted through `store_memory`. This document records the handoff.
