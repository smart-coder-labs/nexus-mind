# Design — Factory F0: Foundation

This revision details **item 1 (merge gate)**. Items 2–8 get their design sections before they are implemented.

## 1. Merge gate

### Current flow (`automation/worker.rs`)

1. `execute_claim` → `bounded_review_diff` reviews the diff at the worktree HEAD.
2. `publish_template_output` (`github_pr_reviewer`): if `trigger.head_sha` exists and the current head differs → `stale_pull_request_head`. It then posts the review.
3. If `config.auto_merge == true` and there are no medium+ findings → `auto_merge_pull`.
4. `auto_merge_pull` re-fetches the PR, checks CI on the *current* head, then calls `merge_github_pull` **without a SHA**.

There are two windows between steps 2 and 4 where unreviewed commits can be merged. Manual runs have no `trigger.head_sha` and skip the stale guard entirely.

### Changes

- **Reviewed SHA.** Auto-merge requires `trigger.head_sha`. Without it → decline `reviewed_head_unknown`. Webhook-triggered reviews always carry it.
- **Head check.** In `auto_merge_pull`, after fetching the PR: if `head.sha != reviewed` → decline `head_changed_since_review`. CI is checked on the reviewed SHA.
- **Pinned merge.** `merge_github_pull` gains `sha: Option<&str>` and sends `{"merge_method", "sha"}`. GitHub returns 409 when the head no longer matches, which surfaces as `merge_rejected`.
- **Path allowlist.** A new connector `list_github_pull_files`, paginated at 100 per page with a cap of 300 files (more → decline `too_many_files`). A pure function `auto_merge_path_verdict(&[ChangedFile]) -> Result<(), String>`:
  - `removed` → ineligible.
  - `renamed` → both `filename` and `previous_filename` must be eligible.
  - Eligible when the path matches one of:
    - Docs: `*.md` anywhere. Under `docs/`, also prose and images (`.txt .rst .adoc .png .jpg .jpeg .gif .webp .svg`). Nothing else under `docs/` counts, because `docs/conf.py` or a site config is executable in a docs build. `.mdx` is never docs: it compiles to JS and can be a live route (e.g. Next.js `app/**/page.mdx`).
    - Tests: any directory segment `tests`, `test`, `__tests__`, `e2e`; file names `*.test.*`, `*.spec.*`, `*_test.go`, `*_test.rs`, `test_*.py`, `*_test.py`.
  - Explicitly **never** eligible, even under a docs or test path. Matching is case-insensitive:
    - Any dot-directory or dotfile segment: `.github`, `.claude`, `.cursor`, `.npmrc`, `.mcp.json`, `.vitepress`…
    - Agent instructions: `claude*.md`, `agents.md`, `gemini.md`, `copilot-instructions.md`, `skill.md`, plus any `.md` under a directory named `agents`, `commands`, `skills`, `prompts`, `hooks`, `rules` or `plugins` (plugin layouts keep instructions in ordinary directories).
    - Dependency and build manifests across ecosystems: lockfiles, `package.json`, `pnpm-workspace.yaml`, `go.mod/go.sum`, `Gemfile*`, `pom.xml`, `*.gradle(.kts)`, `*.csproj`, `requirements*.txt`, `setup.py/cfg`, `conftest.py`, `Makefile`, `Dockerfile*`, `Containerfile*`, `*.toml`, `*.lock`, `*.sh`.
    - `openspec/config.yaml`.

  Agent instructions and dependency manifests change what later runs do, so they are never "just docs" or "just tests".

- **CI signals.** `list_commit_ci_runs` reads **every** check-run page (capped at 1000) plus the combined commit status (Vercel, CircleCI and other non-Checks integrations, converted to check-run shape by `statuses_as_runs`). Any unreadable page or overflow declines with `checks_unavailable` instead of judging a partial list.
- **Required checks.** Every configured `required_checks` name must be present on the reviewed commit and green. A missing required check declines with `required_check_missing:<name>`. Before this change, a configured check that had not reported was silently ignored. With none configured, every reported run must be green.

