# Proposal: Harness Agent Tools

## Intent

Give NexusMind agents an approval-first way to recommend, install, create, and upload shared harnesses directly from the tools they run in — Claude Code, Codex, and Cursor — by adding harness tools to the `nexusmind-mcp` MCP server. The backend and admin already publish, version, hash, and audit shared harnesses (from `harness-sharing` and `harness-format-variants`), but agents have zero harness tooling today, so the "recommend / download-to-tool / create / upload" loop cannot be closed from an agent session. This change adds that agent-facing integration layer while preserving the existing security boundary: the backend never writes local files, and no local mutation happens without an explicit user-confirmed diff.

## Scope

### In Scope
- MCP read tools: `recommend_harnesses`, `list_harnesses`, `get_harness_version`, `list_harness_config_reviews` — thin, permissioned wrappers over existing backend endpoints.
- MCP install tools (two-step, never silent): `plan_harness_install` (download manifest + resolve per-tool destinations + return a diff, no writes) and `apply_harness_install` (approve → write files → record result, only after user confirmation).
- A per-tool destination resolver/materializer for Claude Code (`~/.claude/`), Cursor (`.cursor/`), and Codex (`~/.codex/`), plus a format→tool applicability matrix.
- MCP create/upload tools: `build_harness_manifest_from_path` (read local files, compute sha256, inline content, secret-scan, build a valid schema 1.1 manifest), plus `create_harness` and `publish_harness_version` wrappers.
- Optional MCP `create_harness_config_review` with local redaction + preview before upload.
- Backend Phase 0: make `cursor` a first-class compatibility target, replacing `opencode` in the manifest validation valid-targets list and the admin surfaces that reference it.
- Executable-format acknowledgement gating for `hook` and `claude_code_plugin` on the install path.
- Secret scan on the create/export path before any upload.

