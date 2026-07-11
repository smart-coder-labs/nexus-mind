# Proposal — SDD Artifacts

**Change:** `sdd-artifacts`
**Project:** nexus-mind
**Status:** proposed
**Author:** Cesar Ruiz
**Date:** 2026-07-11

---

## 1. Problem

SDD artifacts have no home in NexusMind. They exist in two places, and neither one works.

**On disk.** The SDD harness (`.claude/skills/sdd-*`) writes `proposal.md`, `design.md`, `tasks.md`,
`specs/{capability}/spec.md`, `verify-report.md` into `openspec/changes/{change-name}/`. Good for git
history and PR review. But they are invisible outside the checkout: you cannot search them from the
admin, you cannot see which specs are in flight this sprint, and a fresh session on another machine
starts blind.

**In the memory store.** The harness's persistence contract has a mode (`engram`, now backed by
NexusMind) that saves each artifact as a **generic memory** with `topic_key: sdd/{change}/{artifact}`
and `type: architecture`. This is where it hurts:

- A 36 KB `design.md` is stored as a memory row. Memory search returns it as a "preview" of ~200 chars.
- SDD artifacts and real human architecture decisions are **indistinguishable** — same `type`,
  same table. Memory search for "auth model" returns three decisions and eleven spec fragments.
- Upsert-by-`topic_key` **overwrites**. There is no revision history: re-running `/sdd-design` destroys
  the previous design. The persistence contract admits this in writing ("no iteration history").
- Nothing links an artifact to a task, a sprint, a project, or the memories it produced.

**The linking gap.** `task_spec_links` already exists — a task links to a change by folder-name string,
with `link_task_spec` / `resolve_tasks_for_spec` MCP tools on top. It works. But the backend validates
that name by **reading the local filesystem** (`spec_change_exists` → `openspec/changes/<name>`). In
production the backend runs on Fly.io, where no `openspec/` directory exists, so the check finds an
unreadable root and **returns `true` (allow)**. Today the validation is decorative: any string links.

So: the join key exists and is right, but there is nothing on the other end of the join.

## 2. Proposal

Make SDD artifacts a **first-class NexusMind domain**, stored and indexed in the backend, surfaced in
the admin, written by the harness — while the markdown files keep living in git.

**Dual persistence, explicitly.** Both stores are real, and each is authoritative for what it is good at:

| | git (`openspec/`) | NexusMind |
|---|---|---|
| Authoritative for | the reviewable text, in a PR, on a branch | the queryable, linkable, cross-session record |
| Gives you | diff review, blame, offline, rollback | search, admin UI, task/sprint/memory links, recovery after compaction |
| Records the other | — | `git_path`, `git_commit`, `content_hash` per revision |

Neither is a cache of the other. The harness writes both; NexusMind stores every revision it is handed
(immutable, content-hashed, de-duplicated), so re-running a phase **appends** instead of destroying.

**Indexed content.** Every artifact revision's markdown is stored in full and indexed in FTS5 — the same
index the memory store already uses. `search_sdd_artifacts` and a facet in `global_search` make "which
spec covers rate limiting?" answerable without a checkout.

**Linked to everything.**
- **Tasks** — reuse `task_spec_links` as-is. Additionally, flip `spec_change_exists` to validate against
  the `sdd_changes` table (falling back to the filesystem), which closes the Fly.io hole above.
- **Memories** — an M:N link so a `design.md` can point at the decisions and bugfixes it produced.
- **Project + Sprint** — every change belongs to a project; optionally to a sprint. Gives the
  "what are we speccing this sprint" view.
- Code/symbol linking is **out of scope for v1** (see §5).

**Closing the loop.** A 5th persistence mode, `nexusmind`, is added to the harness contract, and the ten
`sdd-*` skills are updated to write through the new MCP tools. Without this the admin section ships
empty and nobody fills it — the platform and the producer land together, or not at all.

## 3. Why now

Three things converge:

1. The harness's `engram` mode is **already degraded**. Engram was migrated to NexusMind; the SDD skills
   still speak the old contract and dump artifacts into the memory table. Every SDD run makes memory
   search worse.
2. `task_spec_links` shipped with team-tasks and its FS validation is **dead code in production**. It
   needs a real referent.
3. There are already artifacts to import: `openspec/changes/**` (7 active changes, 4 archived) plus the
   `sdd/*` memories carried over from Engram. A one-shot importer backfills them, so the section is
   populated on day one.

## 4. Scope

### In scope

- **Backend**: `sdd_changes`, `sdd_artifacts`, `sdd_artifact_revisions`, `sdd_change_memories` tables +
  FTS5 index; `sdd:read` / `sdd:write` / `sdd:delete` permissions; `/v1/sdd/*` endpoints; DB-backed
  `spec_change_exists`; an importer binary for `openspec/**` and legacy `sdd/*` memories.
