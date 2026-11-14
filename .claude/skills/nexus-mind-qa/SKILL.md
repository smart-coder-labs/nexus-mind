---
name: nexus-mind-qa
description: Browser and end-to-end checks for nexus-mind — how to get the admin or backoffice running against data (local seeded backend, or the read-only live preview proxy), log in with an API key, capture 1440/390 light/dark screenshots with headless Chromium, and run the backend HTTP smoke scripts. Use for any visible change, for acceptance stories in nexus-mind-evolve, and when reproducing a UI bug.
---

# QA

Two layers. **Layer 1** proves behaviour (the acceptance stories, against an API). **Layer 2** proves the look (screenshots, read against `nexus-mind-design` `references/rules.md`). A visible change needs layer 2; a changed flow needs layer 1 too.

## Get data behind the UI

| Option | When | How |
|---|---|---|
| Local backend + demo seed | backend available (CI box, Docker, a machine with cargo) | see `nexus-mind-verify` §E2E smoke; admin `cd apps/admin && npm run dev` (:3000, proxies `/v1` to :8080); backoffice `cd apps/backoffice && npm run dev` (:3001) |
| Live read-only preview (admin) | this machine (no cargo) | `apps/admin/dev/README.md`: put `VITE_API_URL=`, `ADMIN_PROXY_TARGET=https://api.nexusmind.smartcoderlabs.com`, `VITE_PREVIEW_READ_ONLY=true` in the ignored `apps/admin/.env.development.local`, then `npm run dev -- --port 3005 --strictPort`. The proxy only forwards GET/HEAD/OPTIONS and a POST allowlist (login, logout, memory/code search) — writes return 405 locally. Needs the user's own login; never store credentials in the env file |
| Mocked network | layout-only checks, CI-less | Playwright `page.route('**/v1/**', …)` with fixtures shaped like `apps/admin/src/types.ts`. Label screenshots as mocked |

Demo logins (local seed only): API key tab with `nm_demo_acme_admin` (org admin), `nm_demo_acme_sarah` (member), `nm_demo_techstartup_admin` (other tenant, for isolation stories). The session is an HttpOnly cookie; on plain-HTTP localhost start the backend with `COOKIE_SECURE=false` or the browser drops the cookie and login silently loops (`apps/backend/src/config.rs`, `apps/backend/src/api/auth.rs`).

## Screenshots (layer 2)

Playwright is installed via mise, not in the repo, so a bare `import 'playwright'` fails. Import it by absolute path and use the system Chromium:

```js
// node shots.mjs  — save under the scratchpad, not the repo
const { chromium } = await import(`${process.env.HOME}/.local/share/mise/installs/npm-playwright/latest/node_modules/playwright/index.mjs`)
const browser = await chromium.launch({ executablePath: '/usr/bin/chromium' })
for (const [w, h] of [[1440, 900], [390, 844]]) for (const theme of ['light', 'dark']) {
  const ctx = await browser.newContext({ viewport: { width: w, height: h } })
  await ctx.addInitScript(t => localStorage.setItem('nexusmind-admin-theme', t), theme)
  const page = await ctx.newPage()
  // log in once per context (API key tab) or reuse storageState
  await page.goto('http://127.0.0.1:3005/memories')
  await page.screenshot({ path: `/tmp/…/memories-${w}-${theme}.png`, fullPage: true })
}
await browser.close()
```

The backoffice is dark only (one theme). Capture empty, loading and error states when the change touches them (mock the response to force them).

## Acceptance stories (layer 1)

Write each story as role + action + observable result, then check it through the UI or with `curl -H "Authorization: Bearer <key>"` against `/v1/...`. Always include:
- the empty state;
- one error (403 for a role without permission, or a 4xx validation);
- one tenant-isolation check when data is read or written (TechStartup's key must not see Acme's rows).

`scripts/test-e2e.sh` already covers isolation, audit trail and CRUD for memories; extend the story set instead of duplicating it.

## Report

Per story: pass/fail and the evidence (screenshot path, status code). Say which data option you used. Never paste cookies, API keys other than the public demo ones, or customer data from the live preview into a PR.