- **Order of checks** in `auto_merge_pull`, cheapest and most decisive first: already merged → reviewed SHA known → head matches → paths eligible → CI green → publish authority → pinned merge.

### Why hard-coded patterns

The allowlist is the interim floor (D10). The configurable version is `ActionPolicy` (item 3), which is control-plane owned. The run config (`claim.config`) is editable by `autonomous_agent:update` holders and must not be able to widen merge eligibility, so there is deliberately no config override.

### Not in item 1

The soak window (D17) needs a delayed re-check, which the current one-shot worker cannot do. It lands with the per-action policy engine (item 4). Until then the head-pinned merge is the guard against late pushes.

## 2. Contracts

### Layout (follows the existing `nexusmind-config-v1` convention)

```
schemas/factory/{name}-v1.schema.json            JSON Schema 2020-12
schemas/fixtures/factory/{name}/v1/valid/*.json
schemas/fixtures/factory/{name}/v1/invalid/*.json
schemas/fixtures/factory/cases.json               cross-language contract: fixture -> valid?
apps/backend/src/factory/contracts.rs             hand-written serde types
```

The names are `task-spec`, `routing-decision`, `context-pack`, `code-change-proposal`, `verification-report` and `action-policy`.

### Conformance: two validators must agree

Today only the Rust parser checks `nexusmind-config-v1`; the schema file is never executed, so the two can drift unnoticed. For the factory contracts, one test runs every fixture in `cases.json` through **both**:

1. the JSON Schema, using the `jsonschema` crate as a **dev-dependency** only;
2. `serde_json::from_str::<T>` with `deny_unknown_fields`.

Both must match the expected validity. For valid fixtures, the test also checks a round trip: `serialize(deserialize(x))` must deserialize to an equal value.

### Shared rules

- Every contract has `"schema_version": 1` (a `const`). A breaking change means a new `-v2` file, never a silent reinterpretation.
- `additionalProperties: false` everywhere, mirrored by `#[serde(deny_unknown_fields)]`.
- No `org_id` in payloads. Tenancy comes from the authenticated context, never from contract data (the same principle as `policy.rs`: worker or repository input never fills authority fields).
- Every enum is closed. Unknown values are rejected, not mapped to a default.

### Contracts

| Contract | Key fields |
|---|---|
| `TaskSpec` | `task_id` (uuid) · `source {kind, ref, url?}` · `origin_trust` (`trusted`/`untrusted`) · `privacy_class` · `repository {remote, base_ref}` · `title` · `description` · `task_class` · `requirements[]` · `acceptance_criteria[]` · `created_at` |
| `RoutingDecision` | Per the architecture doc (`task_type`, `execution_tier`, `risk`, `confidence`, `blast_radius`, `risk_factors`, `required_context`, `required_checks`, `max_attempts`, `human_approval_required`), plus `task_id`, `decided_by {provider, model?}` and `actions{}`: a per-action verdict `{verdict: allow/hold/deny, reason, source: policy/floor/decision_model/human}` (D12) |
| `ContextPack` | Per the doc: `task_id` · `repository {commit, branch?}` · `artifacts[] {path, symbol?, kind, reason, content_hash, retrieval_score?}` · `constraints[]` · `acceptance_tests[]` |
| `CodeChangeProposal` | `task_id` · `summary` · `operations[] {path, action: create/modify/delete, symbol?, rationale}` · `assumptions[]` · `verification_plan[]` · `needs_more_context` · `requires_human_approval` |
| `VerificationReport` | `task_id` · `head_sha` (40-hex, the verified commit) · `passed` · `checks[] {name, status: PASS/FAIL/SKIP/ERROR, duration_ms?, artifact?}` · `blocking_failures[]` · `eligible_for_merge` · `human_approval_required` |
| `ActionPolicy` | `action` · `mode` (`never`/`manual`/`criteria`/`after_fix`/`after_merge`) · `scope {project?, task_class?}` · `allow[]` · `stop[]` · `version` |

**Closed enums:**

