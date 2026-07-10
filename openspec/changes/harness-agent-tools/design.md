# Design: Harness Agent Tools

## Technical Approach

Add an agent-facing harness integration layer to the `nexusmind-mcp` sibling repo without granting any new authority: every tool is a thin, permissioned wrapper over already-shipped backend endpoints (`harness-sharing` + `harness-format-variants`), plus one genuinely new client-side capability — a **two-phase local installer** that turns an immutable, hash-pinned manifest into a user-confirmed diff and then materializes files to disk. The backend keeps its no-local-mutation boundary intact: it returns manifests and approvals, never touches the filesystem. All new local logic (destination resolution, diff computation, file materialization, secret scanning, manifest building) lives in the MCP process, which is the only component with filesystem access.

The only backend change is Phase 0: swap `opencode`→`cursor` in the single manifest valid-targets list and the two admin surfaces that reference it, making `cursor` a first-class compatibility target end-to-end while keeping `claude` and `codex`.

## Architecture Decisions

| Option | Tradeoff | Decision |
|---|---|---|
| Two-phase `plan`→`apply` installer | Two round-trips and a plan token to thread through, but writes are structurally impossible until the user has seen a diff. | Adopt. `plan_harness_install` computes a diff and writes nothing; `apply_harness_install` writes only after explicit confirmation. |
| MCP tools vs a standalone CLI | A CLI would duplicate auth/transport and live outside the agent session. | MCP tools — the agent already runs in Claude/Codex/Cursor with the Bearer client; installs happen where the agent is. |
| Cursor as a first-class target replacing `opencode` | Existing `opencode`-targeted rows become invalid (prod-data risk), but Cursor is what users actually run and `opencode` has no consumer. | First-class `cursor` (user-chosen). `opencode` removed from valid-targets; existing rows migrated operationally. |
| Reuse backend approval/hash gate vs new install-lock logic | Reusing means the plan token must carry `manifest_hash` and re-approve on drift, but it avoids a second security surface. | Reuse `approve_install` (hash + warning acknowledgement) as the single enforcement point. |
| Client-side secret scan before upload | Regex scanners have false negatives, but the alternative is shipping raw local content to the backend. | Scan locally, refuse on hit, never transmit raw secrets/paths; backend re-validates `secret_scan_status`. |
| Per-tool destination resolver with an applicability matrix | More upfront mapping, but prevents installing a Claude-only skill into Cursor's rule dir. | Explicit format→tool matrix; refuse unsupported pairs at plan time. |

## Data Flow

```text
recommend_harnesses / list_harnesses ──metadata only──> (no writes, no manifest)
        │
        ▼ user picks a harness+version+tool
plan_harness_install ──download (hash-pinned manifest)──> resolve destinations
        │                                                 + compute per-file diff
        └── returns { manifest_hash, diff[], warnings[] }  (NO WRITES)
        │
        ▼ user reviews diff and confirms
apply_harness_install ── approve_install(hash, ack) ──> materialize files ──> record_install_result
        │                    (re-approves if hash drifted)    (path-traversal guard, atomic)
        └── returns { written[], skipped[], approval_id, result_status }

build_harness_manifest_from_path ──read local files──> sha256 + inline(≤64KiB) + secret-scan
        └── refuse on hit ─┐
                           ▼ (on pass) valid schema 1.1 manifest
        create_harness ──> publish_harness_version
```

## Component Map

New MCP module layout in `../nexusmind-mcp/src/`:

| Module | Responsibility |
|---|---|
| `harness/client.ts` (or extend `client.ts`) | Typed Bearer-token calls to `/v1/harnesses*` + `/v1/harness-*` endpoints. |
| `harness/matrix.ts` | The format→tool applicability matrix + per-tool destination resolver (pure functions, no I/O). |
| `harness/plan.ts` | `planInstall(manifest, tool, scope)` → diff entries; reads existing dest files to compute action + on-disk sha256. |
| `harness/materialize.ts` | `applyPlan(diff)` → atomic writes with path-traversal defense; returns written/skipped report. |
| `harness/build.ts` | `buildManifestFromPath(path, format, targets)` → sha256, inline, folder-walk, schema-1.1 assembly. |
| `harness/secretscan.ts` | Regex-based local secret scanner shared by build + config-review paths. |
| `index.ts` | `server.tool()` registrations for the 10 new tools, matching existing handler shape. |

