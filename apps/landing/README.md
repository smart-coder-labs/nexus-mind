# NexusMind landing

The canonical open-source landing uses the owner-approved SmartCoder/Globant lime identity. It has light/dark themes, a local interactive 3D mesh, pointer deformation, click waves and native-scroll chapter animations. `/smartcoder` redirects to `/` for compatibility with the design preview.

## Development

Requires Node >=22.12.0. Run `npm install`, then `npm run dev` (Astro, normally port 4321). `npm run build` creates the static output in `dist/`.

## Source

- `src/pages/index.astro`: page content and semantic structure.
- `src/styles/smartcoder.css`: isolated lime design tokens and responsive styles.
- `src/scripts/smartcoder.ts`: theme, menu, disclosures and GSAP scroll orchestration.
- `src/scripts/hero-mesh.ts`: locally projected 3D mesh, pointer response and pause/visibility lifecycle.
- `src/lib/features.ts`: eleven product capabilities.
- `src/lib/repository.ts`: canonical open-source destinations.
- `PRODUCT.md` and `DESIGN.md`: product truth and the selected visual system.

There is no waitlist or hosted form. Repository, installation guide, issues, pull requests and MIT license are real links. Content is visible without JavaScript; reduced-motion preferences keep the layout static. Chapter entrances settle once. The canvas stops when paused, offscreen or hidden.

Fonts and conceptual images originate in the owner's SmartCoder design pack. `public/smartcoder/Heebo-OFL.txt` and `image-provenance.json` retain license and provenance. The admin is a separate app and has a separate review/push boundary.
