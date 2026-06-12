# Proposal — Policy Engine (MVP)

> **Change**: `policy-engine`
> **Status**: proposed
> **Owner**: backend
> **Date**: 2026-06-11

---

## 1. Intent

### Problem

NexusMind is positioned as a **control plane** for enterprise AI tooling. The PRD identifies the **Policy Engine** as a P0 requirement: the governance layer that evaluates every AI tool request against org-defined rules before that request reaches an LLM. Today, the backend has memory storage, audit trails, RBAC, and projects — but **no enforcement layer**. There is no way for a CTO to declare "only `claude-3-5-sonnet` is allowed" or "redact SSN-shaped strings before any prompt leaves my org", and no way for tools (Claude Code, Cursor, Copilot plugins) to ask the platform "is this request allowed?".

Without the Policy Engine, NexusMind cannot deliver on its core promise to the **VP Engineering / CTO** persona ("I want any tool my team uses to comply with our policies") or to the **Compliance Officer** persona ("I need to know what data each agent sees, in real time"). The Tool Integrations API surface in `API_SPEC.md` already advertises `POST /v1/policy/check` and `GET /v1/policies` — but those endpoints return 404 today.

### Why now

1. The Memory System and Audit Trail (the other P0 layers) are in place. Policies are the missing third leg of the P0 triangle.
2. The MCP server and plugin work cannot ship a credible "governance" story without an enforcement primitive.
3. The schema is ready: `organizations`, `users`, `roles`, and `audit_logs` already exist (current migration is `v9`). Adding `policies` is a single, low-risk migration (`v10`).
4. RBAC permissions are string-based (`memory:read`, `audit:read`, …) and easy to extend with `policy:read` / `policy:write` without touching the auth middleware.

### Success looks like

- A CTO can `POST /v1/policies` to create a `model_whitelist`, `budget_limit`, or `pii_redact` rule scoped to their org.
- Any tool with a valid API key can `POST /v1/policy/check` with `{ model, prompt_tokens?, user_id?, project? }` and get `{ allowed, violations[] }` back in **<50 ms p95** (PRD non-functional requirement).
- All four CRUD endpoints (`GET / POST / PATCH / DELETE`) enforce org isolation and permission checks (`policy:read` for read, `policy:write` for mutate).
- The change is shippable in one PR (single migration, one new module, mechanical wiring in router + permissions).

---

## 2. Scope

### In scope (MVP)

1. **DB migration `run_v10`** — adds `policies` table + `idx_policies_org` index. Idempotent, follows the established pattern (guard on `PRAGMA user_version`, ignore `duplicate column` errors).
2. **Three rule types**, hard-coded as a `CHECK` constraint on `rule_type`:
   - `model_whitelist` — rejects requests whose `model` is not in `config.allowed_models[]`.
   - `budget_limit` — rejects when the org's daily `audit_logs` count exceeds `config.max_requests_per_day` OR when daily token sum (from `audit_logs.metadata`) exceeds `config.max_tokens_per_day`.
   - `pii_redact` — rejects (MVP) when the optional prompt preview matches any of `config.patterns[]` (regex). Real redaction-and-forward is deferred.
3. **Five HTTP endpoints**, all under the existing `protected` Router (so they go through `auth` + `rate_limit` middleware):
   - `GET /v1/policies`, `POST /v1/policies`, `PATCH /v1/policies/:id`, `DELETE /v1/policies/:id`, `POST /v1/policy/check`.
4. **New module** `apps/backend/src/api/policy.rs` containing handlers + the `evaluate()` algorithm.
5. **Permission additions**: extend `get_role_permissions` so `admin` gets `policy:read` + `policy:write`, and `member` gets `policy:read` only. Permission strings are plain — no schema change.
6. **Models in `models/types.rs`**: `Policy`, `PolicyConfig` (untagged enum over the three variants), `PolicyCheckRequest`, `PolicyCheckResponse`, `PolicyViolation`.
7. **Unit tests** for: migration idempotency, JSON config round-trips, evaluation algorithm (allow / deny / partial), permission gate.

### Out of scope (post-MVP)

