# Exploration: harness-sharing

## Scope

NexusMind should let authorized users publish, review, download, and explicitly install reusable AI tooling harnesses: agents, skills, MCP servers, recommended CLIs, configuration snippets, compatibility targets for Claude/Codex/OpenCode, and optionally a redacted reviewable export of a user's local Claude configuration.

## Current State

- NexusMind is already a multi-tenant control plane: all core records are `org_id` scoped, users authenticate through API keys/cookies, and requests receive an `AuthContext` from `apps/backend/src/api/middleware.rs`.
- RBAC is string-permission based through roles and `require_permission` in `apps/backend/src/api/helpers.rs`. Built-in permissions cover memories, projects, policies, conventions, collections, code, backups, webhooks, tags, and audit, but there is no harness-specific permission yet.
- Project visibility exists through `projects` and `project_members`; non-privileged users only see project-scoped records for projects they belong to. Policies and conventions already have project-scoped visibility patterns.
- Existing reusable knowledge primitives are close but not sufficient:
  - Memories store free-form content and metadata, with collections, tags, pinning, archive, import/export, and search.
  - Conventions store authoritative rules agents receive as context.
  - Policies evaluate model whitelist, budget limit, and PII redaction only; not install/download approval.
  - Agents today are mostly API-key identities in the admin UI, not full runnable agent/harness definitions.
- Admin UI patterns already exist for list/create/edit/delete flows, confirmation modals, downloads, imports, role permissions, policies, conventions, agents, collections, and backups.
- Existing integration docs cover Claude Code and Cursor through MCP. The documented Claude installer (`npx nexusmind-setup`) modifies `~/.claude/settings.json`, hooks, and shell env vars. Cursor uses `.cursor/mcp.json` or `~/.cursor/mcp.json`. The repo has `.mcp.json` for NexusMind itself. There is no local config export/import service for Claude, Codex, or OpenCode in this repo.

## Affected Areas

- `apps/backend/src/db/migrations.rs` — add harness persistence tables and permissions seed/backfill.
- `apps/backend/src/models/types.rs` — add typed request/response/domain structs for harnesses, versions, artifacts, compatibility targets, install manifests, redaction reports, and approval records.
- `apps/backend/src/db/queries.rs` — implement org/project-visible list/get/create/update/version/download/install-state queries.
- `apps/backend/src/api/router.rs` — add `/v1/harnesses*` routes.
- `apps/backend/src/api/helpers.rs` and role seeding — add `harness:read`, `harness:write`, `harness:download`, `harness:install`, and possibly `harness:review_config` permissions.
- `apps/backend/src/api/policy.rs` / `apps/backend/src/policy/mod.rs` — later extension point for policy-gated installation, secret scanning, and target allowlists.
- `apps/backend/src/api/audit.rs` and blanket audit middleware — ensure publish/download/approve/install/export actions are auditable with safe metadata only.
- `apps/admin/src/api/client.ts` and `apps/admin/src/types.ts` — add typed client methods and UI types.
- `apps/admin/src/pages/*` / `components/*` — add a Harness Library page and possibly a Claude Config Review page using existing table/card/modal/download patterns.
- `docs/CLAUDE_CODE_PLUGIN.md`, `docs/CURSOR_PLUGIN.md`, `.mcp.json` — update integration guidance once installer/export flows exist.
- Published MCP/setup package (`@smart-coder-labs/nexusmind-mcp` / `nexusmind-setup`) — likely external to this repo, but required for actual local installation into Claude/Codex/OpenCode.

## Domain Model Recommendation

Use first-class harness tables instead of overloading memories or collections.

```text
harnesses
  id, org_id, project_id NULL, slug, name, description, visibility,
  status=draft|published|archived, created_by, created_at, updated_at

harness_versions
  id, harness_id, version, changelog, manifest_json, manifest_hash,
  created_by, created_at, published_at, revoked_at

harness_permissions / reuse project_members + RBAC
  optional per-harness grants if project/org visibility is not enough

harness_installations
  id, harness_version_id, org_id, user_id, target_tool,
  target_scope=global|project, status=pending|approved|installed|failed|revoked,
  approval_token_hash, installed_at, metadata_json

harness_config_reviews
  id, org_id, user_id, source_tool=claude|opencode|codex,
  redacted_config_json, redaction_report_json, content_hash,
  status=draft|shared|discarded, created_at
```

Manifest shape should be explicit and target-aware:

