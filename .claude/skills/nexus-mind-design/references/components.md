# Components to reuse (admin)

Paths are under `apps/admin/src/`. Import with the `@/` alias (`@/components/ui/...`). Check here before writing markup.

## Two layers

- `components/ui/shadcn/*` — lowercase Audit1/shadcn New York primitives (button, card, chart, checkbox, collapsible, input, popover, select, separator, sheet, sidebar, skeleton, switch, table, tooltip). `components.json` sends new `npx shadcn add` output here. Read `components/ui/shadcn/README.md` first.
- `components/ui/<Capitalized>/` — compatibility adapters used by the routes (Button, Card, Badge, Input, Select/StyledSelect, Modal, Table, EmptyState, Skeleton, Spinner, Switch, Radio, SegmentedControl, Combobox, DateRangePicker, Markdown, Sparkline, StatisticDisplay, KpiMarquee, ThemeToggle, Sidebar, Toast). Extend these without changing their props: many routes depend on them.

## By need

| Need | Use | Notes |
|---|---|---|
| Button | `ui/Button` (maps onto shadcn variants) | default (lime) / secondary / outline / ghost / link / destructive |
| Status label | `ui/Badge` variant `success|warning|error|info|primary|default` | `purple` is a legacy alias of brand-link |
| Count / KPI | `ui/MetricSummary` (static label + value + caption) | `pages/dashboard/StatTile.tsx` wraps it with an optional sparkline |
| Categorical colour for the n-th item | `accentFor(i)` in `pages/dashboard/colors.ts` | stable order across routes |
| Chart | `ui/shadcn/chart.tsx` `ChartContainer`, `ChartTooltipContent`, `ChartLegendContent`; `ui/CountChart.tsx` for a single count series | no entry animation; add a data table |
| Small trend | `ui/Sparkline` | |
| Data table | `ui/Table` (`Column<T>` API) | overflow stays inside the table container |
| Empty result | `ui/EmptyState` (`title`, `description`, `icon`, `action`) | one per panel, with a next action |
| Loading | `ui/Skeleton` (suspense fallbacks per chart, see `pages/Dashboard.tsx`), `ui/Spinner` for buttons | |
| Dialog | `ui/Modal` + `ModalHeader/ModalTitle/ModalDescription` (Radix, needs `ariaLabel`) | `components/ConfirmModal.tsx` for confirm/danger |
| Select | `ui/Select` | encodes `''` internally so "All" filters keep working; callers still get `''` |
| Markdown | `ui/Markdown` (react-markdown + remark-gfm) | memory/SDD content |
| Command palette | `components/CommandPalette.tsx` | |
| Graphs | `components/OrgMemoryGraph.tsx`, `components/CodeGraph.tsx`, `components/graph/*`, `pages/graph/*` | force-graph-3d; escape labels with `escapeHtml` from `lib/utils.ts` |
| Class merging | `cn()` from `lib/utils.ts` | |

`ui/Toast` exists but no `ToastProvider` is mounted; don't assume toasts render. Feedback today is inline (status text near the action).

## Shell

`components/Layout.tsx` owns the sidebar (permission-filtered, collapsible, mobile sheet below 768px), the 56px topbar and the inset workspace. Nav visibility also depends on `config/disabled-sections.ts` (`VITE_ADMIN_PROFILE` = `full` | `only-context`): add a new route there and in `App.tsx`, behind the right route guard (`ProtectedRoute`, `AdminRoute`, `SuperUserRoute`).

`/design-system` (`pages/DesignSystem.tsx`) is a dev-only palette and component preview. Use it to eyeball tokens.

## Backoffice and landing

The backoffice has no component library: pages in `apps/backoffice/src/pages/` use token classes directly and `components/Layout.tsx`. Copy the nearest page's markup. The landing is two Astro pages (`src/pages/index.astro`; `/smartcoder` redirects to `/`) with copy in `src/lib/features.ts` and repo URLs in `src/lib/repository.ts`.