| Deferred | Why |
|---|---|
| OPA / Rego integration | The PRD mentions Rego as a non-functional requirement; for MVP, a closed enum of three rule types is enough to cover the CTO journey. Rego adds a runtime dependency and a non-trivial evaluation surface. |
| Real-time WebSocket streaming of policy decisions | Audit trail already records every interaction; streaming is an admin-console feature, not a backend gating concern. |
| Policy versioning / git-ops | Useful for compliance audits, but the MVP table has `updated_at` and audit log entries cover the immediate need. |
| Dry-run mode | The PRD calls this out for V1. We will add a `mode: 'dry-run' \| 'enforce'` column in a later migration without breaking the contract. |
| PII redaction (replace + forward) | MVP only **flags** PII matches as violations. Actual prompt rewriting belongs in the gateway/plugin layer, not the policy core. |
| Policy templates / inheritance | Single-org, single-rule-per-policy is enough. Templates can sit on top later. |
| Per-project policy scoping | All policies are org-scoped in MVP. The `policies` table omits `project_id` for now; adding it later is additive. |
| Cost-based budget (USD) | MVP tracks tokens + request count only. USD requires per-model pricing tables — separate change. |

---

## 3. Approach

### Shape

A single new module (`api/policy.rs`) holds both the HTTP handlers and the pure `evaluate()` function. The pure function takes `(policies: &[Policy], req: &PolicyCheckRequest, daily_stats: DailyStats)` and returns `PolicyCheckResponse`. This separation means the evaluation logic is unit-testable without an HTTP harness or DB round trip.

```
POST /v1/policy/check
        │
        ▼
  auth middleware  ──→  AuthContext { org_id, user_id, role }
        │
        ▼
  handler:
    1. Load all enabled policies for org_id    (SQL: 1 query)
    2. If any budget_limit policy exists:
         load DailyStats from audit_logs       (SQL: 1 query)
       else: skip
    3. evaluate(&policies, &req, daily_stats)  (pure)
    4. Return PolicyCheckResponse              (HTTP 200, never 403 —
                                                  the caller decides)
```

### Rationale

- **Closed enum of rule types (not OPA/Rego).** MVP. Three rule types cover the PRD examples ("no PII", "budget caps", "model whitelist"). Adding a fourth rule type later is a `CHECK` constraint update + one match arm — additive, no breaking changes. Going straight to Rego means shipping a 200-line `evaluate()` function in week 1 instead of 30, and adopting a runtime we may not need.
- **`config TEXT NOT NULL DEFAULT '{}'` as JSON.** Each `rule_type` has a different config shape. Schema-on-read (JSON column + typed enum on the Rust side) is the right tradeoff for three variants — simpler than three sub-tables, more typed than `(key, value)` pairs.
- **`POST /v1/policy/check` returns 200 with `allowed: bool`, never 403.** The endpoint is an oracle, not a gate. Plugins and tools call it advisorily; the actual block decision belongs to the caller (so a tool can choose to log-and-warn instead of fail). This matches the API_SPEC contract.
- **Budget enforcement reads `audit_logs`.** No new counter table. `audit_logs(org_id, timestamp)` is already indexed (`idx_audit_logs_org_ts` from `run_v9`), so the daily count/sum query is fast. Avoids a second source of truth.
- **PII evaluation is regex-only and prompt-optional.** The check endpoint takes a `prompt_preview?: String` for clients that want PII enforcement, but the contract works without it (only `model_whitelist` and `budget_limit` fire when prompt is absent). Compiling the regex per request is acceptable at MVP scale; we can add a cache later.
- **Permission strings, not new tables.** Adding `policy:read` and `policy:write` to the existing string-list permission model is a one-line change in `get_role_permissions` and keeps the auth surface flat.
- **`PolicyCheckRequest.user_id` is informational only in MVP.** Per-user budgets need a separate column; we don't ship that yet. Documenting the field now keeps the contract forward-compatible.

### Risks & open questions

- **Regex DoS via `pii_redact.patterns`.** A malicious admin could write a catastrophically backtracking regex. Mitigation: compile with `regex` crate (linear-time guarantees), reject patterns >256 chars at create-time.
- **`audit_logs.metadata` token field is convention-only.** Budget enforcement assumes tokens are stored under a known JSON key (e.g., `metadata.tokens_in`). If clients use different keys, budgets undercount. Spec must declare the canonical key (`tokens_total`) and we add a follow-up to backfill.
- **Single rule per policy row.** A policy that needs "model whitelist AND token cap" requires two rows. Acceptable for MVP; admin UI will compose them.
- **No cascade on org delete.** The migration uses `REFERENCES organizations(id)` without `ON DELETE CASCADE` to match the existing convention. If we ever delete orgs, policies will block — same as `api_keys`, `users`.

---

*End of proposal.md*