## 1. Two-Phase Installer Architecture

The installer is deliberately split so that **writes are unreachable from the plan phase**: `plan_harness_install`'s module (`harness/plan.ts`) imports only read + hash utilities, never `fs.writeFile`. The plan output is a pure data structure the agent renders for the user; only `apply_harness_install` imports `harness/materialize.ts`, which is the only module that opens files for writing.

### `plan_harness_install`

Input:
```ts
{ harness_id: string, version: string, target_tool: 'claude'|'codex'|'cursor', target_scope: 'user'|'project' }
```

Steps:
1. Call `downloadHarnessVersion(harness_id, version)` → `{ manifest, manifest_hash, approval_required }`. (Backend already gates this behind a persisted approval; for a first-time plan the flow uses the version *preview* endpoint `getHarnessVersion`, which returns the manifest with content for preview without approval — see backend test `version_manifest_is_readable_for_preview_without_approval`. `plan` uses the preview read; `apply` performs the approval-gated `download`.)
2. Validate `target_tool` against the format via the applicability matrix (§2). Unsupported → refuse with a clear reason, no diff.
3. Resolve each manifest component (and folder entry) to a concrete destination absolute path via the resolver.
4. For each destination, read the existing on-disk file (if any), compute its sha256, and derive the action.
5. Return the diff plus warnings (executable/plugin) and the `manifest_hash` captured from the preview.

Output:
```ts
{
  harness_id: string,
  version: string,
  target_tool: 'claude'|'codex'|'cursor',
  target_scope: 'user'|'project',
  manifest_hash: string,            // threaded to apply; the approval gate key
  format: HarnessFormat,
  requires_acknowledgement: boolean, // true for hook / claude_code_plugin
  warnings: string[],
  diff: DiffEntry[]
}
```

### DiffEntry (the load-bearing structure)
```ts
interface DiffEntry {
  destination: string          // ABSOLUTE resolved path, e.g. /Users/x/.claude/skills/foo/SKILL.md
  relative_path: string        // manifest-relative path (traversal-checked), for display
  action: 'create' | 'overwrite' | 'skip'
  sha256: string               // manifest component sha256 (sha256:hex)
  existing_sha256?: string     // on-disk sha256 if a file already exists
  size_bytes: number
  executable: boolean          // true → chmod +x on write (hook .sh)
  warning?: string             // e.g. "installs an executable hook", "modifies settings.json"
}
```
Action rules: no existing file → `create`; existing file with identical sha256 → `skip`; existing file with different sha256 → `overwrite` (surfaced prominently in the rendered diff).

### `apply_harness_install`

Input:
```ts
{
  harness_id: string, version: string,
  target_tool: 'claude'|'codex'|'cursor', target_scope: 'user'|'project',
  manifest_hash: string,               // from plan
  warning_acknowledged?: boolean,      // required when plan.requires_acknowledgement
  overwrite_confirmed?: boolean        // required when any diff entry action === 'overwrite'
}
```

Steps:
1. Re-download the manifest via the approval-gated path. First call `approve_install(harness_id, version, { target_tool, target_scope, manifest_hash, metadata: { warning_acknowledged } })`. The backend rejects executable formats without `warning_acknowledged=true` (test `executable_approval_requires_warning_acknowledgement_metadata`).
2. `downloadHarnessVersion(...)` now succeeds (approval persisted) and returns `manifest` + `manifest_hash`.
3. **Hash-mismatch / re-approval gate:** if the freshly downloaded `manifest_hash !== input.manifest_hash`, the version drifted between plan and apply. Abort the write, return a `hash_mismatch` result instructing the agent to re-run `plan_harness_install`; the new plan produces a new hash and the user re-approves. Because approval is keyed on `manifest_hash`, a stale approval never authorizes a changed manifest.
4. Materialize files via `harness/materialize.ts` (§5).
5. Call `record_install_result(harness_id, version, { approval_id, manifest_hash, status: 'installed'|'failed', metadata: { changed_files_count } })`. Never send raw file contents (backend test `install_result_records_status_without_local_file_contents`).

Output:
```ts
{
  approval_id: string,
  manifest_hash: string,
  result_status: 'installed' | 'failed' | 'hash_mismatch',
  written: Array<{ destination: string, action: 'create'|'overwrite', size_bytes: number }>,
  skipped: Array<{ destination: string, reason: 'unchanged' }>,
  errors?: Array<{ destination: string, message: string }>
}
```

