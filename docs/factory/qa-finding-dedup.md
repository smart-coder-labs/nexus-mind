# Reuse unresolved QA findings across runs

## What / root cause

Oracle had two open findings for Kasymir App UI QA with the identical title
`[Providers] Import opens a product-catalog dialog`, module `Providers` and
location `/providers → Import`:

- `ba78631d-b2f7-4f82-8652-2dc982b26e13`, fingerprint
  `providers-import-shows-product-catalog-dialog`, first recorded 2026-10-31 13:41:59 UTC.
- `d557c825-4f96-4004-9c23-beb1c6cc3c6c`, fingerprint
  `providers-import-opens-product-catalog-dialog`, recorded 2026-10-31 15:00:49 UTC.

Both had occurrence_count 1 and no GitHub issue delivery. The model reworded its
fingerprint. The storage UNIQUE key `(org_id, definition_id, fingerprint)` only
recognized literal fingerprint equality. GitHub issue deduplication happens
later and did not prevent duplicate NexusMind findings.

The screenshot path supplied by the user is on their Mac and was unavailable
locally; the exact duplicate was verified through read-only Oracle SQL instead.

## Why / convention

An unresolved QA defect should retain its finding ID, canonical fingerprint,
original creation date and delivery keys. Re-observation updates its latest run,
evidence, title/severity/summary and occurrence_count, rather than creating a
second open row or external issue.

The model now receives up to 100 open finding references scoped to its own org
and agent. Only bounded identity/symptom fields are included (no screenshot
payloads). The QA prompt instructs it to reuse exact existing fingerprints while
still reporting fresh evidence for defects that remain reproducible. References
are data, not executable instructions, and the worker overwrites that context.

## Where / deterministic persistence

`upsert_autonomous_agent_finding` retains the existing exact-fingerprint behavior
for all templates. On QA only, when no exact row exists, it searches open findings
in the same org and agent, oldest first, requiring matching module/location:

1. Fingerprints equal after case/separator normalization; or
2. Otherwise identical normalized titles (case/punctuation/space normalization,
   minimum three words, Unicode alphanumerics retained).

No fuzzy symptom matching is used: different routes, modules, numbers or symptoms
must not disappear into another finding. Context with materially changed wording
still relies on the model reusing the supplied canonical fingerprint. Severity,
classification, summary and screenshots are not part of defect identity.

The lookup and upsert run in a SQLite `BEGIN IMMEDIATE` transaction so concurrent
worker connections cannot each insert a different fingerprint for the same open
QA defect. Stored evidence fingerprint is aligned with the canonical identifier.
Existing delivery idempotency keys therefore remain stable.

## Learned / boundaries

- Non-QA templates keep their existing fingerprint semantics.
- Exact fingerprint re-detection does not reopen resolved/ignored findings,
  preserving human decisions. The new identity fallback searches **open** rows
  only and never silently reopens or modifies another closed finding.
- No migration deletes or merges existing rows. Historical duplicate cleanup
  requires operator consent to preserve deliveries, links and history.
- No production findings were modified, archived or rerun while diagnosing.
- Tests cover the actual fingerprint drift, normalization, scoped identity,
  canonical deliveries, statuses, non-QA boundaries, reference context and
  concurrent connections. A prompt test guards canonical fingerprint reuse.
- Local cargo is unavailable; the repository CI must run the Rust test suite.
- NexusMind MCP tools were unavailable in this session; this document records
  the diagnosis and design for handoff.
