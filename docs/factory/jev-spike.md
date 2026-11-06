# Jev spike — findings

Run: 2026-09-29 · model `jev-1.13.0` (requested `jev-latest`) · 12 synthetic tasks × 3 repeats = 36 calls · script `scripts/factory/jev_spike.py`

Only synthetic tasks were sent. No client or repository data left the machine.

## Summary

| Metric | Value |
|---|---|
| Errors / schema violations | 0 / 0 |
| Latency p50 / p95 | 341 ms / 452 ms |
| Mean input tokens | 640 |
| Mean cost per decision (3 questions, one call) | **$0.000027** (≈ $0.027 per 1,000 decisions) |
| Task-class accuracy | 11/12 tasks (92%) |
| Repeat stability | Identical choices across repeats; scores vary by ≤ 0.07 |

Latency and price match what TypeSafe publishes. At this price, cost is irrelevant to the routing design; the only question is quality.

## Per task

| Expected | High risk | Jev class (confidence) | `risk` score (0–3) | `needs_human` |
|---|---|---|---|---|
| docs (README typo) | no | docs (1.00) | 0.00 | 0.35 |
| docs (JSDoc) | no | docs (1.00) | 0.00 | 0.55 |
| tests | no | tests (1.00) | 0.00 | 0.51 |
| ui | no | ui (0.83) | 0.77 | 0.63 |
| bugfix | no | bugfix (0.98) | 1.57 | 0.64 |
| backend | no | backend (1.00) | 1.95 | 0.61 |
| refactor | no | refactor (1.00) | 0.96 | 0.55 |
| infra (CI Node pin) | yes | infra (1.00) | 0.37 | **0.45** |
| migration (drop table) | yes | migration (1.00) | 2.88 | 0.63 |
| security (MFA bypass) | yes | security (1.00) | 3.00 | 0.89 |
| security (JWT key) | yes | security (1.00) | 2.99 | 0.70 |
| backend (payment rounding) | yes | **bugfix (0.50)** | 2.94 | 0.72 |

## Findings

1. **`risk` (score) separates well; `needs_human` (noul) does not.**
   - Risk: docs, tests 0.0 → ui 0.8 → bugfix/backend 1.6–2.0 → migrations, auth, payments 2.9–3.0.
   - `needs_human` sits in a narrow 0.35–0.90 band. Even a README typo gets 0.35, and a documentation task 0.55.
   - All 3 *false-low-risk* hits are the same task (the CI Node pin, 0.45). Jev also scored its risk low (0.37). It judged "pin a Node version" as harmless, which is defensible on the text; it is the path (`.github/`) that makes it sensitive.
2. **The one class miss came with low confidence.** Payment rounding was classified as bugfix at 0.50. Its risk score was still 2.94, so a confidence floor plus the risk score catches it.
3. **`score` returns the rubric index (0 … n-1), not 0–1.** `RoutingDecision.risk` is 0–1, so the provider must normalize (`score / (levels - 1)`).

## Decisions for F3 (proposed, to confirm)

- **Base the merge decision on `risk` + class, not on `needs_human`.**
  - Proposal: `ModelDecision::Allow` only when class ∈ {docs, tests} with confidence ≥ 0.9 **and** normalized risk ≤ 0.15.
  - Anything else is `Hold`.
  - Keep asking `needs_human` only as an audit signal, to measure whether it becomes useful.
- **Deterministic floors stay first.** The CI case confirms why: `.github/` is a never-eligible path in `merge_gate` whatever Jev says.
- Re-run the spike on the golden tasks' own text (internal repositories only, with explicit approval) before enabling it. 12 synthetic tasks are enough to validate the contract, not to calibrate thresholds.