- `source.kind`: `github_issue`, `nexusmind_task`, `slack`, `sentry`, `gmail`, `transcript`, `manual`.
- `task_class`: `docs`, `tests`, `ui`, `backend`, `bugfix`, `refactor`, `migration`, `infra`, `security`, `unknown`.
- `privacy_class`: `public`, `internal`, `confidential`, `restricted`.
- `action`: `fix`, `publish`, `open_pr`, `reply`, `close`, `approve`, `merge`, `deploy`, `notify`, `recover`.
- `execution_tier`: `DETERMINISTIC`, `DECISION_MODEL`, `LOCAL_SMALL`, `CHEAP_CLOUD`, `FRONTIER`.

**Cross-field rules** (in the schema where JSON Schema can express them, and in a Rust `validate()` otherwise):

- `origin_trust` must be `untrusted` for `gmail`, `transcript`, `slack` and `sentry` sources. An external author can never be marked trusted.
- `github_issue` may be either, because a contract cannot know whether the repository is public. **Requirement for the GitHub intake adapter (item 5.2):** set `trusted` only when the issue author's `author_association` is `OWNER`, `MEMBER` or `COLLABORATOR`; everything else is `untrusted`.
- Integers accept a zero fraction (`2.0`), exactly as JSON Schema's `integer` does, so both validators agree. Fractional values are rejected.
- `ActionPolicy` with `mode: criteria` requires a non-empty `allow`.
- `VerificationReport.eligible_for_merge` implies `passed` and an empty `blocking_failures`.

### Not in item 2

Persisting these contracts (tables) belongs to items 3–6. Item 2 only defines and verifies the wire contracts.

## 3. Policy storage and admin

### Table (migration v78)

```sql
factory_action_policies(
  id TEXT PRIMARY KEY,
  org_id TEXT NOT NULL REFERENCES organizations(id),
  action TEXT NOT NULL CHECK (action IN (...10 actions...)),
  mode TEXT NOT NULL CHECK (mode IN ('never','manual','criteria','after_fix','after_merge')),
  scope_project TEXT NOT NULL DEFAULT '',      -- '' = any project
  scope_task_class TEXT NOT NULL DEFAULT '',   -- '' = any class
  allow_json TEXT NOT NULL, stop_json TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version >= 1),
  updated_by TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
  UNIQUE (org_id, action, scope_project, scope_task_class)
)
```

`''` rather than `NULL` for "any": SQLite treats NULLs as distinct in `UNIQUE`, which would allow two "any project" rows for the same action.

### Permissions

- `factory_policy:read` and `factory_policy:write` go through `require_explicit_permission` and resolve against the **persisted role template**, the same mechanism `autonomous_agent:*` already uses. The legacy privileged-role bypass does not apply, so an `admin` is denied unless the grant exists (D16: permission, not role).
- Migration v78 grants both only to `super_user_template`. They are added to the hard-coded `super_user` list, which is what `/v1/admin/auth/me` reports, and deliberately **not** to the `admin` list.
- There is no MCP tool (D16). Only the admin UI edits policies.

### API

| Method | Path | Permission | Behavior |
|---|---|---|---|
| GET | `/v1/factory/policies` | `factory_policy:read` | List the org's policies as `ActionPolicy` + `id` |
| PUT | `/v1/factory/policies` | `factory_policy:write` | Upsert by `(action, scope)`. Body is an `ActionPolicy` (validated with `Contract::validate`). **Optimistic concurrency:** `version` must be `1` for a new row and `current + 1` for an existing one, otherwise 409 `policy_version_conflict` |
| DELETE | `/v1/factory/policies/:id` | `factory_policy:write` | Remove one policy |

Every write is recorded with `log_audit` (`factory_policy.upsert` / `factory_policy.delete`), including the before/after mode.

### Absence of a policy

No row for an action means **`manual`** (hold for a human). Evaluation (item 4) fails closed, so an org with no policies gets no autonomy.

### Admin UI

