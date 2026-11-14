# nexus-mind

NexusMind is an open-source AI control plane: AI coding tools share a team memory, code index, SDD artifacts, tasks and harnesses over MCP, and the **Factory** turns issues into verified PRs under policy, audit and human approval. Monorepo: `apps/backend` (Rust/Axum + SQLite, the authority), `apps/admin` (org admin, React 19 + Vite + Tailwind 4), `apps/backoffice` (platform superadmin), `apps/landing` (Astro), `apps/nexus-cli` and `apps/migrator-tui` (Rust TUIs). The MCP server lives in another repo (`nexusmind-mcp`). Load `nexus-mind-business` before deciding anything a user or an AI tool would notice.

## Rules

Each one exists because breaking it costs real users.

- **Tenant from auth, never from input.** `org_id` comes from the auth middleware; every query filters by it and checks project membership. A cross-org read is the worst bug this product can have, so every data change ships with a cross-tenant test.
- **The backend enforces; the UI only hides.** `AdminRoute` and the sidebar are conveniences. Permissions are checked in `apps/backend/src/api/helpers.rs`, so put the rule there first.
- **Writes are audited.** The `audit` middleware covers protected mutations. If a handler logs its own entry, register it in `AUDIT_SKIP_PATTERNS`. The audit trail is part of what customers buy.
- **Secrets never leave encrypted.** Use `crypto.rs` (AES-GCM) and keep workspace secrets write-only. Never return, log or put a token into a prompt, fixture or PR. If there is no key, don't persist the secret.
- **Never edit a shipped migration.** Add the next `run_vN` in `apps/backend/src/db/migrations.rs` (guarded by `PRAGMA user_version`). Production already ran the old ones.
- **`/v1/*` responses change additively.** The MCP server, plugins and CLI depend on them, and they release separately.
- **Factory gates stay closed.** These include per-revision human approval, the merge risk floor, credential-free publication and PII redaction (see `invariants.md` in the business skill). Never loosen one to make a run pass.
- **Design tokens only, per app.** Admin uses Audit1 Lime, backoffice uses amber, landing uses SmartCoder lime. Lime fills take dark ink, and status is not category. Follow `nexus-mind-design`; the `design-guard` hook flags new raw colours.
- **Merging is deploying.** A push to `main` deploys the backend to Oracle k3s and the web apps to Cloudflare Pages. Humans merge, never an agent.

## Conventions the code won't teach

- **SDD:** `openspec/` holds changes and specs. `openspec/config.yaml` sets `artifact_store: nexusmind` (files *and* `save_sdd_artifact`) and strict TDD for backend and admin. The `sdd-*` skills are user-level, not in this repo.
- **Admin profiles:** `VITE_ADMIN_PROFILE=full|only-context`. A new route goes in `App.tsx`, the `Layout.tsx` nav (with `requiredPermission`) and `config/disabled-sections.ts`.
- **Admin components:** prefer the `components/ui/<Capitalized>` adapters, which wrap `components/ui/shadcn/*`. Classes go through `cn()`, imports through `@/`.
- **npm, not pnpm:** CI runs `npm ci` on `package-lock.json`; the `pnpm-lock.yaml` files are unused.
- **Demo data:** `scripts/reset-demo.sh` seeds `apps/backend/data/nexusmind.db`. Keys like `nm_demo_acme_admin` / `nm_demo_acme_sarah` / `nm_demo_techstartup_admin` are public seed values from `apps/backend/src/bin/seed.rs`.
- **Stale entry points:** `make mcp-build` / `mcp-inspect` point at a non-existent `apps/mcp/`. Don't follow them.

## NexusMind memory (we dogfood it)

The MCP server comes from `.mcp.json` (needs `NEXUSMIND_API_KEY` in your shell) or the NexusMind plugin. Tool names are `store_memory`, `search_memories`, `get_context`, `get_memory`, `list_memories`; the prefix depends on how it is loaded. The project is **`nexus-mind`** (children: `nexusmind-backend`, `nexusmind-admin`, `nexusmind-mcp`, `nexus-cli`). `scripts/nm-context.sh` (SessionStart) injects recent memories.

- **Before** non-trivial work, call `get_context(project="nexus-mind")`. For specifics, use `search_memories` with a real question, never a bare project name.
- **After** a decision, a bug fix (with its root cause), a convention or a gotcha, call `store_memory` with `title`, `type` (decision, bugfix, discovery, pattern, config, architecture, feature, session_summary…), `project`, and `content` (What / Why / Where / Learned). Use `topic_key` only for topics that evolve. It upserts **org-wide**, so namespace it (`nexus-mind/auth-model`).
- **Before saying done** on a multi-step task, store a `session_summary`. Deleting memories happens only when the user asks.

## Where to read more

| Need | Read |
|---|---|
| Domain, roles, invariants, glossary, KPIs, gaps | skill `nexus-mind-business` |
| Design tokens, components, rules, audit script | skill `nexus-mind-design`; `apps/admin/DESIGN.md`, `apps/landing/DESIGN.md` |
| Exact CI steps per app, machine quirks | skill `nexus-mind-verify` |
| Screenshots, e2e smoke, acceptance stories | skill `nexus-mind-qa` |
| Investigating a bug | skill `nexus-mind-triage` |
| Backend layout and patterns | `apps/backend/AGENTS.md`, `docs/ARCHITECTURE.md` (partly aspirational) |
| Admin specifics | `apps/admin/CLAUDE.md`, `apps/admin/PRODUCT.md`, `apps/admin/dev/README.md` |
| Running locally | `docs/RUNNING.md`, `QUICKSTART.md` |
| API surface | `apps/backend/src/api/router.rs` (authoritative), `docs/API_SPEC.md` |
| Factory | `docs/factory/` (mostly Spanish), `apps/backend/src/factory/` |

## Working here

- **Before the first edit:** `git fetch origin`, then work in your own worktree from `origin/main`. The sibling clones (`nexus-mind-*`) sit on old branches with other people's uncommitted work, so don't touch them.
- **Prove it works:** run the `nexus-mind-verify` steps for every app you touched, plus `nexus-mind-qa` for anything visible. This box has no Rust toolchain: say so, and rely on the CI Backend job.
- **Ship:** conventional commits with a scope (`fix(factory): …`, `feat(admin): …`). The PR body uses Problem / Change / verification, and visible admin changes include screenshots, because the owner reviews the admin before it ships. `main` needs one approving review.
- **Autonomous work:** `/nexus-mind-evolve`, which picks a `status:approved` issue, builds it, verifies it and opens a PR. Humans approve issues and merge.
- **Harness changes** go in their own `chore(harness): …` PR. Replace stale text instead of adding next to it.