```json
{
  "schema_version": "2026-07-07",
  "name": "Rust Backend Reviewer",
  "targets": ["claude", "opencode", "codex"],
  "compatibility": {
    "claude": { "min_version": null, "config_paths": ["~/.claude/settings.json"] },
    "opencode": { "config_paths": ["~/.config/opencode/opencode.jsonc"] },
    "codex": { "config_paths": [] }
  },
  "components": {
    "agents": [],
    "skills": [],
    "mcp_servers": [],
    "hooks": [],
    "cli_recommendations": [],
    "config_snippets": []
  },
  "provenance": {
    "created_by_user_id": "...",
    "source": "manual|config_import|template",
    "manifest_hash": "sha256:..."
  },
  "security": {
    "requires_user_approval": true,
    "secret_scan_status": "passed|failed|unknown",
    "redactions": []
  }
}
```

## Approaches

1. **First-class Harness Library** — add dedicated backend tables, API, admin UI, and MCP/local installer commands.
   - Pros: explicit permissions, versions, audit, install state, security review, and future marketplace/distribution model.
   - Cons: larger first slice; requires installer work outside this repo for full value.
   - Effort: Medium/High.

2. **Harnesses as Collections/Conventions** — store harnesses as memory collections plus conventions and downloadable JSON attachments.
   - Pros: fastest prototype; reuses search, tags, project visibility, and export patterns.
   - Cons: weak schema, weak versioning, hard to enforce compatibility, permissions, provenance, and install approvals.
   - Effort: Low.

3. **Config Export First** — begin only with Claude config review/share and later generalize to harnesses.
   - Pros: directly addresses the user's strongest concrete ask; useful security boundary discovery.
   - Cons: risks building a Claude-specific importer that does not generalize cleanly to Codex/OpenCode harnesses.
   - Effort: Medium.

## Recommendation

Build the **First-class Harness Library**, but slice it narrowly:

1. Backend-only harness catalog with published version manifests and download endpoint.
2. Admin UI to create/publish/read/download harness manifests.
3. MCP/tool recommendation endpoint that can say “a relevant harness exists” but returns only metadata and requires explicit user approval before download/install.
4. No direct local writes in the backend. Installation must happen in a local CLI/MCP tool that presents a diff and waits for explicit approval.

This keeps the domain correct from day one while avoiding unsafe remote mutation of a developer's local Claude/OpenCode/Codex configuration.

## Security and Privacy Boundaries

- Full Claude config review/share is high risk. `~/.claude/settings.json`, hooks, MCP env blocks, shell profiles, and related files may contain API keys, bearer tokens, local usernames, absolute paths, private repo names, command arguments, and internal URLs.
- Sharing must use a local exporter that performs deterministic redaction before upload. Required redactions: env var values matching secret patterns, tokens/keys/passwords, absolute home paths, machine-specific paths, local binary paths, private hostnames if configured by policy, and hook command arguments marked sensitive.
- The user must see a diff/review screen before upload and before install. Default action should be “do not share/install”.
- Install must be a local operation with explicit approval, ideally two-step: download manifest → show planned file changes/diff → apply after confirmation. NexusMind should never silently write to local agent config.
- Harness manifests should support provenance and hashes. Downloaded manifests should be immutable by version and auditable.
- Policy checks should eventually gate target tool, MCP package source, command allowlists, and whether remote packages can run through `npx`.

## Product Questions / Assumptions

- Is a harness owned by an org, a project, or both? Recommendation: org-owned with optional `project_id` scope.
- Are harnesses private-only or can orgs publish marketplace/public harnesses later? Recommendation: private org library first; design visibility enum for later.
- Does “configure into Claude/Codex/OpenCode” mean generate instructions or mutate local config files? Recommendation: first slice downloads manifests; local installer handles mutations after approval.
- Which config files are in scope for “full Claude config”: `~/.claude/settings.json`, project `.mcp.json`, hooks, skills, agents, shell env, or all of them? Recommendation: start with settings + MCP servers + hooks; never upload raw shell profiles.
- Should skill bodies be copied into harnesses or referenced by package/source URL? Recommendation: support both, but require provenance and hash for copied content.

## Risks

- Secret leakage from local configs if redaction is incomplete.
- Remote code execution risk through MCP server commands, `npx`, hooks, or CLI recommendations.
- Permission mismatch: existing `policy:read` for members is broad; harnesses need dedicated permissions and project visibility.
- Installer scope lives partly outside this repo; backend/admin can publish/download but cannot safely mutate local Claude/OpenCode/Codex configs alone.
- Versioning and rollback can become confusing if mutable “latest” manifests are installed without recording the exact manifest hash.
- Codex/OpenCode config formats may differ or evolve; target adapters should be isolated.

## Ready for Proposal

Yes. Proposal should define the first slice as a safe harness catalog + manifest download + explicit approval contract, with Claude config review/export as a separate but related capability behind redaction and preview.
