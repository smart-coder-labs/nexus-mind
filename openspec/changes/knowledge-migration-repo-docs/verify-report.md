# Verify Report — Repo Docs Connector

> **Change**: `knowledge-migration-repo-docs`
> **Branch**: `sdd/knowledge-migration-core`
> **Date**: 2026-08-15
> **Verdict**: ✅ complete — every requirement has a passing test, and the connector has been run
> against a real 162-document tree

---

## 1. Gates

| Gate | Result |
|---|---|
| `cargo test` | **1145 lib + 46 integration + 12 runner — 0 failures**, two consecutive clean runs |
| `cargo clippy -- -D warnings` | clean |

Tests: 1121 → **1145** (+24). The runner binary went 11 → 12.

---

## 2. Requirement coverage

| Requirement | Evidence |
|---|---|
| Section-Level Scanning | `scan_splits_a_document_into_sections`, `a_document_without_headings_yields_one_unit` |
| Deterministic Identity Per Section | `identity_is_stable_across_rescans`, `editing_one_section_changes_only_its_identity`, `moving_a_document_changes_its_identities`, `identity_never_contains_an_absolute_path` |
| Default Mapping From Source To Destination | `adr_path_proposes_a_decision_memory`, `unchecked_checklist_item_proposes_a_task`, `checked_items_propose_no_task`, `rule_shaped_section_proposes_a_convention`, `openspec_change_proposes_an_sdd_artifact_only_with_the_flag`, `plain_prose_falls_back_to_an_architecture_memory` |
| Source Excerpt Accompanies Every Candidate | `every_candidate_carries_a_verbatim_excerpt` — asserts each excerpt line appears in the source |
| Scanning Without A Classifier | `fallback_produces_a_candidate_for_every_unit`, `fallback_reports_no_confidence` |
| Cost Is Estimable Before It Is Spent | `scan_report_counts_documents_units_and_exclusions`, `scan_report_and_scan_agree` |
| Noise Is Excluded By Default | `default_excludes_skip_marketing_research_and_living_specs`, `excluded_documents_are_reported_not_omitted` |

---

## 3. The measurement that only a real run produces

```
dry run — source=repo-docs documents=162 units=3377 bytes=2057239 estimated_tokens≈514309
excluded 26 document(s)
```

The proposal said "161 `.md` files". That was a count from `find`; this is the connector's own
measurement of what it would actually process, and it agrees.

**Two things follow, and both matter before the first paid run:**

1. **~514 000 tokens is not a number you spend by accident.** `--max-tokens` exists and now has
   a figure to be set against. Running by subdirectory (`--include docs/adr`) is the sane first
   pass.
2. **3377 candidates make the human gate the bottleneck** — precisely the risk the core's
   proposal recorded. Three mitigations already exist: confidence ordering in the review UI,
   batch approval for verified provenance, and — the one that carries the most weight — the
   `skip` verdict the prompt lets the classifier return. Without `skip`, 3377 units are 3377
   human decisions and nobody does that work.

The dry run is what turns both of those from a guess into a decision.

---

## 4. What is verified, and what is not

**By test**: everything in §2, plus `scanning_this_repository_produces_plausible_candidates`,
which runs the connector over this checkout's `docs/` and asserts properties rather than counts —
`ENGINEERING_PROCESS.md` yields at least one convention, no candidate carries an absolute path,
every candidate carries an excerpt.

**Not verified**: `claude -p` has still never been invoked by this code. The classifier adapter
is covered by fixtures of its envelope. The `--no-llm` path is fully exercised; the LLM path is
not, and the first real classification remains a first.

---

## 5. Adversarial review — 2 findings

### 🟠 A section with twelve checkboxes produced one task

`scan()` emitted one unit per section, so a roadmap with twelve `- [ ]` collapsed into a single
candidate titled after the first box. A reviewer approving it would have believed the roadmap
was captured while **eleven tasks were silently lost**.

A section with N unchecked boxes now emits **N units**, each with its own identity
(`{anchor}-task{idx}`) and its own verbatim excerpt. Checked boxes still produce nothing.
Checklists inside an ADR or an `openspec/changes/**` are deliberately exempt — those are that
decision's follow-up list, not the team's backlog.

### 🟡 My own BYOM test mutated the process environment

It called `std::env::remove_var`. The environment is process-global and this suite runs in
parallel, so a test that removes a variable can break an unrelated one mid-flight. The BYOM
claim never needed it. Removed.

### Reported, not fixed — a pre-existing flake in `crypto`

`crypto::tests::with_key` does `set_var` then `remove_var` on the **same** variable, and several
tests in that module call it concurrently: one clears the key while another is using it.
`decrypt_rejects_tampered_blob` failed once in a full run and passes 3/3 in isolation.

Left alone on purpose — it is code unrelated to this change, and fixing it (a mutex around
`with_key`, or serialising those tests) deserves its own commit so the diff says what it does.
Recorded here because it will fail CI again and the cause should be written down somewhere.

**Verdict: approved.** No blockers.

---

## 6. Pending

- [ ] Fix the `crypto::tests::with_key` flake — its own commit.
- [ ] First real classification pass, scoped (`--include docs/adr`) and budgeted.
- [ ] The three remaining connectors, still in `propose`.
- [ ] **The NDA question**, still unanswered, still blocking two of them.
