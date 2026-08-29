# Pending — issue #265: landing refleja el producto actual

## Done
- Contexto cargado desde NexusMind (convenciones + memorias del landing: waitlist-only, sin pricing, landing en español).
- Verificado contra el código qué está realmente shippeado (no se vendió humo):
  - **SDD** — `apps/backend/src/api/sdd.rs`, rutas `/v1/sdd/{artifacts,changes,specs,search}` + revisiones por artefacto; dual-persistence vía `openspec/config.yaml` (`artifact_store: nexusmind`); admin `/sdd`.
  - **Tasks** — `apps/backend/src/api/tasks.rs`, `/v1/tasks` con subtasks, assignees, labels, comments, spec-links; admin `/tasks`.
  - **Harnesses** — `apps/backend/src/api/harnesses.rs`, `/v1/harnesses` (versions/publish/download/approval/install-result) + `/v1/harness-recommendations`; targets soportados **solo** Claude Code, Codex y Cursor (OpenCode fue removido como target de harness).
  - **Agentes autónomos** — worker `apps/backend/src/bin/autonomous_worker.rs`, templates `github_issue_resolver`, `lead_generation`, `qa`; siempre entregan borradores (draft PR), nunca mergean ni deployan.
- **Hero.astro** — segunda línea del H1 → «Necesitas el control plane que las une.» y subcopy reescrito nombrando los cinco pilares. Badge, CTAs, stats, IDs de animación y canvas intactos.
- **Features.astro** — 11 cards (antes 9), mismo patrón `{ icon, title, desc }` + `.feature-card`:
  - nuevas: `📐 Artefactos SDD`, `✅ Tasks de Equipo`;
  - reescritas: `🌐 Tool-Agnostic Plugins` → `🧰 Harnesses Compartidos`, `🤖 Multi-Agent Orchestration` → `🤖 Agentes Autónomos` (se quitó el name-drop no verificado de CrewAI/Cline y la mención a OpenCode como target de harness);
  - las 7 restantes sin tocar.
- **Solution.astro** — los 4 pills del nodo central pasan a 7 pilares (Memoria · Artefactos SDD · Tasks · Harnesses · Agentes Autónomos · Policy Engine · Audit Trail) y la línea de cierre se amplía. Fila de herramientas, flechas, BYOM y clases intactas.
- Revisión adversarial del diff (subagente en contexto fresco): PASS — sintaxis Astro/JSX válida, 11 entradas bien formadas, sin clases/tokens/hex/ids/CTAs nuevos, sin secciones ni imports nuevos, anclas del Navbar siguen resolviendo, copy en español correcto.

## Left to do
- **`npm ci && npm run build` en `apps/landing` NO se pudo ejecutar**: el sandbox de esta sesión bloquea `npm` y `apps/landing/node_modules` no existe. Hay que correrlo antes de mergear (CI ya lo hace: `.github/workflows/deploy.yml` → `cd apps/landing && npm ci && npm run build`). Los cambios son solo copy + datos de cards sobre markup existente, así que el riesgo es bajo, pero queda sin verificar.
- Falta QA visual en desktop / mobile / `prefers-reduced-motion` (no hubo navegador disponible). Mirar sobre todo la última fila del grid de Features: ahora son 11 cards en `lg:grid-cols-3`, así que la fila final queda con 2.
- Opcional, fuera del alcance de este issue: `Pricing.astro` (hoy **no** se renderiza) todavía dice «Multi-agent orchestration» y lista OpenCode como plugin; si alguna vez se reactiva, hay que alinearlo con el nuevo framing.

## Preguntas abiertas del issue — cómo se resolvieron
1. **Idioma**: se mantiene **español**; el landing es ES-only y unificar a inglés excede el alcance acotado.
2. **Cards vs. pilares**: se agregaron 2 cards nuevas y se reescribieron 2 existentes que ya se solapaban (9 → 11), en vez de reagrupar los 9 items — reagrupar era un rediseño, explícitamente fuera de alcance.
3. **Naming**: se tomó el naming oficial del producto (admin nav + rutas de API): **SDD / Changes / Specs / Artefactos**, **Tasks**, **Harnesses**, **agentes autónomos**.
