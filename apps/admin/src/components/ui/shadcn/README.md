# Audit1 / shadcn primitives

These local primitives follow Audit1 UI Commons **v2.0.670** (`47968e54`), the
version consumed by Audit1 Admin portal `e9e01b2a`. The reference checkout is
`/Volumes/external/Documents/nexu-loop-agents/audit1-ui-reference`.

- New York style, neutral base, Radix behavior, Lucide icons and Geist.
- Brand roles are translated through `src/styles/audit1-theme.css`; no Audit1
  runtime package, backend, credentials or business modules are imported.
- `components.json` points shadcn additions to this directory.
- Existing capitalized component paths remain compatibility adapters. Extend
  these adapters without changing route API contracts.
- The Select adapter encodes values internally because existing filters use
  an empty string as a real “All” option. Callers and API requests still receive
  the original value, including the empty string.
- The table container adds horizontal overflow for narrow admin viewports.
- `/design-system` is a development-only palette/component preview.

Visual parity still requires current desktop/mobile comparison captures;
source matching and unit tests do not certify pixel-perfect fidelity.

Chart and Collapsible primitives also come from the same reference. Chart keeps
Recharts 2.15.4 compatibility; local adaptations use the project helper path,
Tailwind 4 CSS-variable syntax, Geist tooltip numbers, and visible keyboard focus.
The analytical panels provide accessible data tables and skip entry animation.
