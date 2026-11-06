# Nexus Harness CLI

Guía paso a paso en español: [GUIA_USO.md](GUIA_USO.md).

`nexus` is an interactive coding-agent harness for NexusMind. It owns session
checkpoints, verification, and the completion decision. Model runtimes are
replaceable; OpenShell is mandatory for every agent process and shell command.

## Install and start

```bash
cd apps/nexus-cli
cargo install --path . --locked
cd /path/to/your/git/repository
nexus init --project YOUR_NEXUSMIND_PROJECT --project-id YOUR_REAL_PROJECT_ID
nexus chat
nexus claude
nexus codex
```

`nexus init` installs [NVIDIA OpenShell](https://docs.nvidia.com/openshell/latest/about/installation)
with NVIDIA's official installer if the CLI is absent. On macOS that uses
Homebrew and starts the local gateway. Later commands also install OpenShell
automatically if needed. No local-shell fallback exists. The first command that
needs a sandbox pulls the OpenShell `base` image; this can take a few minutes.
`nexus status` is read-only and never installs anything.
Init also creates a schema-v1 `.nexusmind.yaml` with the `essential` profile if
it does not exist; an existing file is never overwritten. Omit `--project-id`
only if you will review the generated `project_id` before ID-dependent workflows.
Nexus can also run from a parent directory containing multiple direct child
Git repositories. It uploads files selected independently by each child's Git
ignore rules, excludes loose parent-directory files, and only synchronizes
changes back inside those child repositories. Use `--repository PATH` to scope
the session to one child instead.

OpenShell's default base image contains Claude Code and Codex, but not every
language toolchain. For a Rust project (a root `Cargo.toml` or this repo's
`apps/backend/Cargo.toml`), Nexus automatically builds a Rust-capable image
on first use with local Docker. This image includes narrow OpenShell network
allowlists for Cargo's registry and the ONNX Runtime build download used by
NexusMind's backend. Other projects can set
`NEXUS_OPENSHELL_IMAGE` to a suitable existing image. Nexus uses a separate
sandbox name per repository, runtime, and image. A configured existing sandbox
can be selected with `NEXUS_OPENSHELL_SANDBOX`.

If a managed sandbox enters OpenShell's `Error` phase, Nexus preserves it and
uses a stable recovery sandbox (`r1`, then `r2`/`r3`). It never deletes the old
workspace. Inspect it with `openshell sandbox get NAME --output json` and
`openshell logs NAME` before deciding whether to remove it. A sandbox selected
explicitly with `NEXUS_OPENSHELL_SANDBOX` is never replaced automatically.

## Sign-in and optional integrations

The default Claude Code and Codex CLI runtimes do not require API keys.
`nexus codex` reuses the host Codex ChatGPT login through an OpenShell
credential provider. Run `codex login` once if needed. `nexus claude` uses
Claude Code's account login inside its OpenShell sandbox; type `/login` in
the Nexus TUI once. The host's macOS executable cannot run in a Linux
sandbox, so OpenShell's installed CLI binaries do the agent work. Check either
login without running a task with `nexus auth codex` or `nexus auth claude`.

Jev and NexusMind are optional. If you configure them, set keys in your
terminal environment, not in a prompt, repo file, or session:

```bash
export TYPESAFE_API_KEY='your-jev-key'
export NEXUSMIND_BASE_URL='https://your-nexusmind-server'
export NEXUSMIND_API_KEY='your-nexusmind-key'
```

`TYPESAFE_API_KEY` is the Jev key from TypeSafe. `JEV_API_KEY` is accepted as
an alias. In the default `auto` mode, Nexus uses Jev when configured and
otherwise uses explicitly labeled local decisions. `--decision-engine jev`
requires a Jev key; `--decision-engine local` always uses local rules.
NexusMind context is optional unless
`NEXUSMIND_REQUIRED=1`; set both NexusMind variables to enable it.

The optional direct API runtimes require their respective API keys and an explicit model:
`NEXUS_CLAUDE_MODEL` for `claude-api`, or `NEXUS_OPENAI_MODEL` for
`openai-api`. Direct API runtimes
call the model from the host but execute every tool only in OpenShell. Do not
put API keys in `NEXUS_OPENSHELL_IMAGE` or sandbox environment flags.

## Daily use

```bash
nexus chat
nexus claude "Fix the authentication redirect"
nexus codex "Fix the authentication redirect"
nexus --runtime codex-headless chat
nexus --runtime claude-api plan "Plan the migration"
nexus run "Fix the authentication redirect"
nexus resume SESSION_UUID
nexus verify SESSION_UUID
nexus logs SESSION_UUID
nexus shell
```

`nexus chat`, `nexus claude`, `nexus codex`, and `nexus resume` open a two-pane
terminal UI. The conversation is on the left; commands, tool output, sandbox
sync, verification, and Jev progress are on the right. Type a task to run an
agent turn. Up/Down scroll the activity pane. `nexus chat --plain` and
`nexus resume SESSION_UUID --plain` retain the line-oriented fallback.
`/plan TASK` is read-only; `/login` starts the account sign-in flow;
`/runtime ID` switches runtime; `/verify` reruns checks; `/status` and
`/logs` inspect the session; `/shell` opens a real interactive TTY inside
OpenShell; `!COMMAND` runs a one-shot command there; `/finish` records your
manual acceptance; `/exit` leaves the UI.

`NEXUS_VERIFY_COMMAND` overrides automatic `cargo test`/`npm test`. In this
NexusMind checkout, the backend's automatic command limits Cargo to one build
job and skips the one test that requires the host Git history, which is not
uploaded into OpenShell.
`NEXUS_ACCEPTANCE_COMMAND` defines a task-specific acceptance check. A
generic test pass is not enough to prove the objective; without task-specific
evidence, Jev cannot automatically mark the task finished. Set
`NEXUS_MAX_TASK_COST_USD` to cap Claude Code headless task cost.

## File and decision safety

The host repository is never mounted into OpenShell. Nexus sends a filtered
snapshot of Git-tracked and non-ignored untracked files, excluding `.git`,
`.nexus`, common build directories, and `.env*` files. The sandbox gets its
own Git baseline. After a turn, new, modified, and deleted project files are
synchronized back; a concurrent host edit aborts rather than being overwritten.
If synchronization fails, a recovery download is retained in the temporary
directory and the sandbox remains available. Git-ignored files are not sent.

Session checkpoints live under `.nexus/sessions/` with private file
permissions. `nexus init` adds a nested `.nexus/.gitignore` so these files stay
out of Git. The Jev request
contains compact task evidence, not the full transcript, API keys, or local
repository path. The completion judge combines Jev with hard evidence gates;
an API error is never presented as a successful Jev decision.

## Verification

```bash
cd apps/nexus-cli
cargo test
cargo clippy -- -D warnings
```

Local tests use mock HTTP responses for Jev and do not require credentials.
Live Claude, OpenAI, Jev, and NexusMind calls still require valid keys and
reachable services.
