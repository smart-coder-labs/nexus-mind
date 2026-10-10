# Landing motion iteration — 2026-10-09

Reference direction: [Senthora](https://senthora.ai/) for an animated sculptural wireframe, [Lazy](https://lazy.so/) for scroll-linked composition. Preserve NexusMind content, dark surfaces and blue accent; admin design remains unchanged in this iteration.

Motion thesis: independent tools converge into the shared control plane. The hero introduces that connected shape through a locally rendered, rotating and deforming 3D mesh. The next section assembles tool connections, capabilities and models as the visitor scrolls. Supporting sections use restrained entrances.

Implementation:
- Canvas projects 3D geometry at a capped 30fps; reduced mesh density and pixel ratio on mobile. Stops when hidden/offscreen and supports manual pause.
- Existing GSAP handles one scroll-linked sequence with CSS sticky positioning. Uses native document scrolling, with no wheel/touch interception or Lenis.
- Mobile and reduced-motion layouts show the complete diagram without an extended sticky sequence. Content is visible before scripts execute.
- Consolidated the previous two reveal systems and removed initially hidden hero markup. Confetti also respects reduced motion.
- No new dependency or remote animation asset.

Validation:
- Landing production build passes; TypeScript check of both new motion modules passes.
- Desktop hero and live mesh rendered; pause toggled to its selected state.
- Scrolling changed connection transforms/progress and completed the assembly.
- Mobile at 390px: 382px document width, no horizontal overflow, normal diagram layout. Mobile navigation reached waitlist at 100px below viewport top.
- Browser reduced-motion emulation was blocked by the user's saved raw-CDP preference. That path was reviewed in code, not visually certified.
- An existing LastPass DOM injection caused a waitlist React hydration warning; React recovered and the form remained visible. No form was submitted.

Local availability: admin :3005, read-only fixtures :8788 and landing :4321 were restarted in detached processes after discovering all previous dev servers had stopped. Admin Memories rendered again. Preview is still sample data, not production.

Screenshots: landing-hero-desktop.png, landing-connections-desktop.png, landing-mobile-form.png.

## Cursor and navigation follow-up

Revisited Senthora's live background and its public main.js: the interaction combines local magnetism, trailing illumination, pointer-speed intensity, camera parallax and a click ripple. Implemented equivalent interaction principles in NexusMind's existing local canvas: a bounded attraction field, lit vertices, damped pointer tracking, stronger depth movement and a decaying background-click wave. Hero cursor uses an immediate ring/dot indicator and expands over links/buttons. Native cursor returns on exit or pause. Reduced motion and coarse pointers do not activate the custom cursor; no scroll event is intercepted.

Verified in-browser that the pointer indicator tracked the actual coordinates, the mesh changed visually, and pausing changed the canvas to static and removed the custom cursor. Production build and motion-module TypeScript check passed.

Replaced the floating boxed navbar with Senthora's compact translated-page layout: logo on the left, small menu control and pill CTA on the right. Uses native details/summary disclosure with actual NexusMind anchors, Escape focus return, outside-click and link-selection dismissal. No placeholder language switcher was added. Desktop opening/Escape passed; at 390px the dropdown remained within the viewport (92–362px), and choosing Características reached #features and closed the menu. Final landing production build passed.