### Out of Scope
- Backend mutation of local Claude/Codex/Cursor files (the boundary is preserved: backend returns manifests/approvals only).
- New backend REST endpoints or data-model changes beyond the single `opencode`→`cursor` target swap.
- Public marketplace distribution.
- Landing/marketing copy referencing `opencode` (`apps/landing/*.astro`) — noted, but not changed here.
- Automatic migration of already-published harnesses that target `opencode` (flagged as a prod-data risk, handled operationally, not by this change's code).
- Full Codex install-destination research beyond a documented conservative default.

## Capabilities

### New Capabilities
- `harness-agent-tools`: Agent-facing MCP tools that recommend, plan/apply installs with mandatory diff preview and per-tool materialization, and create/upload harnesses, all approval-first and with no silent local mutation.

### Modified Capabilities
- `harness-library`: Replace `opencode` with `cursor` as a first-class compatibility target for published manifests and validation.
- `harness-install-approval`: Reaffirm approval-first, diff-before-write, and executable/plugin acknowledgement now that a real MCP installer exists as the enforcing client.
- `harness-config-review`: Reaffirm local redaction + preview when the review is created from an agent session.

## Approach

Add harness tools to `nexusmind-mcp` using the existing `server.tool()` pattern in `src/index.ts` and the Bearer-token client in `src/client.ts`, wrapping the already-shipped backend endpoints (`recommendations`, `download_version`, `approve_install`, `record_install_result`, create/publish, config-reviews). Keep every tool permissioned by the backend's existing `harness:*` scopes — the MCP layer adds no new authority, it exposes existing authority to agents.

The install path is deliberately two-phase and the only genuinely new logic. `plan_harness_install` downloads the immutable manifest, runs a per-tool destination resolver (mapping manifest components + format to concrete local paths for the chosen target tool), and returns a diff plus any executable/plugin warnings — writing nothing. `apply_harness_install` runs only after the user confirms: it records `approve_install` (with manifest-hash and warning acknowledgement), materializes files to disk, then calls `record_install_result`. A **format→tool applicability matrix** governs which formats install to which tools (skill/output_style are Claude-centric; not every format maps to every tool), so unsupported combinations are refused at plan time.

The create/upload path reads local files at a given path, computes sha256 per component, inlines content (≤64KiB per the existing manifest rules), runs a secret scan, and assembles a valid `schema_version` 1.1 manifest (`format`, `targets`, `components`, `provenance`, `security`) before `create_harness` / `publish_harness_version`. Phase 0 is a minimal backend change: swap `opencode`→`cursor` in the single valid-targets list in `apps/backend/src/models/types.rs` and the corresponding admin surfaces, keeping `claude` and `codex`.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `apps/backend/src/models/types.rs` | Modified | Replace `opencode` with `cursor` in the manifest valid-targets list (1 ref, line ~1661) + test update. |
| `apps/admin/src/pages/Harnesses.tsx` | Modified | Replace `opencode` target option/label with `cursor` (2 refs). |
| `apps/admin/src/types.ts` | Modified | Replace `opencode` target type with `cursor` (1 ref). |
| `../nexusmind-mcp/src/index.ts` | New | Register harness read/install/create MCP tools via `server.tool()`. |
| `../nexusmind-mcp/src/client.ts` | Modified | Add typed Bearer-token client methods for harness endpoints. |
| `../nexusmind-mcp/src/` (new module) | New | Per-tool destination resolver/materializer + format→tool matrix + local secret scan. |
| `apps/landing/*.astro` | Noted only | Marketing `opencode` copy (4 refs) — out of scope, tracked separately. |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Silent local mutation / RCE via installed hooks or plugins | High | Two-phase plan→apply, diff before any write, executable/plugin acknowledgement, preserve backend no-write boundary. |
| Secret leakage on create/upload or config review | High | Local secret scan + preview before upload; reject on hits; never store raw local secrets/paths. |
| Already-published harnesses targeting `opencode` become invalid | Med | Flag as prod-data risk; require a data UPDATE of affected rows before/with rollout; validation stays additive-safe. |
| Codex install destinations under-documented | Med | Ship a conservative documented default; refuse ambiguous combinations via the applicability matrix. |
| Format→tool mismatch installs to the wrong place | Med | Enforce the applicability matrix at plan time; refuse unsupported format/tool pairs. |
| Manifest hash drift between plan and apply | Low | Reuse existing hash-mismatch approval gate; re-approve on new hash. |

## Rollback Plan

MCP tools are additive: unregister the harness tools in `nexusmind-mcp` to remove the agent-facing surface with no backend impact. Phase 0's target swap is reverted by restoring `opencode` in the valid-targets list and admin surfaces; any `cursor`-targeted rows published in the interim would then need a compatible data update, mirroring the forward-migration risk.

## Dependencies

- Shipped `harness-sharing` backend/admin (PR #210): harness tables, `harness:*` permissions, REST endpoints, approval-gated downloads.
- Shipped `harness-format-variants`: typed formats, owner metadata, manifest schema 1.1.
- `nexusmind-mcp` runtime: `@modelcontextprotocol/sdk`, `server.tool()` registration, Bearer-token client (sibling repo at `../nexusmind-mcp`).

## Success Criteria

- [ ] Agents can recommend and list accessible harnesses via MCP without downloading or installing.
- [ ] `plan_harness_install` returns a per-tool diff and writes nothing.
- [ ] `apply_harness_install` writes only after explicit user confirmation, records approval + result, and gates executable formats behind acknowledgement.
- [ ] Claude Code, Codex, and Cursor each resolve to correct destinations via the applicability matrix.
- [ ] `build_harness_manifest_from_path` produces a valid schema 1.1 manifest and refuses on secret-scan hits.
- [ ] `cursor` is a first-class target end-to-end; `opencode` no longer validates.

## Assumptions / Deferred Questions

- The format→tool applicability matrix is authored in this change; exact per-format/per-tool cells are refined in spec/design.
- Codex install destinations (`~/.codex/`) start from a conservative documented default pending better upstream docs.
- Phase 4 (`create_harness_config_review`) is optional and may ship after Phases 0–3.
- Operational migration of existing `opencode`-targeted rows is handled outside this change's code path; count and remediation are confirmed at rollout.
- MCP tools add no new authority beyond existing `harness:*` backend permissions.
