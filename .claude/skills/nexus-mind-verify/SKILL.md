---
name: nexus-mind-verify
description: The verification gate for nexus-mind — the exact steps .github/workflows/ci.yml runs per changed app (backend, nexus-cli, admin, backoffice, factory image, e2e smoke), in order, plus the design audit and this machine's quirks (no Rust toolchain, Node 26 default vs CI Node 20). Use before saying a change works, before committing, and as the verify step of the nexus-mind-evolve loop.
---

# Verify

CI (`.github/workflows/ci.yml`, on every PR to `main`) uses a paths filter and runs **only the jobs for the apps you touched**. Run the same jobs locally, in the same order, from the repo root. Stop at the first failure, fix, re-run from that step.

## Per app

| Touched | Steps (CI order) |
|---|---|
| `apps/backend/**`, `scripts/reset-demo.sh`, `scripts/test-mcp.sh` | `cargo build --manifest-path apps/backend/Cargo.toml` → `cargo test --manifest-path apps/backend/Cargo.toml` → `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` → **e2e smoke** (below) |
| `apps/nexus-cli/**` | `cargo test --manifest-path apps/nexus-cli/Cargo.toml` → `cargo clippy --manifest-path apps/nexus-cli/Cargo.toml -- -D warnings` |
| `apps/admin/**` | `cd apps/admin && npm ci` → `npm run build` (`tsc -b && vite build`) → `npm run test` (vitest + jsdom) |
| `apps/backoffice/**` | `cd apps/backoffice && npm ci` → `npm run build` → `npm test` (`scripts/check-accessibility.mjs`: contrast of `text-quaternary`, reduced-motion block, five search `aria-label`s) |
| `apps/backend/factory-commands.Dockerfile` | arm64 image build on CI only; don't try it here |
| any web-app file | `node .claude/skills/nexus-mind-design/scripts/design-audit.mjs` (not in CI; 0 MUST to pass) |
| visible change | `nexus-mind-qa` skill (screenshots) |

Not covered by CI: `apps/landing` (check with `cd apps/landing && npm ci && npm run build`), `apps/migrator-tui`, `apps/bootcamp-tracker`. Say so in the PR when you touch them.

While iterating, narrow first: `cargo test --manifest-path apps/backend/Cargo.toml <test_name_filter>`, `cd apps/admin && npx vitest run src/pages/Memories.test.tsx`.

## E2E smoke (backend changes)

```bash
cargo build --release --manifest-path apps/backend/Cargo.toml
SKIP_BUILD=1 ./scripts/reset-demo.sh                 # seeds apps/backend/data/nexusmind.db
(cd apps/backend && ./target/release/nexusmind &)    # :8080
curl -sf http://localhost:8080/v1/health
NEXUSMIND_API_KEY=nm_demo_acme_sarah ./scripts/test-mcp.sh
./scripts/test-e2e.sh                                # tenant isolation, audit, CRUD (not in CI, worth it for auth/tenancy changes)
```

Demo keys are public seed values from `apps/backend/src/bin/seed.rs`, not secrets.

## This machine

- **No Rust toolchain** (`cargo` is not installed). Backend and nexus-cli steps can't run locally: say "backend not verified locally, relying on CI" in the PR and watch the Backend job (`gh pr checks <n> --watch`). Don't install a toolchain without asking.
- **Node 26 is the default; CI uses Node 20.** Run admin/backoffice steps with `mise exec node@20 -- npm …` when a result differs or before trusting a pass.
- RAM is tight (~4–5 GB free with the usual apps open). Check `free -m` before `vite build` or a release cargo build in Docker.
- Several sibling checkouts (`nexus-mind-*`) hold uncommitted work. Verify in your own worktree only.

## Reporting

List what ran and what it returned, per step. A skipped step gets a reason ("cargo unavailable"). If something fails that you think is unrelated, show it failing on `origin/main` too before saying so.
