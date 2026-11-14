---
name: nexus-mind-evolve
description: The loop that lets nexus-mind improve itself with Claude Code — pick an approved GitHub issue, restate it in product terms, build it, verify it, open a PR, answer review, and turn what went wrong into harness fixes; when the queue is empty, discover and propose issues. Use when asked to "work the backlog", "evolve nexus-mind", run unattended (/loop), or pick up the next approved issue.
disable-model-invocation: true
---

# Evolve

The loop: **pick → understand → build → verify → ship → answer review → learn**. With nothing approved, it switches to **discover**.

Humans keep two gates: they approve what gets built (label `status:approved`) and they merge. Branch protection on `main` requires one approving review. Everything in between is yours.

This is the Claude Code loop for developing *this repository*. It is not the product's Factory (`apps/backend/src/factory/`, `docs/factory/`), which runs workflows for customers. Don't drive Factory runs from here, and don't change Factory safety gates to make your own run easier.

## Autonomy level

Set by whoever starts the run ("evolve, level 2"). Default **1**.

| Level | You may |
|---|---|
| 0 | read, plan, write the plan to the scratchpad (nothing leaves the machine) |
| 1 | everything up to a **draft** PR; comment on the issue |
| 2 | ready-for-review PR, answer bot review, file issues from discover (without `status:approved`) |

Never, at any level: merge, push to `main`, use `--admin`, force-push a branch you didn't create, edit `.env*` or `.mcp.json` secrets, touch `deploy/`, `infrastructure/` or `.github/workflows/` without the issue asking for it, deploy, run anything that writes to the production API (`api.nexusmind.smartcoderlabs.com`), or start/stop Factory runs on real repositories.

## 1. Pick

```bash
gh issue list -R smart-coder-labs/nexus-mind --state open --label status:approved \
  --json number,title,labels,assignees,updatedAt --limit 50
```

Skip issues assigned to someone else, labelled `status:needs-review`, or with an open PR (`gh pr list -R smart-coder-labs/nexus-mind --search "<number> in:body"`). Prefer `bug` and `security` over `type:feature`, then oldest. One issue per PR. If it would exceed ~400 changed lines, split it into stacked PRs and say so on the issue.

## 2. Understand

- Read the issue, its comments and linked PRs/specs. Check `openspec/changes/` for an existing change on the same capability.
- Load `nexus-mind-business` (or ask the `business-analyst` agent) and restate the issue: which actor (org admin, member, superadmin, agent via MCP), which invariant applies (tenancy, audit, permissions, secrets), what "correct" looks like with concrete values.
- Check the contract: the HTTP route in `apps/backend/src/api/router.rs`, the client in `apps/admin/src/api/client.ts`, and whether the MCP server (separate repo `nexusmind-mcp`) consumes it. A response-shape change breaks the MCP and plugins: keep fields additive.
- Write acceptance stories ("As an org admin, when I revoke a key, the next call with it returns 401 and an audit row appears"). They become the `nexus-mind-qa` layer-1 checks.

## 3. Build

- `git fetch origin` and branch from `origin/main` in your own worktree: `git -C <main clone> worktree add -b <type>/<slug> ../nexus-mind-<slug> origin/main`. The sibling checkouts hold other people's uncommitted work; never build there.
- Follow `CLAUDE.md`, the per-app `CLAUDE.md`/`AGENTS.md`, and `nexus-mind-design` for UI. Reuse before creating.
- Tests come with the change: a Rust unit/integration test for backend logic (`apps/backend/tests/` for HTTP), a vitest for admin logic. `openspec/config.yaml` says strict TDD for backend and admin: write the failing test first.

## 4. Verify

Run `nexus-mind-verify` to the end for every app you touched, plus `nexus-mind-qa` layer 2 for a visible change (layer 1 for a changed flow). For a larger UI change, hand the screenshots to the `design-reviewer` agent and fix what it marks blocking. Up to 3 fix rounds, then stop and report.

## 5. Ship

Commit with a conventional prefix and scope (`fix(admin): …`, `feat(backend): …`, `chore(harness): …`), as the history does. PR body:
- what changed and why (Problem / Change, like recent PRs);
- `Closes #<n>`;
- acceptance stories and their results;
- the verify table (step → result; skipped steps with the reason, e.g. "cargo unavailable locally, CI Backend job");
- screenshots for visible changes (the admin owner reviews them before merge);
- what you did not check.

`gh pr create --draft` at level 1. Watch CI with `gh pr checks <n> --watch`.

## 6. Answer review

For each thread: read the code it points at before agreeing; fix it if valid, otherwise reply with the reason; resolve bot threads yourself, never a human's. Re-run the verify steps your fix touched.

## 7. Learn

After each issue ask: what cost a retry that the harness could have prevented (wrong command, stale path, missed rule, a domain fact you had to dig for)? Fix the harness in a separate small PR titled `chore(harness): …` — the skill, `CLAUDE.md`, the audit script or a hook — replacing stale text rather than adding beside it. Also save the non-obvious part as a NexusMind memory (project `nexus-mind`), per `CLAUDE.md`.

## Discover (queue empty)

One source per run, rotating:
- **Design debt:** `design-audit.mjs apps/admin/src/pages/<Area>` → one issue per screen (e.g. hard-coded colours in `pages/Settings.tsx`).
- **Business gaps:** `nexus-mind-business` `references/gaps.md` — pick one still true on `origin/main`.
- **QA pass:** `nexus-mind-qa` on login → memories → projects → factory home with the demo seed.
- **Doc drift:** a doc in `docs/` that contradicts the code.

At most 3 issues per run (level 2), using existing labels (`bug`, `type:feature`, `ux`, `api`, `security`, `qa`, `factory`) plus `status:needs-review`, **never** `status:approved`. Body: Context, Evidence (command output or screenshot), Proposal, Acceptance stories.

## Running unattended

From a dedicated worktree on `origin/main`: `/loop 2h /nexus-mind-evolve level 1`. Each run handles one issue or one discover pass and ends with a 5-line report (picked, built, verify result, PR link, harness lesson). If a permission prompt blocks a step, stop and report it; don't route around it.
