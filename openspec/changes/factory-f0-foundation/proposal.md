# Proposal — Factory F0: Foundation

**Change:** `factory-f0-foundation`
**Project:** nexus-mind
**Status:** proposed
**Author:** Cesar Ruiz
**Date:** 2026-09-29
**Plan:** [`docs/factory/PLAN.md`](../../../docs/factory/PLAN.md) §7 F0

---

## 1. Problem

The factory plan needs a measured, policy-gated foundation before any routing or specialist work. There are also two concrete risks in today's autonomous merge path (`github_pr_reviewer` with `auto_merge: true`):

1. **The merge is not bound to the reviewed commit.** The review runs against the worktree HEAD. `auto_merge_pull` then re-reads the PR's *current* head, checks CI on that head, and merges without pinning a SHA. If someone pushes after the review and CI goes green, unreviewed code is merged. Runs triggered manually have no `trigger.head_sha`, so they have no stale-head guard at all.
2. **Merge eligibility ignores what the PR touches.** A clean review plus green CI can merge a change to auth, a migration, or CI config. The plan (D10) limits autonomous merge to docs/tests until the risk router exists (F3).

## 2. Scope (F0)

| # | Item | Plan ref |
|---|---|---|
| 1 | Bind auto-merge to the reviewed head SHA, and restrict it to a docs/tests path allowlist | D10, D12 |
| 2 | `schemas/factory/*` contracts (TaskSpec, RoutingDecision, ContextPack, CodeChangeProposal, VerificationReport, ActionPolicy) and Rust types | §5 |
| 3 | `ActionPolicy` storage, admin API and admin UI, gated by `factory_policy:read/write` | D14, D16 |
| 4 | Per-action evaluation order (policy → floors → Jev stub → merge checks → audit), including a 10-minute soak | §4, D17 |
| 5 | `IntakeSource` trait with GitHub and NexusMind tasks adapters that produce a `TaskSpec` | D13 |
| 6 | Cost and latency instrumentation per `task_id` | §7 |
| 7 | Jev API spike (contract, latency, real price) | D6 |
| 8 | First 50 golden tasks harvested from merged PRs | D8 |

## 3. Out of scope

Container sandbox (F1), code intelligence (F2), the live Model Gateway and router (F3), and new intake connectors beyond GitHub and NexusMind tasks.

## 4. Success criteria

- Auto-merge can no longer merge a commit that differs from the reviewed one, or a PR that touches paths outside the allowlist.
- Golden tasks are replayable, and every run records its cost.
- Policies are editable only by holders of `factory_policy:write`, through the admin UI.