**Why writes are impossible in plan:** the plan handler and its module never import the materializer; its return type is data-only; and the destination resolver is a pure function. The single filesystem-write module is reachable only from `apply`, which cannot run without a `manifest_hash` that had to come from a prior plan the user reviewed, plus a fresh backend approval keyed on that hash.

## 2. Per-Tool Destination Resolver — Format→Tool Applicability Matrix

`target_scope` selects the root: `user` → the tool's home config dir; `project` → the repo-local dir (`.claude/`, `.cursor/`, or `.codex/` under cwd). The table below shows the `user` scope home paths; project scope substitutes the repo-local root. Cells marked **unsupported** are refused at plan time.

| Format \ Tool | Claude Code (`~/.claude/`) | Codex (`~/.codex/`) | Cursor (`.cursor/` / `~/.cursor/`) |
|---|---|---|---|
| `agent` | `~/.claude/agents/<name>.md` | `~/.codex/agents/<name>.md` (conservative default) | `~/.cursor/rules/<name>.md` (agent-as-rule) |
| `skill` | `~/.claude/skills/<name>/SKILL.md` (+ folder entries under `~/.claude/skills/<name>/`) | **unsupported** (Claude-centric) | **unsupported** (Claude-centric) |
| `command` | `~/.claude/commands/<name>.md` | `~/.codex/prompts/<name>.md` (conservative default) | **unsupported** (no stable slash-command dir) |
| `hook` | `~/.claude/hooks/<name>.sh` + register in `~/.claude/settings.json` | **unsupported** (hook contract undocumented) | **unsupported** |
| `output_style` | `~/.claude/output-styles/<name>.md` | **unsupported** (Claude-centric) | **unsupported** (Claude-centric) |
| `claude_code_plugin` | plugin JSON under `~/.claude/plugins/` + register in `~/.claude/settings.json` | **unsupported** | `.cursor/mcp.json` entry (MCP-style plugin) |
| `theme` | `~/.claude/themes/<name>.json` | **unsupported** | **unsupported** |

Resolution notes:
- **Claude Code** is the reference consumer: markdown formats map to their dedicated dirs; `hook` and `claude_code_plugin` require a **settings.json merge** (register the hook command / plugin entry) in addition to the file write, which is why those rows carry an executable/settings warning.
- **Cursor** is first-class but consumes a narrower surface: `agent`/rule-shaped markdown → `rules/`; MCP/plugin-shaped JSON → `mcp.json`. Skill/output_style/theme/hook are Claude-only concepts with no Cursor destination, so they are unsupported (refused, not silently dropped).
- **Codex** destinations are under-documented; we ship a **conservative documented default** (`~/.codex/agents/`, `~/.codex/prompts/`) only for the markdown formats we can place safely, and mark everything ambiguous (hook, plugin, theme, skill, output_style) unsupported until upstream docs stabilize. This is intentional under-reach: better to refuse than to write to a guessed path.

Justification for unsupported cells: `skill`, `output_style`, and `theme` are Claude Code product concepts with no equivalent install target in Codex or Cursor; `hook`/`claude_code_plugin` require tool-specific registration semantics we will not guess for Codex. The matrix is the single source of truth; both `plan` and `apply` consult it and refuse identically.

## 3. MCP Tool Signatures

All handlers follow the existing `server.tool(name, description, zodShape, async handler)` pattern and return `{ content: [{ type:'text', text }] , isError? }`, matching `store_memory`/`search_memories`. Read tools render metadata; install/create tools render structured JSON in the text block.