A new page, `FactoryPolicies` at `/factory-policies`, gated by `factory_policy:read`. It shows one row per policy and an editor for action, mode, scope, allow and stop lines; saving requires `factory_policy:write`.

The page states the states explicitly:

- **Empty:** "No policies — every action waits for a person."
- **409:** "Someone changed this policy; reload."
- **No write permission:** read-only.

## 4. Policy evaluation

### Engine (`src/factory/policy_engine.rs`, pure)

`evaluate(input, policies, model) -> ActionVerdict` in a fixed order. The first rule that decides wins:

1. **Select the policy:** the most specific match for `(action, project, task_class)`, in this order: exact project + class → project only → class only → any. If there is no match, the mode is `manual`. When the item's project is unknown (autonomous runs do not carry one today) and a project-scoped policy exists for the action → `hold` with reason `project_unresolved`: the engine never falls back past a policy that might be the one that applies.
2. **`never`** → `deny` (source `policy`). **`manual`** → `hold` (source `policy`).
3. **Floors** (deterministic; any hit → `hold`, source `floor`). The caller computes them:
   - a sensitive path is touched (the `merge_gate` never-eligible rules, plus paths outside docs/tests for merge);
   - a blocking verification failure;
   - a second failed repair attempt;
   - an external source (`gmail`, `transcript`, `slack`, `sentry`) combined with `fix`.
4. **`after_fix` / `after_merge`** → `allow` (source `policy`) only when the milestone is reached; otherwise `hold`. These are deterministic facts, not model judgments.
5. **`criteria`** → the decision model (`DecisionProvider`):
   - **not configured** → `hold` with reason `decision_model_not_configured` (decision above: an unevaluated written condition is never treated as met);
   - error, timeout or schema violation → `hold` with reason `decision_model_failed`;
   - `allow` with confidence ≥ threshold (default 0.8) → `allow` (source `decision_model`);
   - otherwise → `hold` (source `decision_model`).

The model can only turn an eligible item into `allow`, or keep it held. It is never consulted for items that steps 1–4 already decided.

### Merge soak (D17)

When every gate passes for `merge`, the merge does **not** happen yet:

- A row in `factory_merge_soaks` records `(org_id, run_id, repository, pull_number, head_sha, required_checks, due_at = now + 10 min)`, and the verdict is `hold` with reason `soak_pending`.
- Every new review of the PR **restarts** the soak with its own run and required checks when it again ends in `allow`, and **cancels** it on any other outcome (blocking findings, a failed gate, a `hold`). A soak therefore always reflects the latest verdict, never an outdated one.
- The worker tick runs `process_due_soaks`. For each row that is due, it re-runs the **full** gate: PR head still equals `head_sha`, paths, CI (every page plus statuses), policy, model and publish authority. It then merges pinned to `head_sha`.
- Any change or failure deletes the row with a recorded decision. A new push is reviewed again by the normal webhook flow.

### Decision audit (`factory_decisions`)

Every evaluation writes one row:

- `org_id`, `subject` (e.g. `acme/web#42@<sha>`), `action`;
- `verdict`, `source`, `reason`;
- `policy_id` and `policy_version`;
- `provider`, `model`, `confidence`;
- `inputs_json`: floors hit, task class and source kind. It never includes diff content or secrets.

This is the dataset for the *false-low-risk* metric (plan §4 step 5).

### Integration in F0

`auto_merge_pull` keeps the item-1 gates, then asks the engine for `merge`. Since no decision model exists until F3, auto-merge now always ends in `hold` (`decision_model_not_configured`, or `manual` when no policy exists), and the reason is visible in the run result. The soak and decision audit are exercised by tests with a fake provider, ready for F3.

### Known effects (item 4)

- **Judge chaining.** `maybe_trigger_next_agent` chains the Judge only when the reviewer run itself reports `merged: true`. Merges now happen later, in the worker tick after the soak, so the Judge is not chained automatically. Re-attaching it to `merge_after_soak` belongs to F3, when merges can actually be allowed.
- **Soak failures fail closed.** An error while re-checking a due soak (network, token) ends the soak without merging and logs it. The PR stays open for a person or for the next review.
- **Existing `auto_merge: true` agents stop merging.** With no merge policy the verdict is `no_policy_defaults_to_manual`; with a `criteria` policy it is `decision_model_not_configured` until F3. The reason is returned in the run result and recorded in `factory_decisions`.

