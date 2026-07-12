# Design — SDD Specs (the living specification)

**Change:** `sdd-specs`
**Project:** nexus-mind

---

## D1 — A living specification is a root entity, not an artifact

**Decision.** `sdd_specs` is its own table, keyed on `(org_id, project, capability)`. It does not
reference `sdd_changes`.

**Why.** `openspec/specs/{capability}/spec.md` belongs to the **project** and outlives every change
that amends it. Modelling it as an `SddArtifact` of a change would make the durable thing a child of
the ephemeral one — and the lifecycles are visibly different: a change folder is deleted when the
change is archived, the specs tree never is.

The relationship runs the other way, and only through history:
`sdd_spec_revisions.merged_from_change_id` says *which change produced this revision*. A change is an
event in the life of a specification, not its parent.

**Consequence.** `merged_from_change_id` is `ON DELETE SET NULL`. Purging a change costs a revision its
provenance; it must never cost the specification its content.

## D2 — `project` is a name string (inherited from D4 of sdd-artifacts)

`sdd_specs.project TEXT NOT NULL`, not a `project_id` FK — mirroring `sdd_changes.project`,
`tasks.project`, `sessions.project`. Unregistered and org-shared project names stay visible.

## D3 — Idempotent by content hash, against the LATEST revision only

`upsert_sdd_spec` is the exact analogue of `upsert_sdd_artifact`:

- Identical content → **no revision, no FTS write, no `updated_at` bump.** The call is free to make on
  every run of a phase.
- The hash is compared against **the latest revision only, never the whole history.** Content
  A → B → A appends **revision 3**. Reverting a contract to an earlier text is an event and the
  history must show it happening — a `UNIQUE(spec_id, content_hash)` would silently swallow it.

## D4 — The 1 MB cap is atomic

The size guard is the **first statement of the function**, before the transaction opens and before any
row is resolved-or-created. A rejected save leaves **no spec row and no revision row** — not a spec at
`latest_revision = 0` with nothing under it. Error: `spec_too_large` → **422**, never a 413.

## D5 — Revisions are immutable and append-only

Nothing in `db/queries.rs` may modify or remove a row of `sdd_spec_revisions`. They are produced by
`upsert_sdd_spec`'s INSERT and reclaimed only by `ON DELETE CASCADE` from the parent spec.

A source-scan test (`no_store_function_mutates_a_spec_revision`) enforces this by reading the file with
`include_str!`. **Its needles are assembled at runtime with `format!`.** Spelling them as string
literals would plant them in the very file the scan reads — `include_str!` pulls in the test module
too — and the test would match itself and fail against perfectly correct code. This has bitten this
codebase repeatedly; the artifact scan carries the same warning.

## D6 — FTS is delete-then-insert, tracking the latest revision only

`sdd_specs_fts` is standalone FTS5, not external-content: many revisions map to one indexed document,
so the `memories_fts` trigger pattern does not apply. Every new revision deletes the spec's row and
inserts a fresh one, so a specification contributes exactly **one hit** however long its history, and a
requirement struck from the contract stops matching.

## D7 — `merged_from_change_name` that resolves to nothing is a 404, and the save is rejected whole

The provenance is the entire reason the column exists. Storing the content with a silently-NULL
`merged_from_change_id` would leave a specification whose history lies by omission — it would *look*
like a revision that came from outside the change pipeline, which is a different and legitimate state
(an import, an admin edit).

So: resolve the name **before** anything is written; an unresolvable name errors with
`change_not_found` → **404**, and nothing lands. The MCP tool says so in as many words: *"The
specification was NOT saved."*

The lookup is scoped by `(org_id, project, name)`, so another org's change is not a resolvable
provenance either.

## D8 — `GET /v1/sdd/search` spans both trees, and every hit says which one

**Decision.** The endpoint returns `SddSearchResult`, carrying a required `hit_type` of `"spec"` or
`"artifact"`, with the tree-specific ids as `Option`.

**Why.** "Which spec covers rate limiting?" is a question about the **contract**. Answering it with
three drafts from an in-flight change answers a different question, and an agent has no way to notice.
A spec hit genuinely has no `change_id`; giving it one — or omitting the discriminator and letting the
caller guess from which fields are populated — would be a lie with a plausible shape.

