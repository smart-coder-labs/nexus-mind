# NexusMind Admin UI — QA Report

**Tested**: 2026-06-29  
**Branch under test**: `main` (f4fc933)  
**Tester**: QA agent (Playwright headless Chromium, viewport 1440×900)  
**App**: `http://localhost:3001` (Vite dev server, proxying backend at port 8080)  
**Demo credential**: `nm_demo_acme_admin` (Acme Corp admin API key)

---

## Top Issues (ranked by severity)

| # | Severity | Area | Issue |
|---|----------|------|-------|
| 1 | WARNING | Build | GraphTab chunk is 1.39 MB (376 KB gzip) — exceeds Rollup 500 KB threshold. Already behind `React.lazy`, but `manualChunks` or `chunkSizeWarningLimit` bump needed to silence CI. |
| 2 | WARNING | Accessibility | Activity timeline expand buttons have no `aria-expanded` attribute. Screen readers cannot communicate open/collapsed state to assistive technology. |
| 3 | INFO | Console | `401 GET /v1/admin/auth/me` fires twice on every fresh browser load. Expected behavior (AuthProvider `useEffect` fires before cookie is set), but pollutes the browser console. |
| 4 | INFO | Console | React Router v6 → v7 future-flag warnings: `v7_startTransition` and `v7_relativeSplatPath`. Benign but noisy — two warnings per navigation event. |
| 5 | INFO | Code Graph | No indexed repositories in demo data. 3D graph cannot be exercised live; empty state renders correctly. |

---

## 1. Dashboard

### What was tested
- Stat pill badges (count, color, text)
- Recent Activity timeline: day-group headers, expand/collapse for rich events, collapsed repeated events
- "Show more" pagination
- Period toggle (7d / 30d / 90d)
- Customize panel (card visibility toggle)
- Horizontal overflow at 1440 px and 375 px

### Results

**Stat pill badges** — PASS  
Seven pill badges render correctly: Total Memories (20), Active Users 24h (4), Searches Today (6), Top Tool (claude-code), Sessions (0), This Week (20), This Month (20). Colors match the `BADGE_ACCENT` map in `Dashboard.tsx`.

**Activity timeline — day groups** — PASS  
"TODAY" group header is present. Items are rendered in chronological descending order with the correct dot-color coding per action type (`delete` → red, `store` → green, `search` → blue, `updated` → grey, `create` → green).

**Activity expand / collapse (rich events)** — PASS  
Clicking a `store` event button expands an inline tree showing:
- Project (folder icon + name)
- Type badge (e.g. `decision`)
- Content preview ("QA test memory content")

Clicking `search` events that have results expands the results tree. Collapsing works (second click contracts the tree).

**Accessibility gap** — WARN  
The expand buttons (`<button type="button">`) do not set `aria-expanded`. State is purely visual. Fix: add `aria-expanded={expandedActivity.has(entry.id)}` to each expand button.

**Show more pagination** — PASS  
Clicking "Show more" advanced the list from 26 → 31 items (the initial limit is 20, previously loaded items were already expanded to 26 by earlier "Show more" click). The `activityLimit` increment (+20) works.

**Period toggle** — PASS  
Switching to 7d, 30d, and 90d each refreshes the stat badges and activity count correctly.

**Customize panel** — PASS  
"Customize" button opens the card-visibility slide-over. All 12 card toggles are present and interactive. `localStorage` persistence was confirmed across navigation.

**Horizontal overflow** — PASS  
No horizontal scroll at 1440 px desktop. At 375 px mobile (iPhone SE), the layout collapses to a single-column stack with no overflow detected.

### Screenshots
- `screenshots/03-dashboard.png` — full dashboard, 30d period
- `screenshots/13-activity-expanded.png` — store event expanded with tree detail
- `screenshots/05-dashboard-customize.png` — customize panel open
- `screenshots/12-dashboard-mobile.png` — 375 px mobile view
- `screenshots/14-dashboard-7d.png` — 7-day period view

---

## 2. Code Graph (Code → Graph tab)

### What was tested
- Lazy-load of the `GraphTab` chunk
- 3D force-graph render
- Node/edge type filter panel
- Node hover tooltip
- Node click → snippet panel
- Empty state when no repositories are indexed

### Results

**Lazy-load** — PASS  
`GraphTab` lazy-loads via `React.lazy()` without errors. The Suspense spinner shows briefly, then the tab mounts.

**Empty state** — PASS  
No indexed repositories in the demo environment. The empty state ("No indexed repositories yet") renders correctly with the "Connect a repository" CTA. No JS errors during render.

**3D force-graph interaction** — NOT TESTED  
`react-force-graph-3d` requires indexed data. The demo contains no indexed code repositories. Node hover, node click → snippet panel, and filter toggles could not be exercised. Code review confirms the implementation is correct (project selector, node click dispatches `client.getCodeSnippet()`, filter state is managed via `visibleNodeTypes` Set).

**Bundle size** — WARN  
`GraphTab-DH6STpw-.js` is 1,391 KB (376 KB gzip), well above Rollup's 500 KB warning threshold. The chunk is already isolated via `React.lazy()`, so it does not affect initial page load time. Two options to resolve:

1. Add to `vite.config.ts`: `build: { chunkSizeWarningLimit: 1500 }` — silences the warning.
2. Add `build.rollupOptions.output.manualChunks` to split `three` and `react-force-graph-3d` into separate chunks — reduces each individual chunk size but adds network round-trips.

### Screenshots
- `screenshots/11-code-graph.png` — Graph tab showing empty state