## 5. Intake

### Scope in F0

Intake **normalizes** external work into validated `TaskSpec`s. It does not persist them or route them: the router (F3) consumes them. Slack/Sentry (F3) and Gmail/transcripts (F4) plug into the same trait.

### Contract (`src/factory/intake.rs`)

```rust
#[async_trait]
pub trait IntakeSource {
    fn kind(&self) -> SourceKind;
    async fn fetch(&self) -> anyhow::Result<Vec<TaskSpec>>;
}
```

Every adapter is a thin fetcher over a **pure normalizer**, and the normalizer is what the tests cover:

- `github_issue_to_task_spec(issue, target, now) -> Result<Option<TaskSpec>, String>`
- `nexusmind_task_to_task_spec(task, target, now) -> Result<Option<TaskSpec>, String>`

`IntakeTarget { repository, base_ref, privacy_class }` comes from the control-plane configuration of the source, never from the item. NexusMind projects have no repository field, so the target says which repository a project's tasks apply to.

### Rules

- **Opt-in by label** (assumption, reversible): only items labeled `factory` (any case, in both sources) enter, the same pattern as the Gmail decision. `Ok(None)` means "not for the factory"; `Err` means the item is malformed.
- **Only unstarted work:** GitHub asks for open issues; NexusMind tasks must be `backlog` or `todo`. A task someone is already working on (`in_progress`, `in_review`) or that is closed never enters. The NexusMind fetch paginates the whole project and filters in code, so closed tasks cannot crowd out open ones.
- Pull requests returned by the issues API are skipped.
- **Trust.**
  - GitHub issue: `trusted` only when `author_association` ∈ {`OWNER`, `MEMBER`, `COLLABORATOR`}; otherwise `untrusted` (design §2).
  - NexusMind task: `trusted`, because it was created by an authenticated org member.
- **Stable identity.** `task_id` is a name-based UUID derived from the source reference (`github_issue:acme/web#42`), so re-fetching the same item yields the same id. The router can deduplicate without storage.
- **Task class** is derived deterministically from labels. The most sensitive class wins when labels conflict: `security` > `migration` > `infra` > `bugfix` > `backend` > `ui` > `tests` > `docs`. With no known label the class is `unknown`, and the router treats that as needing more context.
- **Acceptance criteria** are the Markdown checklist items (`- [ ] …`) of the description.
- Every produced `TaskSpec` passes `Contract::validate()`. The description is truncated to the contract's 64 KiB.

## 6. Telemetry

### What exists

Autonomous runs parse Claude Code's final `result` event only to enforce `max_cost_usd`. Nothing is stored. `usage_events.task_id` references NexusMind tasks (a foreign key), not factory task ids, so it cannot carry factory runs.

### Table `factory_run_metrics` (migration v80)

One row per finished autonomous run (`UNIQUE(run_id)`) with these columns:

- `org_id`, `run_id`, `template_key`;
- `task_id`: the factory `TaskSpec` id; nullable until the router (F3) assigns work by task;
- `subject`: e.g. `acme/web#42`, from the run trigger;
- `provider`, `model`;
- `input_tokens`, `cached_input_tokens`, `cache_write_tokens`, `output_tokens` — the architecture doc asks for cache accounting separately, because it changes the economics;
- `cost_usd`, `duration_ms`, `num_turns`, `outcome`, `created_at`.

### Extraction

`extract_run_metrics(result: &Value) -> RunMetrics` is a pure function over Claude Code's `result` event:

- `total_cost_usd`, `duration_ms`, `num_turns`;
- `usage.{input_tokens, cache_read_input_tokens, cache_creation_input_tokens, output_tokens}`;
- the model is the `modelUsage` entry with the highest cost.

