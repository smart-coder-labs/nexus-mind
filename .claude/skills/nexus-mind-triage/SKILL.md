---
name: nexus-mind-triage
description: Investigate a reported nexus-mind bug down to its layer, read-only — reproduce it through the same surface (admin, backoffice, MCP tool, plugin, Factory run), capture the exact HTTP call and response, decide whether the UI, the backend route, the MCP server or the deployment is wrong, and write the issue or fix plan. Use when someone reports "X shows wrong data / fails / leaks / doesn't save", for a failing QA story, or before fixing any bug you haven't reproduced.
---

# Triage

A bug isn't understood until you have watched it happen and named the layer that is wrong. This skill only reads. The fix goes through `nexus-mind-evolve` or a normal branch.

## 1. Reproduce through the reporter's path

- Same surface, same role, same org. An MCP-tool bug reproduces with the tool call (or the REST call the tool makes), not through the admin.
- Capture every `/v1/...` request and response involved (Playwright `page.on('response')`, or `curl -sv -H "Authorization: Bearer <key>"`), console errors, and a screenshot of the wrong state. Setup is in `nexus-mind-qa`.
- Use the demo seed locally. Against production use only read calls, and only with the reporter's consent; never copy customer memories or code into an issue.
- If you can't reproduce it, say what you tried. Don't fix blind.

## 2. Find the layer

| Evidence | Layer | Next |
|---|---|---|
| Response is right, UI shows something else | admin/backoffice | fix in `apps/<app>/src` |
| UI faithfully shows a wrong response | backend route/store | find the handler via `apps/backend/src/api/router.rs`, then the store/module it calls |
| 403/401 where the role should be allowed (or 200 where it shouldn't) | permissions | the role/permission checks in `apps/backend/src/api/helpers.rs`, and the route guard in `apps/admin/src/App.tsx`; `nexus-mind-business` `references/roles.md` |
| One org sees another org's rows | tenancy — **critical** | every query must filter by the caller's `org_id`; add a cross-tenant test (`scripts/test-e2e.sh` pattern) |
| REST works, the MCP tool fails or returns a different shape | MCP server (repo `smart-coder-labs/nexusmind-mcp`) | read it at `origin/main`; tool names like `search_memories` live in its `src/legacy.ts`/`src/reduced.ts` |
| Works locally, fails in production | deploy/config | backend runs on the Oracle k3s box (`deploy/oracle/k8s/`, `.github/workflows/deploy.yml`); compare env and image tag, don't touch the cluster |
| Factory run stuck/failed | Factory | read the run's phases and events in the admin (`/factory/runs`) and `docs/factory/` before reading code |

## 3. Write it down

Issue (or PR description if you fix it right away):
- **Context:** one paragraph.
- **Steps:** surface, role, org, actions.
- **Evidence:** screenshot, method + route, status, relevant response excerpt (no keys other than public demo keys, no cookies, no customer content).
- **Layer and why.**
- **Proposed fix** and the test that would have caught it.
- **Acceptance story.**

Labels: `bug` plus the area (`api`, `ux`, `security`, `factory`, `qa`). The repo has no severity labels; state severity in the body — critical (cross-tenant leak, secret exposure, data loss), high (flow blocked), medium (workaround exists), low (cosmetic).

Check `nexus-mind-business` `references/gaps.md` first; if it's already listed, link it instead of rediscovering it.