```ts
// ── Read tools ──
recommend_harnesses: { target: z.enum(['claude','codex','cursor']).optional() }
  → text list of { harness_id, version, name, targets, format, approval_required, warning_metadata }

list_harnesses: { target: z.enum(['claude','codex','cursor']).optional(), owner_user_id: z.string().optional() }
  → text list of visible harnesses with owner metadata

get_harness_version: { harness_id: z.string(), version: z.string() }
  → manifest preview (format, targets, components summary, manifest_hash) — NO local writes

list_harness_config_reviews: { status: z.string().optional() }
  → text list of shared config reviews (redacted snapshots only)

// ── Install tools ──
plan_harness_install: {
  harness_id: z.string(), version: z.string(),
  target_tool: z.enum(['claude','codex','cursor']),
  target_scope: z.enum(['user','project']).default('project'),
} → { manifest_hash, format, requires_acknowledgement, warnings[], diff: DiffEntry[] }   // writes nothing

apply_harness_install: {
  harness_id: z.string(), version: z.string(),
  target_tool: z.enum(['claude','codex','cursor']),
  target_scope: z.enum(['user','project']).default('project'),
  manifest_hash: z.string(),
  warning_acknowledged: z.boolean().optional(),
  overwrite_confirmed: z.boolean().optional(),
} → { approval_id, manifest_hash, result_status, written[], skipped[], errors? }

// ── Create / upload tools ──
build_harness_manifest_from_path: {
  path: z.string(),                                   // local dir or file to package
  format: z.enum(['agent','skill','command','hook','output_style','claude_code_plugin','theme']),
  targets: z.array(z.enum(['claude','codex','cursor'])).min(1),
  source: z.string(),                                 // provenance.source label
} → { manifest, secret_scan_status: 'passed', component_count } | refuse on secret hit

create_harness: {
  slug: z.string(), name: z.string(),
  description: z.string().optional(),
  project_id: z.string().optional(),
  visibility: z.string().optional(),
  owner_user_id: z.string().optional(),
} → { id, slug, owner_user_id }

publish_harness_version: {
  harness_id: z.string(), version: z.string(),
  manifest: z.record(z.any()),                        // schema 1.1 object from build tool
  manifest_hash: z.string().optional(),
} → { id, version, manifest_hash }

create_harness_config_review: {                       // optional, Phase 4
  source_tool: z.enum(['claude','codex','cursor']),
  config_path: z.string(),                            // local config to redact + preview
} → { redacted_config, redaction_report, content_hash } (local preview) then upload on confirm
```

## 4. New Client Methods (`../nexusmind-mcp/src/client.ts`)

Follow the existing `request<T>()` helper (Bearer auth, JSON, error mapping). Add typed methods:

| Method | Verb + Path | Params → Return |
|---|---|---|
| `listHarnesses` | `GET /v1/harnesses?target=&owner_user_id=` | filters → `Harness[]` |
| `recommendHarnesses` | `GET /v1/harness-recommendations?target=` | target → `HarnessRecommendation[]` |
| `getHarnessVersion` | `GET /v1/harnesses/:id/versions/:version` | ids → `HarnessVersion` (manifest preview) |
| `downloadHarnessVersion` | `GET /v1/harnesses/:id/versions/:version/download` | ids → `HarnessDownloadResponse` (approval-gated) |
| `approveHarnessInstall` | `POST /v1/harnesses/:id/versions/:version/approval` | `{ target_tool, target_scope, manifest_hash, metadata }` → `HarnessApproval` |
| `recordHarnessInstallResult` | `POST /v1/harnesses/:id/versions/:version/install-result` | `{ approval_id, manifest_hash, status, metadata }` → `HarnessApproval` |
| `createHarness` | `POST /v1/harnesses` | `CreateHarnessRequest` → `Harness` |
| `publishHarnessVersion` | `POST /v1/harnesses/:id/versions` | `{ version, manifest, manifest_hash? }` → `HarnessVersion` |
| `listHarnessConfigReviews` | `GET /v1/harness-config-reviews?status=` | status → `HarnessConfigReview[]` |
| `createHarnessConfigReview` | `POST /v1/harness-config-reviews` | `{ source_tool, redacted_config, redaction_report, content_hash, status }` → `HarnessConfigReview` |

New TS types mirror the backend DTOs (`Harness`, `HarnessVersion`, `HarnessRecommendation`, `HarnessDownloadResponse`, `HarnessApproval`, `HarnessConfigReview`) with the manifest typed as the schema-1.1 shape from the format-variants design.

## 5. Local File Materialization Module (`harness/materialize.ts`)

`applyPlan(diff: DiffEntry[]): { written[], skipped[], errors[] }`. Rules:

1. **Path-traversal defense (defense-in-depth).** Manifest paths are already relative-only and traversal-checked server-side (`validate_safe_manifest_path` rejects `/`, `~`, `..`, `C:\`, `/users/`, `.ssh`, `.env`). The materializer re-checks every `relative_path` before resolving: reject absolute paths, `..` segments, and any resolved path that escapes the tool root. `assert(resolved.startsWith(rootRealPath + sep))` after `path.resolve(root, relative_path)`. Refuse the whole apply if any entry fails — no partial write on a poisoned manifest.
2. **sha256 verification.** Before writing, recompute sha256 of the inline `content` and assert it equals the manifest component `sha256`. Mismatch → abort (the download is corrupt or tampered).
3. **Atomicity.** Write each file to a sibling temp file (`<dest>.nm-tmp-<rand>`) then `rename` into place (atomic on same filesystem). `mkdir -p` the parent dir first. `chmod 0o755` when `executable` is true (hooks), else `0o644`.
4. **settings.json merges** (hook/plugin registration) are read-modify-write: parse existing JSON, merge the new hook/plugin entry idempotently, write atomically via the same temp+rename. A settings merge is itself a DiffEntry with `warning`.
5. **Reporting.** Return exactly what was written (destination, action, size), what was skipped (unchanged sha256), and any errors — mapping back to the `record_install_result` `changed_files_count`. Never surface or transmit file contents.

Folder components expand to one write per entry; each entry inherits the same traversal + sha256 checks.

## 6. Secret-Scan + Manifest Build (`harness/secretscan.ts` + `harness/build.ts`)

**Scanner checks** (regex-based, run on every file's content before inlining):
- API-key/token patterns: `sk-`, `nm_`, `ghp_`, `AKIA…`, generic `[A-Za-z0-9_\-]{32,}` near `key|token|secret|password`.
- Private key blocks: `-----BEGIN (RSA|EC|OPENSSH|PRIVATE) KEY-----`.
- `.env`-style assignments: `SECRET|PASSWORD|TOKEN|API_KEY = <value>`.
- Local-path leakage: absolute paths, `/Users/…`, `.ssh`, `.env` (mirrors backend `validate_safe_manifest_path`/content rules).

**Refuse-on-hit behavior:** any hit → return an error listing categories and file names (never the matched secret value); do **not** build or upload a manifest. This is a hard stop, matching the backend which rejects `secret_scan_status: "failed"`.

**Manifest assembly** (`build.ts`), producing a valid schema-1.1 object:
1. Walk `path` (file → single component; dir → folder walk preserving relative paths).
2. For each file: read bytes, compute `sha256: "sha256:"+hex`, `size_bytes = byteLength`.
3. **Inline rule:** if `size_bytes ≤ 64 KiB`, embed `content` inline; the backend `require_file_metadata` enforces `size_bytes` and `sha256` match the inline `content`, and `validate_safe_manifest_content` rejects `content > 64*1024`. Files over 64 KiB are refused with a clear message (this change inlines only; large-blob upload is out of scope).
4. Build `components` matching the format template (e.g. `agent` → single `.md` `file`; `skill` → `.md` file or `folder` with `entries`; `claude_code_plugin` → `plugin_marketplace` `.json` with object content; `theme` → `theme_json`), so the backend `validate_manifest_component` format check passes.
5. Assemble the top-level manifest:
```jsonc
{
  "schema_version": "1.1",
  "format": "<format>",
  "targets": ["claude", ...],
  "components": [ ...sha256/size/media_type/content... ],
  "provenance": { "source": "<source label>" },
  "security": {
    "requires_approval": true,
    "executable": true,            // only for hook / claude_code_plugin
    "secret_scan_status": "passed"
  }
}
```
6. Return the manifest for `publish_harness_version`. The backend re-runs `validate_typed_harness_manifest`; the client build mirrors it so publish never 422s on a clean input.

## 7. Backend Phase 0 — `opencode` → `cursor`

Exact, minimal edits:

| File | Change |
|---|---|
| `apps/backend/src/models/types.rs` (~line 1661) | In `validate_typed_harness_manifest`, change `matches!(v.as_str(), Some("claude" \| "codex" \| "opencode"))` → `Some("claude" \| "codex" \| "cursor")`. |
| `apps/backend/src/models/types.rs` (tests) | Update any manifest fixture/test asserting `opencode` is a valid target to `cursor`; add a test asserting `opencode` now fails `missing_targets`. |
| `apps/admin/src/pages/Harnesses.tsx:940` | `<option value="opencode">OpenCode</option>` → `<option value="cursor">Cursor</option>`. |
| `apps/admin/src/pages/Harnesses.tsx:546` | Approval copy "…Claude, Codex, OpenCode, shell…" → "…Claude, Codex, Cursor, shell…". |
| `apps/admin/src/types.ts:676` | `targets: Array<'claude' \| 'codex' \| 'opencode'>` → `'cursor'`. |

**Prod-data migration note (operational, not code in this change):** any already-published `harness_versions` whose `manifest_json.targets` contains `opencode` will fail re-validation after this swap (validation runs on publish, so persisted rows are not re-checked on read — but downloads/recommendations filtering by `target=cursor` will not match them, and any republish fails). Before/with rollout, run a data `UPDATE` replacing `opencode`→`cursor` in affected `targets` arrays, or archive those rows. Count and remediation confirmed at rollout. This mirrors the rollback risk: reverting restores `opencode` and any `cursor`-only rows created in the interim then need the reverse update.

Landing copy (`apps/landing/*.astro`, 4 refs) is explicitly **out of scope** and tracked separately.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| **Single-phase install** (download → write in one call) | Removes the mandatory diff-before-write guarantee; a compromised or drifted manifest could mutate `~/.claude/settings.json` or drop an executable hook with no user review. The two-phase split makes writes structurally unreachable without a reviewed plan + hash-keyed approval. |
| **Standalone CLI** instead of MCP tools | Duplicates auth/transport, runs outside the agent session, and can't participate in the agent's approval UX. MCP keeps installs where the agent already is, reusing the Bearer client. |
| **Treat Cursor as a Claude-format consumer** (install Claude layouts into `.cursor/`) | Cursor has a distinct config surface (`mcp.json`, `rules/`); dumping Claude skill/hook layouts there would produce dead files. The user chose Cursor as a **first-class target replacing `opencode`**, so it gets its own resolver column and a narrower, correct applicability set. |
| **Guess full Codex install destinations** | Codex install dirs are under-documented; guessing risks writing to wrong paths. We ship a conservative documented default for markdown formats only and refuse ambiguous formats via the matrix. |
| **Server-side file materialization** | Would break the no-local-mutation boundary that the whole security model depends on. The backend stays write-free; only the MCP process touches the filesystem. |
| **Backend-side secret scan only** | The backend never sees raw local content by design; scanning must happen locally before anything is inlined/uploaded. Backend re-validates `secret_scan_status` as a second gate. |

## Testing Strategy

| Layer | What to test | Approach |
|---|---|---|
| Backend unit | valid-targets accepts `cursor`, rejects `opencode`; existing safe-path/size caps unchanged. | Rust tests in `types.rs`. |
| Resolver | Matrix returns correct dest per (format, tool, scope); unsupported pairs refuse. | Pure-function unit tests, no I/O. |
| Plan | Diff actions (create/overwrite/skip) from mocked on-disk sha256; writes nothing. | Vitest with mocked `fs` reads; assert no write calls. |
| Materialize | Path-traversal refusal (`..`, absolute, escaping root); atomic temp+rename; chmod on executables; sha256 mismatch abort. | Vitest against a temp dir. |
| Apply gate | Hash-mismatch → `hash_mismatch` no write; executable without ack → approval refused; result recorded without file contents. | Vitest with mocked client + backend contract. |
| Build/scan | Secret hit refuses; inline ≤64 KiB, >64 KiB refused; produced manifest passes backend `validate_typed_harness_manifest`. | Vitest; cross-check against a Rust validator fixture. |
| Admin | `cursor` option renders; approval copy updated. | Vitest + Testing Library. |

## Migration / Rollout

MCP tools are additive — unregister them to remove the agent surface with zero backend impact. Phase 0 ships with the operational `opencode`→`cursor` data update for any affected published rows. Roll out backend Phase 0 first (so `cursor` validates), then the MCP tools.

## Open Questions

- [ ] Codex `command`/`agent` destination dirs (`~/.codex/prompts`, `~/.codex/agents`) are a conservative default; confirm against upstream Codex docs before widening the matrix.
- [ ] Cursor `agent`→`rules/` mapping fidelity (frontmatter translation) — start with a verbatim copy, refine if Cursor rule format diverges.
- [ ] Whether `apply_harness_install` should support a dry-run replay of the plan hash without a fresh approval when nothing drifted (optimization, not required for correctness).