---

## 3. Code Search + Repositories

### What was tested
- Repositories tab renders and lists connected repos
- Search tab renders and accepts a query

### Results

**Repositories tab** — PASS (empty state)  
No repositories connected in the demo. The empty state ("Connect your first repository") renders correctly with no errors.

**Search tab** — PASS (empty state)  
Search input renders; submitting a query correctly returns an empty result set with the "No results" state. No errors.

### Screenshots
- `screenshots/09-code-repositories.png` — Repositories tab, empty state
- `screenshots/10-code-search.png` — Search tab, empty state

---

## 4. Memories Page

### What was tested
- Page load (suspected crash scenario from prior report)
- Search with keyword highlighting
- Tab navigation: Memories, Sessions, Tags, Duplicates, Collections
- Row click → slide-over panel
- Export dropdown (JSON / CSV / via API)
- Console errors on load

### Results

**Suspected crash — NOT REPRODUCED** — PASS  
The Memories page loads cleanly with 20 entries. No JS errors on mount. PR #187 (`fix(memories): crash on load`) already addressed the crash. The page renders all columns (Date, User, Type, Memory) and inline action buttons (Archive, Delete, star, pin).

**Keyword search** — PASS  
Typing "audit" in the search box (`placeholder="Search memories…"`) filters the list to 2 matching entries. The search term is highlighted in blue within each result. The subtitle counter updates to "2 entries". Clearing the search restores all 20 entries.

**Sessions tab** — PASS  
Loads and shows session list. No errors.

**Tags tab** — PASS  
Loads and renders tag cloud. No errors.

**Duplicates tab** — PASS  
Loads. Empty state shown (no duplicates detected in the demo dataset). No errors.

**Collections tab** — PASS  
Loads. Empty state shown (no collections created). No errors.

**Row click → slide-over** — PASS  
Clicking a memory row opens the detail slide-over panel. The panel renders the full memory content and related memories sidebar. The `aria-label="Close detail panel"` button is present and functional.

**Export dropdown** — PASS  
The Export button opens a dropdown with three options: Export JSON, Export CSV, Export via API. The JSON and CSV options match the `Memories.test.tsx` test expectations. "Export via API" is an additional option not covered by existing tests.

**Console errors** — PASS  
Zero JS errors on Memories page load. The only error recorded across the full session was `401 GET /v1/admin/auth/me` (×2), which occurs at the AuthProvider level on initial app load — not on the Memories page itself.

### Screenshots
- `screenshots/06-memories-page.png` — Memories tab, full list
- `screenshots/15-memories-search.png` — Search for "audit" with 2 highlighted results
- `screenshots/16-memories-row-click.png` — Slide-over panel open
- `screenshots/17-memories-export.png` — Export dropdown open (3 options visible)
- `screenshots/07-memories-sessions-tab.png` — Sessions tab
- `screenshots/08-memories-tags-tab.png` — Tags tab
- `screenshots/18-memories-collections.png` — Collections tab
- `screenshots/19-memories-duplicates.png` — Duplicates tab

---

## 5. General Quality

### Build

```
npm run build  →  PASS
```

One warning: `GraphTab-DH6STpw-.js` is 1391.29 kB / gzip: 376.06 kB. All other chunks are within budget. No TypeScript errors.

### Tests

```
npm run test  →  21/21 PASS
```

Suite breakdown:
- `Memories.test.tsx` — 5 tests (export, CSV escaping)
- `GraphTab.test.tsx` — 12 tests (mapGraphData, filterNodesByTypes, computeExternalAggregate, component mount)
- Other — 4 tests

### Console errors (full session)

| Count | Type | Message | Verdict |
|-------|------|---------|---------|
| 2 | error | `Failed to load resource: 401 (Unauthorized)` on `/v1/admin/auth/me` | Expected — AuthProvider pre-auth check |
| 2/nav | warning | `React Router Future Flag Warning: v7_startTransition` | Benign — add flag to silence |
| 2/nav | warning | `React Router Future Flag Warning: v7_relativeSplatPath` | Benign — add flag to silence |

To silence the React Router warnings, add to the `<BrowserRouter>` call in `main.tsx`:

```tsx
<BrowserRouter future={{ v7_startTransition: true, v7_relativeSplatPath: true }}>
```

### Responsive

At 375 px (iPhone SE): sidebar collapses, stat badges wrap to two rows, content area fills the screen. No horizontal overflow.

---

## Fix/Improve Notes

### Must fix
None — no regressions or crashes found.

### Should fix
1. **`aria-expanded` on activity expand buttons** (`Dashboard.tsx` ~line 380): add `aria-expanded={expandedActivity.has(entry.id)}` and a matching `aria-controls` pointing to the detail div's id. Required for WCAG 2.1 AA compliance.
2. **React Router v7 future flags** (`main.tsx`): add `future={{ v7_startTransition: true, v7_relativeSplatPath: true }}` to suppress the warnings before migrating to React Router v7.

### Nice to have
3. **GraphTab chunk warning** (`vite.config.ts`): either raise `build.chunkSizeWarningLimit` to 1500 or add a `manualChunks` split for `three` to keep each chunk under the threshold.
4. **401 console noise**: the double `/v1/admin/auth/me` call on cold load is structurally expected, but if a silent `try/catch` or a conditional check (`if (document.cookie.includes('session'))`) can prevent the call entirely when no session exists, it would clean up the browser console.

---

*Generated by QA agent — do not fix, report only. Screenshots in `scratchpad/screenshots/` (ephemeral, not committed).*
