---
version: 1
slug: "pages-factory-workflows-workflowspage-tsx-323e692e"
primary_target: "apps/admin/src/pages/factory/workflows/WorkflowsPage.tsx"
related_targets: ["apps/admin/src/pages/factory/workflows/RuntimeEditor.tsx"]
---

# Workflows — guided workspace operation

Mode: Operate. Scope: Factory workspaces, runtime configuration, delivery, sources and execution details; retain existing admin identity and permission boundaries.

## Direction contract

THESIS: one workspace, one understandable next action. Replace the wall of unrelated forms and raw JSON with task navigation and explicit readiness checks.

OWN-WORLD: inherit Audit1 Lime, Geist, neutral surfaces, shared controls and semantic status colors. No new palette or application shell.

STORY: the operator selects Kasymir, sees runner and configuration state, configures agents/models, actual checks and test services, then starts a bounded task and opens its evidence by URL. Internal clean-context review is distinct from GitHub approval; author self-approval is never implied.

FIRST VIEWPORT: page title and workspace selector, horizontal task navigation, one readiness explanation and its primary action. Repositories are searchable compact rows, not 49 stacked cards. Execution links open a focused detail above configuration.

FORM: prescribed extension of the established Operate workflow, no visual-world roll. The compact confirmation launcher exited 2 (headless); infer these task priorities from the user's explicit request. Code-led, no image-generation tool.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

Constraints: show a local preview before publishing admin changes. Real private credentials and approved runtime images remain operator-controlled; missing capabilities stay blocked rather than looking configured. Do not change production authorizations or execute paid models during design verification.

## Finish review

Verdict: implementation verified locally; owner visual approval is pending. No push or deployment is authorized by this review.

Impeccable review/documenter roles were performed inline, without delegation. Existing `apps/admin/DESIGN.md` is preserved: this is an ordinary extension, not a new visual world.

System summary:
- Palette: inherited neutral surfaces, restrained lime actions and semantic success/status roles.
- Typography: self-hosted Geist; existing mono only for code and identifiers.
- Layout: workspace-first task navigation, compact inventory and progressive disclosure of phase details.
- Named rules: retain Action Ink, Semantic Pair, Geist Interface and Quiet Depth.
- Controls: shared Button/Input/Badge, explicit labels, native focus, disabled write actions and error/retry states.

Not canonized: no route-specific dimensions or UI workflow choices were promoted into global design tokens. The wider application's incumbent English navigation is outside this task's Spanish workflow copy.

Evidence:
- Browser review: baseline, implementation, then one confirmation round; desktop 1440, tablet 1024 and mobile 390. Five task surfaces checked, no page JavaScript errors or page-width overflow in the illustrated fixture. Primary next action moved above the checklist for mobile visibility; check layouts respond to content width.
- UI: 23 focused tests passed, TypeScript check and production build passed. Formatting checked for workflow source files.
- Backend: 1,855 library tests passed, 4 ignored; 23 relevant workflow/backup/API integration tests passed, including runtime owner-versus-runner authorization and revision conflicts.
- Python: 52 tests passed with an installed immutable Docker fixture image. New multi-repository suite ran tests and bytecode builds on two final candidate commits, with distinct artifact names and missing-coverage rejection.
- No paid model, device, live Kasymir fix or production rollout was exercised. Runtime configuration remains a prerequisite, not a passing QA claim.

Operator guide: `docs/factory/CONFIGURACION_ADMIN.md`. Preview scaffolding: `apps/admin/dev/workflows-preview-{data,proxy}.mjs`; all API writes are rejected and fixtures are explicitly labelled.
