---
name: nexus-mind-business
description: What NexusMind is, who uses it and the rules a change must respect — orgs, clients and projects, roles and permission strings, the superadmin vs super_user split, tenancy/audit/secret/memory-upsert/SDD invariants, the Factory's safety gates, the ES↔EN↔code glossary, admin route → API → module map, KPIs and known gaps. Use before deciding anything a user or an AI tool would notice (what a screen or endpoint returns, who may call it), when writing an issue or acceptance stories, and when a Spanish Factory term (Entradas, Hallazgos, Pendientes, Laboratorio…) needs mapping to code.
---

# NexusMind, the product

NexusMind is an open-source AI control plane (`PRODUCT.md`): AI coding tools (Claude Code, Codex, Cursor, via MCP and plugins) share a team memory, code index, SDD artifacts, tasks and harnesses; on top sit autonomous agents and the **Factory** (workflows that turn issues into verified PRs), plus policy, audit and administration. Customers are organizations — typically consultancies with clients — and one self-hosted single-org deployment ("u2s").

Surfaces: `apps/backend` (Rust/Axum + SQLite, the authority), `apps/admin` (org admins), `apps/backoffice` (platform superadmin), `apps/landing` (marketing), the MCP server (separate repo `nexusmind-mcp`, npm `@smart-coder-labs/nexusmind-mcp`), plugins (`docs/*_PLUGIN.md`), `apps/nexus-cli` and `apps/migrator-tui`.

## Ask these before building

1. **Who calls it?** Org admin, member, viewer, custom role, `super_user`, the backoffice superadmin, an AI tool over MCP, or a Factory runner (`references/roles.md`). Each has a different auth path.
2. **Whose data is it?** Tenant comes from the authenticated `org_id`, never the request body. Project membership narrows it further (`references/invariants.md` §Tenancy).
3. **Is it a write?** Then it must be audited, and you need a cross-tenant test.
4. **Does a tool depend on the shape?** MCP tools and plugins call these routes. Keep responses additive.
5. **Is it the Factory?** Its gates (human approval per revision, merge floor, credential-free publication) are product guarantees. Never weaken one to make a feature work.

## References

| Need | Read |
|---|---|
| Entities, hierarchy, surfaces, deploy topology | `references/domain.md` |
| Roles, permission strings, where checks happen, auth paths | `references/roles.md` |
| Rules that must never break | `references/invariants.md` |
| Spanish ↔ English ↔ code | `references/glossary.md` |
| Admin route → page → API prefix → backend module | `references/modules.md` |
| KPIs and where they are computed | `references/kpis.md` |
| Known gaps and inconsistencies (issue material) | `references/gaps.md` |

Compiled on 2026-11-14 from `origin/main` (5dc4664). The code wins over these notes and over `docs/` (several docs, e.g. `docs/AUTH_SPEC.md`, describe plans, not code). Fix a stale reference in the same PR that finds it.

The `business-analyst` agent turns an idea or issue into a spec with acceptance stories using these references.
