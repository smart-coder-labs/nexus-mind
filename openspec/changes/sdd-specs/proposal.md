# Proposal — SDD Specs (the living specification)

**Change:** `sdd-specs`
**Project:** nexus-mind
**Status:** proposed
**Date:** 2026-07-12

---

## 1. Problem

`openspec/` has **two** trees. `sdd-artifacts` modelled one of them.

| Tree | What it is | In NexusMind? |
|---|---|---|
| `openspec/changes/{name}/` | The work in flight: proposal, delta specs, design, tasks, reports. | Yes — `sdd_changes` / `sdd_artifacts` / `sdd_artifact_revisions`. |
| `openspec/specs/{capability}/spec.md` | **The source of truth.** The living specification — what a capability has actually agreed. `sdd-archive` merges each change's delta specs into it when the change closes. | **No. Nothing.** |

So the platform centralises the **drafts** but not the **contract**.

Three living specifications exist in this repository right now — `harness-library` (117 lines),
`harness-config-review` (74), `harness-install-approval` (66) — and NexusMind has never seen a byte of
them. The importer does not even walk the directory.

The consequences are not cosmetic:

- **An agent asking "what does the spec say about X?" can only find drafts.** `search_sdd_artifacts`
  searches `openspec/changes/**`. A hit is something somebody *proposed*, in a change that may have
  been abandoned. An agent that quotes it as the specification is quoting a rejected proposal as
  though it were the contract, and nothing in the response tells it the difference.
- **A change's outcome is invisible.** `sdd-archive` merges deltas into the main spec and the record of
  that merge exists only in git. You cannot ask "which specs did this change actually change?", nor
  "which changes shaped this requirement?".
- **The admin shows the drafting and not the result.** `/sdd` lists changes moving through a pipeline
  whose output is nowhere on the page.

## 2. What we are building

A living specification is a **first-class entity**, not an artifact of a change.

- `sdd_specs` — one row per `(org, project, capability)`. The contract.
- `sdd_spec_revisions` — immutable, append-only history, each revision carrying
  **`merged_from_change_id`**: which change merged its deltas to produce it.
- `sdd_specs_fts` — full-text over the latest revision of every contract.
- `PUT /v1/sdd/specs` and the reads beneath it, mirroring the artifact endpoints exactly.
- `GET /v1/sdd/changes/:id/specs` — the reverse edge.
- `GET /v1/sdd/search` extended to span **both** trees, every hit saying which one it came from.
- A `sdd_specs` facet on `global_search`, gated exactly as `sdd_changes` is.
- The importer walks `openspec/specs/*/spec.md`, over both sinks, idempotently.
- The admin's `/sdd` page gains a **Specs** view; a change's drawer gains **Specs merged**.
- Three MCP tools: `save_sdd_spec`, `get_sdd_spec`, `list_sdd_specs`.

`merged_from_change_id` is the payoff. From a change you can see which specifications it changed;
from a specification you can see which changes shaped each revision. Neither direction exists today
outside `git log`.

## 3. Why a new entity and not a new artifact kind

The tempting shortcut is an `SddArtifactKind::MainSpec` hanging off a synthetic change. It is wrong,
and it is wrong in a way that would be expensive to undo later.

**A main spec is not an artifact of a change. It outlives the changes that modify it.** Twenty changes
may amend `harness-library` over two years; the specification is the thing that persists, and each of
those changes is an *event in its history*. Modelling the spec as a child of a change inverts the
relationship — it makes the durable thing a dependent of the ephemeral one, requires a fictional change
to own specs nobody amended this quarter, and makes "delete this change" a question about whether the
contract survives.

Ownership is the tell: `openspec/changes/{name}/` is deleted when the change is archived. `openspec/specs/`
is never deleted. They are different lifecycles, so they are different tables.

Hence: `sdd_spec_revisions.merged_from_change_id` is `ON DELETE SET NULL`, never `CASCADE`. Purging a
change may cost the spec its provenance; it must never cost it its content.

## 4. Non-goals

- **No authoring in the admin.** The contract is written by the harness and by git; the admin reads it.
  Same rule (A7) as artifacts, and for the same reason.
- **No delta-to-main merge engine.** `sdd-archive` performs the merge and calls `save_sdd_spec` with the
  result. NexusMind stores what it is told, it does not compute the merge.
- **No new permission strings.** `sdd:read` / `sdd:write` / `sdd:delete` already mean the right things.
- **No `delete_sdd_spec` MCP tool.** An agent does not delete a contract.

## 5. Risks

| Risk | Mitigation |
|---|---|
| The 1 MB cap rejects a large spec mid-write, leaving a half-created row. | The guard runs **before** the transaction opens (A2), as it does for artifacts. Asserted. |
| A revert (A → B → A) silently no-ops instead of recording that it happened. | The hash is compared against the **latest revision only**. A → B → A appends revision 3. Asserted. |
| `GET /v1/sdd/search` starts returning hits with a shape existing callers do not expect. | Artifact hits keep every field they had. `hit_type` is additive, and the spec-only fields are optional. The MCP formatter is updated in the same change. |
| The `sdd_specs` facet 403s `global_search` for users without `sdd:read`. | Gated on the same boolean as the `sdd_changes` facet — **empty, never a 403** (A4). Asserted. |
| A `merged_from_change_name` that names nothing silently stores a NULL, and the spec's history lies by omission. | It is a **404 and the save is rejected whole**. Asserted at the store, the API and the MCP tool. |
