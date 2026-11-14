# Motion

Source: `src/styles/signal.css`, identical in admin and backoffice (PR #464). Legacy utilities (`animate-fade-in`, `--ease-apple`) use the same curve and stay.

| Token / class | Value | Use |
|---|---|---|
| `--ease-signal` | `cubic-bezier(0.16, 1, 0.3, 1)` (expo-out) | every entrance; also a Tailwind `ease-signal` |
| `--ease-signal-std` | `cubic-bezier(0.2, 0.7, 0.2, 1)` | data that changes length |
| `.sig-reveal` | 1.1s, from `translateY(21px) scale(.985) blur(6px)`; siblings stagger 90ms, capped at the 5th | stat cards, card grids on mount |
| `.sig-bar` | gradient green → lime; `width`/`height` 0.8s `--ease-signal-std` | a bar whose length is the datum |
| `.sig-card` | border-color 0.2s, lime edge on hover | stat cards |
| `.sig-live` | lime dot plus a 1.6s ping | only while data is actually streaming, next to a text label |
| `.sig-hud` | always-dark immersive surface (haze plus grid) | one hero widget per page at most |

Existing timings stay: entrances 220ms, badges 160ms, sidebar 200ms, sheets 500/300ms. Framer Motion components read `useReducedMotion()`.

## Rules

- **Reduced motion.** Under `prefers-reduced-motion: reduce`, `signal.css` removes the `sig-reveal`/`sig-live` animations and the `sig-bar`/`sig-card` transitions. The global guard in each app's `index.css` shortens everything else. Any new keyframes or transition needs its own counterpart (D-MOT-01).
- **What to animate.** Animate `transform` and `opacity`. Never loop `width`, `height`, `top` or `left`. No bounces on entrances. No scroll-jacking.
- **Live data.** It pings only while it is live. A stale or finished run shows no `sig-live`.
- **Charts.** No decorative entry animation.
