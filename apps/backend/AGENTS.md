# apps/backend

The NexusMind API and Factory control plane: Rust 2021 (stable toolchain), Axum, rusqlite (bundled SQLite), fastembed (nomic-embed-text), tree-sitter for code chunking. Crate `nexusmind`; main binary `nexusmind`, plus `nexusmind-seed`, `backfill-embeddings` and the tools in `src/bin/`.

## Layout

| Path | What |
|---|---|
| `src/api/router.rs` | every route; `build()` / `build_with_store()`; the `auth` and `audit` middleware layers are applied here |
| `src/api/<area>.rs` | handlers, one file per area (memory, sdd, tasks, harnesses, factory*, workflow*, internal…) |
| `src/api/helpers.rs` | `require_permission`, `require_explicit_permission`, visibility checks, `AppJson`/`JsonBody` extractors (both normalise body errors to a structured `ApiError`), `hidden_resource_not_found`, pagination (`MAX_LIST_LIMIT` 500) |
| `src/api/middleware.rs` | auth (cookie `nexusmind_session` then Bearer, SHA-256 key lookup) and the audit layer (`AUDIT_SKIP_PATTERNS`) |
| `src/db/migrations.rs` | `run_all` + `run_vN` (current 111), guarded by `PRAGMA user_version` |
| `src/db/queries.rs` | most SQL (≈25k lines — grep for the function, don't read it whole); other `db/*.rs` hold area queries |
| `src/store/sqlite.rs` | `SqliteStore` wrapping `Arc<Mutex<Connection>>` |
| `src/models/types.rs` | request/response types, `AuthContext`, `ApiError`, `UserRole` |
| `src/factory/`, `src/automation/` | Factory engine, sandbox/egress, policy engine, publication, merge gate, agent runtimes |
| `src/indexer/`, `src/retrieval/`, `src/embed/` | code index and search |
| `src/migration/` | knowledge migration (Claude memories, repo docs, git history, DB schemas) |
| `src/crypto.rs` | AES-256-GCM for tokens at rest |
| `tests/` | HTTP integration tests (`tower::ServiceExt::oneshot` against `build()` with an in-memory DB) |

## Rules

- Tenant is `AuthContext.org_id`, never request input. Every new query takes `org_id`; project-scoped reads check membership with the helpers. Return `hidden_resource_not_found` (404, audited) for rows outside the caller's scope: a 403 would confirm they exist.
- Permission checks go in the handler before any read or write: `require_permission` (privileged roles bypass it) or `require_explicit_permission` for `autonomous_agent:*` / `factory_policy:*` (no bypass, fails closed).
- Mutations are audited by the middleware. If you log your own audit row, add the route to `AUDIT_SKIP_PATTERNS` so it isn't logged twice.
- Schema: add `run_v112` (next number) and call it from `run_all`; make it transactional; to change a CHECK constraint, rebuild the table (`*_v111` pattern). Never edit an existing `run_vN`.
- Use `AppJson`/`JsonBody` instead of `axum::Json` for request bodies, to keep the error model consistent.
- Secrets go through `crypto.rs`; never log them or include them in `ApiError` messages.
- Clippy runs with `-D warnings` in CI (without `--all-targets`, so test code isn't linted, but keep it clean anyway).

## Test

```bash
cargo test --manifest-path apps/backend/Cargo.toml <filter>
cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings
```

CI clones with full history because `migration::git_history` has a calibration test that walks this repo's real commits; a shallow clone fails it. Strict TDD (`openspec/config.yaml`): write the failing test first. Every data endpoint gets a cross-tenant test. This machine has no cargo; see the `nexus-mind-verify` skill.

## Run

`cd apps/backend && cargo run` (reads `data/nexusmind.db` relative to `apps/backend`, port 8080). Seed with `scripts/reset-demo.sh`. Set `COOKIE_SECURE=false` for browser logins over plain HTTP.
