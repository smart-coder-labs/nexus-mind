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
    - Docs: `*.md`, `*.mdx`, `*.txt` under `docs/`, anything under `docs/`.
    - Tests: any path segment `tests`, `test`, `__tests__`, `e2e`; file names `*.test.*`, `*.spec.*`, `*_test.go`, `*_test.rs`, `test_*.py`, `*_test.py`.
  - Explicitly **never** eligible, even under a docs or test path: `.github/`, lockfiles, `Dockerfile*`, `*.toml`, `package.json`, `openspec/config.yaml`, `.mcp.json`, `CLAUDE.md`, `AGENTS.md`.

  `CLAUDE.md` and `AGENTS.md` are agent instructions, so changing them changes agent behavior.

- **Order of checks** in `auto_merge_pull`, cheapest and most decisive first: already merged → reviewed SHA known → head matches → paths eligible → CI green → publish authority → pinned merge.

### Why hard-coded patterns

The allowlist is the interim floor (D10). The configurable version is `ActionPolicy` (item 3), which is control-plane owned. The run config (`claim.config`) is editable by `autonomous_agent:update` holders and must not be able to widen merge eligibility, so there is deliberately no config override.

### Not in item 1

The soak window (D17) needs a delayed re-check, which the current one-shot worker cannot do. It lands with the per-action policy engine (item 4). Until then the head-pinned merge is the guard against late pushes.