Every field is optional. A missing field is stored as `NULL`, never as `0`, so an unknown cost is never reported as free.

### Recording

The worker records metrics once the run has an outcome, including `budget_exhausted` and failures that still produced a result event. It is best-effort: a metrics write failure is logged and never changes the run's outcome.

### Not in F0

*Cost per accepted change* joins these rows with verification and human outcomes; that needs the router's task ids (F3) and the trajectory store (F5). F0 guarantees the raw data exists from now on.

## 8. Golden tasks

### Where the data lives (decision)

`scripts/factory/harvest_golden_tasks.py` lives in the repo and contains **no data**. It writes to `~/.nexusmind/evals/golden/v1/` (override with `--out`), outside git.

- kasymir is a private client repository and nexusmind is going open source, so no client text can reach the repo.
- An agent working in a nexusmind checkout cannot read the answers.

### What a golden task is

One merged PR becomes one JSONL record:

- `id`: a name-based id of `repo#number`, the same scheme as intake;
- `repository`, `pr_number`, `title`;
- `task_text`: the body of the closing issue when there is one, otherwise the PR body. This is the ticket as a person wrote it;
- `base_sha`: where the agent starts; `merge_sha`: the reference answer, never shown to the agent;
- `changed_files` and `task_class`, from labels first, then a path heuristic;
- `acceptance`: an empty list. Filling it with the per-task verification commands is a curation step, not automatic.

### Selection

The harvest samples merged PRs, newest first, and skips:

- bot authors (dependabot, renovate, github-actions);
- PRs touching more than 40 files;
- PRs with no ticket text;
- reverts.

The defaults are 30 from `smart-coder-labs/nexus-mind` and 20 from `kasymir/kasymir-app-ui`, which gives 50 (plan F0). Repositories and counts are flags.

### Replay rule

Replaying a task clones the repository fresh at `base_sha` with `--depth 1`. A full clone contains `merge_sha`, which is the answer.

### Tests

The selection and normalization functions are pure and covered by `scripts/factory/test_harvest_golden_tasks.py` (stdlib `unittest`). The `gh` calls are a thin shell around them.

## 7. Jev spike

### What the public docs establish (https://docs.typesafe.ai/api.md, read 2026-09-29)

- `POST https://api.typesafe.ai/v1/systemone`, `Authorization: Bearer <key>`, body `{model, state, questions}`.
- **Jev answers typed questions, not free-form JSON:**
  - `noul` → a probability in 0–1;
  - `choice` → the chosen option, with `probabilities` and `confidence`;
  - `score` → a value on a 2–10 level rubric, with `probabilities` and `confidence`.
- The response carries `usage.input_tokens` / `output_tokens`. Errors are 401, 422, 429 (back off) and 529 (overloaded).

### Consequence for F3 (design change)

The architecture doc imagines Jev emitting a `RoutingDecision` JSON. It cannot. The Rust `DecisionProvider` will instead ask **one call with several questions** and assemble the contract itself:

- `task_class` → `choice`;
- `risk` → `score` on a 4-level rubric;
- `needs_human` → `noul`.

The engine's `ModelDecision::Allow { confidence }` maps from `needs_human < threshold` together with the score confidence. The mapping and its thresholds are decided in F3, calibrated with the spike data.

### The spike (`scripts/factory/jev_spike.py`)

- Sends only **12 synthetic tasks**, one or more per class, 5 of them high-risk: auth, MFA bypass, a destructive migration, payment rounding, CI. No client or repository data is ever sent to the third party.
- Measures p50/p95 latency, mean input tokens and cost at the published $0.042/MTok, class accuracy, schema violations, and **false-low-risk**: a high-risk task where Jev says no person is needed.
- The key comes from `JEV_API_KEY` and is never printed. The report is written outside the repository.
- The pure parts (request building, parsing, stats) are covered by `test_jev_spike.py`.

### Pending

The operator runs it (the agent sandbox has no key). Findings go into `docs/factory/jev-spike.md` and decide F3's thresholds.
