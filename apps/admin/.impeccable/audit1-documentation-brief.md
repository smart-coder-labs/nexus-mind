# Audit1 admin documentation refresh

Reference: Admin-portal-front e9e01b2a; UI Commons v2.0.670 / 47968e54. Scope: admin DESIGN.md and schemaVersion 2 design sidecar. The owner selected Audit1 Admin, shadcn New York, and lime first. The landing is outside this scope. Admin changes remain local pending owner review; no admin push is authorized.

Source evidence: src/styles/audit1-theme.css; src/index.css; src/components/Layout.tsx; src/components/ui/shadcn/{button,card,input,sidebar,sheet}.tsx; Input, Button, Card, Badge and Modal compatibility components; src/pages/dashboard/StatTile.tsx; KpiMarquee.css; PRODUCT.md. This refresh replaces the obsolete Heebo/black-sidebar/landing-derived rules with implemented Audit1 tokens and geometry.

Validation status: source extraction complete; browser capture and visual comparison pending. The reviewer disposition is RECAPTURE because browser checks were unavailable and no authorized screenshots were obtained. This document does not certify pixel-perfect parity. Final validation: TypeScript and production Vite build passed; all 365 tests in 46 files passed. The build retains a large-chunk advisory for the application/3D graph bundles. Local admin and /design-system returned HTTP 200. Visual fidelity remains unverified; test/build success is not a substitute for the missing captures.

Not canonized or repaired: isolated compact domain typography, residual legacy comments and route styling exceptions, and tiny topbar metadata. The selected Geist family and subtle two-axis workspace grid are explicit Audit1 reference decisions, not generic detector defects. The Migrations border treatment needs contextual review rather than automatic global adoption. Detector findings based on the obsolete DESIGN.md are not evidence of a visual pass or failure under the refreshed system. No detector rerun was performed.

Preview: http://127.0.0.1:3005/design-system is a development-only component and palette surface. Visual snippets in design.json illustrate primitives; they do not certify runtime interactions or every route.
