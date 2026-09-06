# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Scope of this record

This file governs three surfaces in the monorepo:

- `apps/admin` — the product panel (dark, Action Blue), used by a customer org
- `apps/backoffice` — internal operations across orgs (dark, amber), used by Smart Coder Labs
- `apps/landing` — the marketing site (Astro, light-first)

Out of scope, and deliberately not described here: `apps/bootcamp-tracker` (a separate
product that happens to share the design-system package) and `apps/migrator-tui` (a Rust
terminal UI driving `migrate-knowledge`; not a web surface). Either would need its own
record if design work moves there.

## Users

**Primary — the engineering lead / manager who governs the team's AI memory.** They
administer an org: users and roles, API keys, projects and clients, policies,
conventions, autonomous agents, usage, backups and the audit trail. They are the reason
the panel carries 29 routes and dense tables; they arrive to answer "what does my team's
AI know, who wrote it, and what is it allowed to do."

Secondary audiences, real but not the design target:

- **Developers on that team**, whose AI tools (Claude Code, Cursor, Copilot, any MCP
  client) write to and read from the store over MCP. Most of their interaction is
  through the tool, not the panel; when they do open it, it is to search memories,
  code, and sessions.
- **Smart Coder Labs internal operators**, who work in `apps/backoffice` across orgs.
- **Prospects** on the landing page, who are pre-purchase and have never seen the panel.

Roles present in the backend today: `admin`, `member`, `viewer`, `client`, `owner`.

## Product Purpose

NexusMind is a **control plane that sits between a team's AI tools and their LLMs**. It
does not replace any tool. It gives a team one shared, durable context layer plus the
governance around it: identity and auth per user and tool, RBAC/ABAC policy enforcement,
persistent cross-tool memory, an append-only audit trail, and multi-agent orchestration.

Success is that a team's engineering knowledge stops living inside individual AI
sessions — it survives compaction, tool switches, and staff turnover, and a lead can see
and govern all of it from one place.

## Positioning

The differentiator is the *control plane* position itself, not the memory store. A
competing memory product stores context for one tool; NexusMind is BYOT (bring your own
tool) and BYOM (bring your own model), sitting in front of both, so identity, policy,
and audit are enforced once regardless of which tool or model a developer uses. The
audit trail is append-only and hash-chained.

## Operating Context

- Developers reach the product through an **MCP server over stdio**, launched by their
  editor. The panel is the second surface, not the first.
- Auth is an **API key per user, scoped to an org**. Multi-tenant by design: one
  deployment serves many orgs.
- The product ships as a **self-hostable single deployment** (Docker Compose today;
  `deploy/` also carries Oracle and u2s/k3s targets, and `infrastructure/` carries
  Terraform).
- Sibling repositories are part of the operating surface:
  `smart-coder-labs/nexusmind-mcp` (the published MCP server) and
  `smart-coder-labs/nexusmind-claude-plugin` (the Claude Code plugin bundling the MCP
  server, hooks, and skills).
- Demo data is seeded by `scripts/reset-demo.sh`, which prints working API keys — the
  standard way anyone first sees the panel.

## Capabilities and Constraints

Shipped capability areas, as reflected by the admin routes and backend modules:

| Area | Surface |
|---|---|
| Knowledge | memories, collections, tags, conventions, sessions, search, graph |
| Code | projects, code search and indexing, symbol context |
| SDD | versioned specs, changes, and artifacts |
| Work | tasks, sprints, autonomous agents and their runs |
| Access | users, roles, API keys, agents, policies, clients |
| System | webhooks, audit log, usage, backups, migrations, settings |

Constraints and terminology:

- Backend: Rust + Axum + SQLite (`rusqlite`, bundled). Admin and backoffice: React 19 +
  Vite + Tailwind v4 + TanStack Query. Landing: Astro + Tailwind, with a Supabase-backed
  waitlist. MCP server: TypeScript.
- "Memory", "convention", "collection", "harness", "SDD change", "autonomous agent" are
  product terms with specific meanings in the API — do not rename them in UI copy.
- **Product copy language is English on every surface.** `apps/landing` is currently
  written in Spanish while declaring `<html lang="en">`; that is debt to be translated,
  not a bilingual strategy. No i18n infrastructure exists in any app today, and none is
  planned by this decision.
- Several pages are monolithic (`Memories.tsx` ~3.3k lines, `Settings.tsx` ~1.4k,
  `Dashboard.tsx` ~1.3k). Refactor opportunistically; never as a pure-styling batch.

## Brand Commitments

Binding:

- The name **NexusMind**, published by **Smart Coder Labs**.
- **MIT licensed, open source.**

Explicitly *not* binding: the Apple-derived visual language currently in place
(Action Blue `#0066cc`, the amber backoffice variant, pill CTAs, surface-step elevation
with no shadows), codified in `docs/DESIGN_DIRECTION.md` v1.0 and the
`@smart-coder-labs/apple-design-system` package. The user has confirmed this is the
**incumbent state, replaceable** — evidence of where the product is, not a promise. A
future redesign may propose and substitute a different visual world; until one does,
`docs/DESIGN_DIRECTION.md` remains the operative spec for refinements.

## Evidence on Hand

Real and citable:

- Working software and seeded demo data (`scripts/reset-demo.sh`, documented demo keys).
- `docs/` — ARCHITECTURE.md, API_SPEC.md, AUTH_SPEC.md, DESIGN_DIRECTION.md, RUNNING.md,
  ENGINEERING_PROCESS.md, two ADRs, a Postman collection.
- Two public sibling repositories (see Operating Context).
- A live waitlist count, read from Supabase at build time.

**Absent — must not be fabricated or reused as proof.** The product is **pre-launch**.
Nothing on the landing page is currently enforceable:

- "Cumple SOC2" — no audit has been confirmed. Do not restate as fact.
- "Disponible on-prem" — a deployment path exists; paying on-prem customers do not.
- The hero counter renders `waitlistCount + 30`. That padding is a factual
  misstatement in shipped copy, not a rounding choice, and should be removed.
- Pricing tiers, SLA figures, and "Trusted by forward-thinking teams" have no named
  customers behind them. There are no testimonials, logos, case studies, or benchmarks.

## Product Principles

1. **Never replace the developer's tool.** Every capability must work through whatever
   AI client the team already uses; the panel is for governance, not for doing the work.
2. **Governance is the product, memory is the substrate.** Who wrote it, who may read
   it, and what happened is as important as the content.
3. **The lead's question is "what does my team's AI know, and is that allowed?"** Design
   for auditability and scanning across large volumes, not for single-record beauty.
4. **Legibility over miniature chrome.** Density is legitimate here; illegibility is not.
5. **Claim nothing the repository cannot prove.** Pre-launch status means marketing copy
   states capability, never adoption or certification.

## Accessibility & Inclusion

**WCAG 2.2 AA is the required floor** for all three surfaces. This makes the already
inventoried defects non-conformances rather than opinions: missing `:focus-visible`
states on admin sidebar nav / notification bell / sign-out, `text-quaternary` (#555) used
for body-size text on `#1d1d1f`, no `prefers-reduced-motion` handling in any app, and
`apps/landing` declaring `lang="en"` over Spanish content.
