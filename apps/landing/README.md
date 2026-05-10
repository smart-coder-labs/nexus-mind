# NexusMind — Landing Page

Landing page for the NexusMind Enterprise AI Platform.

Built with **Astro** + **Tailwind CSS v4**.

## Stack
- Astro 5 (static site generation)
- Tailwind CSS v4
- Vanilla JS (scroll animations, waitlist form, confetti)
- Inter font (Google Fonts)

## Sections
1. Hero — gradient text, staggered entrance, dual CTAs
2. Early Adopters Program — 3 pain-point cards + benefits package + slot counter
3. Social Proof — trusted by logos
4. Problem — fragmentation, lost knowledge, no governance
5. Solution — convergence diagram (Copilot + CrewAI + Mem0 + Retool + Okta → NexusMind)
6. Features — 9 feature cards with hover effects
7. Pricing — 3 tiers: Developer ($29), Team ($49 featured), Enterprise (Custom)
8. Waitlist Form — with interest chips, localStorage persistence
9. Footer

## Interactive Features
- Scroll-triggered reveal animations (IntersectionObserver)
- Staggered hero entrance (opacity + translateY)
- Glass-morphism navbar with show/hide on scroll
- Mobile hamburger menu
- Interest chips (multi-select)
- Confetti on form submit
- Toast notifications

## Development
```bash
npm install
npm run dev     # dev server on localhost:4321
npm run build   # static output to dist/
```
