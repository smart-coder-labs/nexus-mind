# NexusMind design records

Design is scoped to each application. Use the current records rather than the
September 2026 blue/glass snapshot originally proposed in PR #271:

- [Admin design](apps/admin/DESIGN.md): Signal with the SmartCoder palette
  (ink structure, forest-green actions, lime as accent only), Geist and shadcn
  New York, light and dark.
- [Backoffice tokens](apps/backoffice/src/index.css): the same Signal system and
  palette, dark-only, using the admin's dark values; readable neutral text and
  reduced-motion support.
- [Landing design](apps/landing/DESIGN.md): SmartCoder lime, Heebo and light/dark themes.

Admin and backoffice share one Signal layer, `src/styles/signal.css`, kept
byte-identical in both apps (accent block, glass levels, motion, phi spacing and
the golden type scale). Lime (`#c3d82e`) is the signal accent only: focus,
live data, highlights and gradient ends, never a large fill or body text.

The application records and runtime tokens are authoritative. Do not restore
retired landing components, the blue palette, the all-lime admin or the amber
backoffice, or the old root design sidecar.
Keep existing routes, permissions, data contracts and real chart data intact.

Accessibility floor: WCAG 2.2 AA. Text must clear 4.5:1 on its actual background;
controls need accessible names and motion must respect the user's preference.
