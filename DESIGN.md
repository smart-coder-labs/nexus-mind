# NexusMind design records

Design is scoped to each application. Use the current records rather than the
September 2026 blue/glass snapshot originally proposed in PR #271:

- [Admin design](apps/admin/DESIGN.md): Audit1 Lime, Geist and shadcn New York.
- [Landing design](apps/landing/DESIGN.md): SmartCoder lime, Heebo and light/dark themes.
- [Backoffice tokens](apps/backoffice/src/index.css): internal operations,
  amber actions, readable neutral text and reduced-motion support.

The application records and runtime tokens are authoritative. Do not restore
retired landing components, the blue palette or the old root design sidecar.
Keep existing routes, permissions, data contracts and real chart data intact.

Accessibility floor: WCAG 2.2 AA. Text must clear 4.5:1 on its actual background;
controls need accessible names and motion must respect the user's preference.