- **MCP**: `save_sdd_artifact`, `get_sdd_artifact`, `list_sdd_changes`, `get_sdd_change`,
  `update_sdd_change`, `search_sdd_artifacts`, `link_sdd_change_memory`.
- **Admin**: a `/sdd` section (change list, artifact detail with rendered markdown + revision history),
  a shared `<Markdown>` primitive with GFM (tables and `- [ ]` checklists are mandatory — `tasks.md` is
  nothing but checklists), cross-links from the task detail, and an SDD facet in global search.
- **Harness**: `nexusmind` persistence mode; `sdd-*` skills updated; published to the harness library.

### Out of scope (v1)

- **Semantic search over artifacts.** `memory_embeddings` is FK'd to `memories.id`, so artifacts need
  their own embeddings table, and a 36 KB document needs heading-level chunking to embed usefully.
  FTS5 first; semantic is a follow-up, flagged behind the existing `NEXUSMIND_EMBED_ENABLED`.
- **Code/symbol links** to the code-knowledge-graph. Least mature, most expensive, no demand yet.
- **Editing artifacts from the admin.** The admin is read + link. Artifacts are written by the harness
  and by git. A write path from the UI would make "which store won?" an open question, and it isn't one.
- **Diff UI between revisions.** No diff library is installed in the admin today. Revisions are listed
  and individually viewable; rendering a two-column diff is a follow-up.
- **Deleting `openspec/`.** The files stay. This proposal is explicitly *not* a migration off git.

## 5. Key decisions

| # | Decision | Rationale |
|---|---|---|
| D1 | NexusMind stores **full content**, not a pointer to git | A pointer cannot serve cross-session recovery, cannot be searched, and cannot replace `engram`. Storage is cheap; a 36 KB doc is smaller than the memories it currently pollutes. |
| D2 | Revisions are **immutable and append-only**, de-duplicated by `sha256(content)` | Fixes the `topic_key` overwrite that loses iteration history. Re-running a phase with identical content is a no-op, so the harness can call `save_sdd_artifact` freely without churn. |
| D3 | Reuse `task_spec_links` (join by `spec_change_name`), do **not** add a `change_id` FK to tasks | The name is already the key across three MCP tools, `Task.spec_links`, and the admin. Adding a parallel FK creates two sources of truth for the same edge. |
| D4 | `sdd_changes.project` is a **name string**, not a FK | Matches `tasks.project` and `sessions.project` exactly. Deliberate in v51 to keep unregistered/org-shared projects visible. Consistency beats normalization here. |
| D5 | `spec_change_exists` checks the **DB first, filesystem as fallback** | Makes the check real in production without breaking a local backend running inside a checkout. |
| D6 | The admin is **read-only** over artifacts | See "out of scope". One writer (harness/git), many readers. |
| D7 | The harness update ships **in the same change** | A store nobody writes to is a dead table. |

## 6. Risks

| Risk | Mitigation |
|---|---|
| **Artifact content is large** (36 KB design.md); FTS5 index and API responses grow | List endpoints return metadata only — never content. Content is fetched per-artifact-revision. Cap a revision at 1 MB, return 422 above it. |
| The harness writes on every phase → **revision churn** | Content-hash de-dup (D2): identical content creates no revision. |
| **Response-shape change** to `GlobalSearchResult` (a new `sdd_changes` field) breaks the admin | Both are in this monorepo and ship together; the field is additive and the admin ignores unknown keys today. Covered by a test. |
| Importer **double-inserts** on re-run | Importer is idempotent by `(project, change_name, kind, capability)` + content hash. Safe to re-run. |
| Legacy `sdd/*` memories are imported **and** left in the memory table → duplicates in search | Importer tags migrated memories and the follow-up archives them; it does **not** delete. Deletion is a separate, explicit user decision. |
| Cross-repo change (backend + mcp + admin + harness) is **hard to review** | Ship as 10 chained PRs (see design.md §8), each independently green. |

## 7. Success criteria

1. Running `/sdd-design` on any project writes `design.md` to disk **and** a versioned artifact to
   NexusMind, and re-running it a second time with edits produces **revision 2**, not an overwrite.
2. The `/sdd` admin section lists every change across all projects, with its phase, and renders
   `tasks.md` with working checkboxes and tables.
3. From a task, you can reach the spec that motivated it; from a change, you can see every task it
   spawned and every memory it produced.
4. `link_task_spec("does-not-exist")` returns 422 in production, not 201.
5. A session that starts fresh on another machine can recover a change's full state from NexusMind
   alone, with no checkout.
