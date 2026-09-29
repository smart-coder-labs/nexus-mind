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