**Ordering.** Specs first, then artifacts, each ordered by its own FTS `rank`, truncated to `limit`.
The two ranks come from different FTS tables and are not comparable, so they are not interleaved and
pretended to be. The contract outranking the drafts is the correct default for the question being asked.

**Compatibility.** Artifact hits keep every field they had, so an existing caller reading `artifact_id`
still works. The store's `search_sdd_artifacts` is unchanged; the merge happens in `search_sdd_all`.

## D9 — The `sdd_specs` facet on `global_search` is empty, never a 403 (A4 restated)

Gated on the **same boolean** as the `sdd_changes` facet — one `require_permission` call feeds both, so
they cannot drift into disagreeing about who may see the SDD trees. A caller without `sdd:read` gets
`sdd_specs: []` and a 200. Gating the whole of global search on the grant would break search for every
user who does not have it.

## D10 — The importer does not invent a provenance

`import_specs` sets `merged_from_change_name: None`. The filesystem does not record which change last
merged into a spec — only git history does — and guessing would be worse than admitting there is none.
The agents running `sdd-archive` supply it on the live path, where the fact is actually known.

The importer walks only `spec.md` inside each capability directory. A capability folder may hold notes
or scratch files beside it; the convention names exactly one file, so exactly one file is imported. A
capability directory with no `spec.md` yields nothing, rather than an empty contract.

Title: the first markdown `# ` heading, or `None`. A missing title is not an error, and manufacturing
one from the directory name would be a worse answer than no answer.

`--skip-specs` takes the subtree out. `--skip-filesystem` takes it out too — the specs tree lives
*under* `openspec/`, so a run that is not walking `openspec/` is not walking the specs either.

## D11 — Both sinks, and the idempotency belongs to neither of them

`Sink::save_spec` calls `upsert_sdd_spec` on the DB sink and `PUT /v1/sdd/specs` on the API sink —
which is that same call behind a socket. The importer owns no insert path and no de-duplication logic
of its own, so a second run creates zero revisions on **either** sink, and it cannot drift.

`source` is sent in the body (`import`) and honoured by the handler. Hard-coding `agent` server-side
was a real bug on the artifact path — it stamped every imported revision as agent-authored, a lie the
DB path did not tell — and the spec path must not reintroduce it. Asserted over a real socket.

## D12 — The admin shares one document viewer between the two trees

`DocumentView` is **extracted** from `ChangeDetail`, not copied into `SpecDetail`. Both trees hold
immutable, revision-addressed markdown that the admin is read-only over. Two copies would drift — one
would grow a Raw default, or a revision selector that silently falls back to the latest when an older
revision fails to load — and the two trees would start disagreeing about what "showing a document"
means.

It owns the view mode (a purely presentational choice) and nothing else. *Which* revision is selected
stays with the caller, because the caller is the one that must reset it when the user switches document.

The specs view differs in exactly one respect, and it is the point of the feature: its revision labels
name the **change** each revision was merged from.

## D13 — Every admin query states its `sdd:read` grant

An ungated query that 403s trips the client's global error handler and redirects **the whole app** to
`/401`. So `enabled: canRead` is on every SDD query, including the ones inside drawers the parent only
renders for a permitted caller. Belt and braces, because the failure mode is catastrophic and silent.

## D14 — Permissions: no new strings

`sdd:read` on every read, `sdd:write` on the save. `sdd:delete` is unused here — there is no spec
delete endpoint and no `delete_sdd_spec` tool, because an agent does not delete a contract. Migration
v54 already grants the `sdd:*` strings to the role templates; v55 adds no grants.

## D15 — Route ordering

`/v1/sdd/specs` (the static collection) is registered **before** `/v1/sdd/specs/:id`, or `:id` swallows
it and the collection read tries to look up a spec whose id is the literal string `specs`. Same trap,
same fix, same test as the artifacts routes.

`GET /v1/sdd/specs` is one endpoint with two readings, discriminated by `capability`: with it, the
natural-key read of one contract **with full content**; without it, the list — **metadata only**.
A capability with no spec is a **404**, never a 200 carrying an empty document: "no contract yet" and
"an empty contract" are different facts, and an agent must be able to tell them apart.
