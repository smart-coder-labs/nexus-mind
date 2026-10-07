# Specialist eval (factory F4)

F4's exit criterion is that **each specialist beats the baseline on a frozen eval**. The baseline is today's generic issue resolver. `factory-specialist-eval` runs that comparison. The code is in `apps/backend/src/automation/specialist_eval.rs`, and the specialists themselves are in `apps/backend/src/factory/specialists.rs`.

## What runs

Every task runs twice from the same `base_sha`, both times in the sandbox (task pods behind the egress proxy, as in production):

| Arm | Steps | Models |
|---|---|---|
| `baseline` | The resolver's fixed prompt, one invocation | Standard tier (`sonnet`) |
| `specialist` | Plan, then implement, then the specialist's checks, then review | Plan and review on the frontier tier (`opus`); the docs specialist implements on the cheap tier (`haiku`), the tests specialist on the standard tier (`sonnet`) |

Each specialist step is a separate Claude Code invocation with its own `--model`. In production every step records its own `model.selected` event, so the daily frontier cap (`FACTORY_FRONTIER_RUNS_PER_DAY`, 20 by default) counts the plan and review steps. The eval records no events: it is not an org's production traffic, and it must not use up the org's frontier cap.

## When an attempt is accepted

An attempt is accepted only if all of these hold:

1. It produced a change. A no-op answer or an empty diff is not accepted. `PENDING.md` is ignored, as it is at publish time.
2. The change passes the merge gate's path and risk floor (`merge_gate::auto_merge_path_verdict`), with the agent's proposed PR title.
3. The change passes the specialist's deterministic checks. **These checks apply to both arms**, so the baseline is held to the same bar. For docs, the checks are:
   - `doc_paths_only`: every changed path is documentation (`merge_gate::is_doc_path`) and is not a never-eligible file such as `CLAUDE.md`, skills or dotfiles.
   - `links_resolve`: relative links and `#anchor`s added by the change point at files and headings that exist. External URLs are not fetched.
   - `identifiers_exist`: code names the change adds in backticks exist in the repository's non-documentation files. A name counts if it is snake_case, SCREAMING_CASE or camelCase, or if it is a file path. Plain words, flags, routes and code samples are skipped.

   For tests (`apps/backend/src/automation/specialist_tests.rs`, `apps/backend/src/factory/mutation.rs`), the checks are:
   - `test_paths_only`: every changed path, on both sides of a rename, is a JavaScript/TypeScript test file (`*.test.*`, `*.spec.*` or under `__tests__/`, with a `ts`, `tsx`, `js`, `jsx`, `mjs` or `cjs` extension) and is not never-eligible. At least one test file is added or changed, and none is removed.
   - `tests_pass`: in one sandbox commands pod, each changed test's package (nearest `package.json`) is installed from its lockfile (`package-lock.json` with `npm ci`, `pnpm-lock.yaml` with `pnpm install --frozen-lockfile`, `yarn.lock` with `yarn install --frozen-lockfile`), and the changed tests run with the package's runner (vitest or jest, from its `test` script or dependencies). They must exit 0.
   - `mutation_score`: the same tests rerun against up to 10 deterministic mutants of the source files they import through relative imports (one flipped `===`/`!==`, `==`/`!=`, `&&`/`||`, `true`/`false`, or a spaced `<`, `>`, `<=`, `>=`; strings, comments and regexes are skipped). With named imports, only the imported declarations are mutated. At least 60% of the mutants must make a test fail. When there is nothing to mutate, the check is advisory and the review judges the tests.
   - `seeded_fault_caught` (eval only): the same tests rerun with each of the task's `seeded_faults` applied, and must fail on every one.

   These checks run only when `test_paths_only` passes, and only in a sandbox: the worker never runs repository code, and without a sandbox the generic resolver keeps the task.
4. On the specialist arm, the specialist's own review accepted the change.
5. An independent Opus judge accepted it. The judge reads the checkout, the task, its acceptance criteria and the diff, and answers `{"verdict":"accept|reject","reasons":[…]}`. Anything else counts as a reject.

The judge runs only when steps 1 to 4 pass, so a failing change costs no judge call.

## Task file format

The task file is JSONL, one task per line. Eval data lives **outside the repository**, for example in `../.evals/specialists/docs-v1.jsonl`.

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Lowercase letters, digits and `-`, at most 32 characters. It becomes part of run ids and pod labels |
| `repository` | yes | `owner/name` |
| `base_sha` | yes | The full 40-hex commit both arms start from |
| `task_class` | yes | A `TaskClass` (`docs`, …). It selects the specialist and becomes the issue label |
| `title`, `description` | yes | The issue both arms see |
| `acceptance_criteria` | no | A list of strings, appended to the issue body and given to the judge |
| `merge_sha` | no | The merged reference answer, when the task was mined from a PR |
| `changed_files` | no | The reference change's files: a hint for the judge, never a gate |
| `seeded_faults` | tests: yes (1 to 3); others: no | `[{"path", "find", "replace"}]`: a fault planted in a source file (never a test file). `find` must occur exactly once in `path` at `base_sha`. Both arms' tests must catch every fault. The agents never see it |

Example:

```json
{"id":"docs-01","repository":"smart-coder-labs/nexus-mind","base_sha":"fd3fb4077251d6c0f7910c92f5ebc7a59456d15e","task_class":"docs","title":"Document the factory operator API","description":"Write docs/factory/operator-api.md describing …","acceptance_criteria":["every endpoint, field and error code matches apps/backend/src/api/factory.rs"]}
```

A tests task:

```json
{"id":"tests-01","repository":"smart-coder-labs/nexus-mind","base_sha":"3fed74e9c9967e78d047f068c0dc173275ba8d9d","task_class":"tests","title":"Add unit tests for the graph type-filter toggle","description":"… add vitest unit tests for `toggleNodeType` …","acceptance_criteria":["toggling the only remaining visible type restores every type"],"seeded_faults":[{"path":"apps/admin/src/pages/graph/graphChrome.ts","find":"currentSet.size === 1 && currentSet.has(type)","replace":"currentSet.size === 1 || currentSet.has(type)"}]}
```

## Building the frozen set

Aim for about 10 real docs tasks per specialist. There are two sources:

**1. Docs-only PRs from nexus-mind's merged history.** These tasks have a reference answer.

```bash
gh pr list -R smart-coder-labs/nexus-mind --state merged --limit 400 \
  --json number,title,body,mergeCommit,files \
| jq -c '.[]
    | select(all(.files[].path; test("\\.md$") or (startswith("docs/") and test("\\.(txt|rst|adoc|png|jpe?g|gif|webp|svg)$"))))
    | {number, title, merge_sha: .mergeCommit.oid, changed_files: [.files[].path]}'
```

Then, for each candidate:

1. Drop PRs that touch agent instructions (`CLAUDE.md`, `AGENTS.md`, `skills/`, `agents/`, …), changelogs and pure typo fixes. They are not docs work the factory should do.
2. Set `base_sha` to the merge commit's first parent: `git rev-parse <merge_sha>^1`.
3. Write `title` and `description` the way the original issue would have been written: what to document, not how the PR did it. Never paste the PR's text into the task.
4. Add `acceptance_criteria` that a reviewer can check against the code.

**2. "Document this existing code" tasks at one pinned commit.** These have no reference answer. Use them when the history has too few docs-only PRs. `docs-v1` is built this way: 10 tasks at `fd3fb40`. Each criterion names the code it must match (for example "routes match `factory/egress.rs`"), so the judge can verify it.

**Tests tasks** ask for tests of existing, untested behavior at one pinned commit. `tests-v1` has 8 tasks on `apps/admin` (vitest) at `3fed74e`. Each has one seeded fault that a good test of the described behavior catches: flip a condition, break a boundary or a case the acceptance criteria name. Check by hand that `find` occurs exactly once at `base_sha`; the eval fails the attempt with `seeded_fault_not_found` or `seeded_fault_ambiguous` otherwise.

**Freezing.** Once the first comparison has run, the file never changes. A new task set is a new file (`docs-v2.jsonl`), and results are only compared within one file.

## Running

The eval runs inside the worker container, which has the sandbox ServiceAccount, the proxy signing key, the database and the server's GitHub login (the same setup as `factory-golden-replay`):

```bash
factory-specialist-eval <org_id> < docs-v1.jsonl > docs-v1-results.jsonl
# options: --arms baseline,specialist   --max-turns 150   --wall-time-secs 3600
```

Tasks run one at a time, because the node has room for one task pod. Each line of the output is a `TaskOutcome`:

- `accepted` and `reasons`;
- `changed_files`, `checks`, the specialist's `review` and the `judge` verdict;
- `cost_usd` and tokens over every model step of the attempt;
- `judge_cost_usd`, reported apart because it is the eval's cost, not the change's;
- `steps`: model, cost and tokens per step.

The last line is the summary. For each arm it gives `tasks`, `accepted`, `acceptance_rate`, `cost_usd` and `cost_per_accepted_change`, the plan's primary metric: every attempt's cost, accepted or not, divided by the number accepted. It also gives `specialist_beats_baseline`. That is true when the specialist is accepted more often, or equally often (at least once) at a lower cost per accepted change. A cost that is unknown never wins a tie.

**Budget.** A docs task costs one `sonnet` run for the baseline. For the specialist it costs two `opus` steps and one `haiku` run, and each accepted-so-far attempt adds one `opus` judge call. A tests task's specialist arm implements on `sonnet` instead, and each arm adds one commands pod (install, tests, at most 10 mutants and the seeded faults, each command under 300 s). All of it runs on the Claude subscription.

## Turning a specialist on

A specialist runs in production only when the worker's `FACTORY_SPECIALISTS` names it, as in `FACTORY_SPECIALISTS=docs` or `FACTORY_SPECIALISTS=docs,tests`. It is off by default, so the generic resolver is unchanged until the eval shows a win. Even when it is on, a specialist applies only to Claude Code issue-resolver runs whose issue labels give its class (`gateway::class_from_labels`).

The tests specialist also needs a sandboxed run. If the specialist's checks fail, the run ends as `blocked_policy` with code `specialist_checks_failed`. If the review rejects the change, the code is `specialist_review_rejected`. In both cases the reasons are under `specialist` and nothing is published.
