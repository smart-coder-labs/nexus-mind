# Exploration — Knowledge Migration (épica)

> **Épica**: `knowledge-migration-*`
> **Fase**: explore
> **Fecha**: 2026-08-15
> **Owner**: backend + tooling

Este documento es el estado verificado del terreno antes de proponer nada. Cada afirmación
tiene evidencia en el árbol; lo que no pude verificar está marcado como pregunta abierta.

---

## 1. Qué se pidió

Construir los sistemas de migración/ingesta de conocimiento pre-NexusMind desde **cinco**
fuentes:

1. Repos git (historia: commits, PRs, tags, ADRs, CHANGELOG)
2. Memorias de Claude Code (`~/.claude/`, `CLAUDE.md`, `AGENTS.md`, memoria de proyecto)
3. Supabase
4. Bases de datos Postgres
5. **Documentos del repo** (`docs/`, `openspec/`, README, specs, tasks, convenciones)

La inferencia que requiera un LLM (clasificar, extraer, resumir, inferir semántica) se hace
con **Claude Code headless** (`claude -p`), no con una llamada del backend a un proveedor.

---

## 2. Estado real: nada de esto existe

Confirmado el 2026-08-14. La ingesta está **explícitamente fuera de alcance** del change que
construyó el company brain:

> `openspec/changes/u2s-client-model/proposal.md:69` — *"**Data migration and ingestion**
> (Claude Code memories, repo `.md`, git/GitHub, Postgres/Supabase schemas) — separate change;
> depends on this one."*

Lo que sí se entregó para u2s (PR #250): modelo de clientes (`run_v58`, `/v1/clients`),
métricas de uso (`run_v59`, `usage_events`, `/v1/usage`) y fixes de seed.

### Planificación previa, nunca implementada

| Artefacto | Id / ubicación | Estado |
|---|---|---|
| Épica con 12 subtareas | `fb64331c-1023-4cda-ae20-a7940a1dbd4a` (2026-07-14) | backlog |
| Design SDD | memoria `8beff8c4-…` (2026-07-19) | escrito, sin implementar |
| Carpeta openspec | `openspec/changes/knowledge-migration/` | **no existe en disco** |

---

## 3. El hallazgo que más ahorra trabajo: la migración v56

`apps/backend/src/db/migrations.rs:422` — **`run_v56` ya creó el esquema completo de staging**,
y es **esquema muerto**: `grep` sobre `apps/backend/src` y `apps/admin/src` no encuentra una
sola referencia fuera de `migrations.rs`. Cero queries, cero rutas en `router.rs`, cero UI.

Lo que v56 ya modela correctamente:

| Tabla | Aporte |
|---|---|
| `migration_runs` | Scope org/proyecto con triggers que abortan si el proyecto no pertenece a la org. Máquina de estados `staging → in_review → committing → completed \| cancelled`. |
| `migration_candidates` | `UNIQUE(run_id, source_identity)` → **idempotencia por procedencia**. `version INTEGER CHECK(version > 0)` → **concurrencia optimista** en la revisión. `provenance_kind IN ('client_attested','verified_manifest')`. |
| `migration_review_actions` | **Append-only forzado por triggers** (`RAISE(ABORT)` en UPDATE y DELETE). Registra `expected_version`/`resulting_version`, actor, autorización y correlation id. |
| `migration_provenance` | `UNIQUE(org_id, destination_kind, source_identity)` → **re-correr no duplica destinos**. `candidate_id ON DELETE RESTRICT`: no se puede borrar el candidato que produjo un destino. |
| `migration_outcomes` | Traza por candidato del resultado de cada intento de commit. |

### Dónde se queda corta

1. **No tiene `client_id`.** v56 es de antes de v58 (modelo de clientes). Para una consultoría,
   una migración que escribe memories sin cliente asociado es exactamente el agujero de
   aislamiento que `u2s-client-model` cerró.
2. **`destination_kind CHECK IN ('memory','convention')`.** No admite `task` ni `sdd_artifact`,
   que sí se pidieron. En SQLite ampliar un CHECK exige **rebuild de tabla**.
3. **No modela la fuente.** No hay `source_kind` (`repo-docs` / `git-history` / `claude-memories`
   / `db-schema`), así que no se puede filtrar ni reportar por conector.
4. **No modela la indexación.** No hay nada que registre si un candidato commiteado quedó
   vectorizado.

**Conclusión**: construir encima de v56 con una `run_v60`, no rediseñar. El diseño conceptual
(staging + review humano + provenance idempotente + versionado optimista) es correcto y es
justo el que pidió la planificación de julio.

---

## 4. Restricción de arquitectura: BYOM

`docs/ENGINEERING_PROCESS.md:14`, principio de ingeniería #4:

> **BYOM (Bring Your Own Model)** — *Nunca dependemos de un proveedor de LLM. El core funciona
> sin LLMs.*

Esto **no bloquea** usar Claude Code headless; lo ubica. La inferencia tiene que correr del
lado cliente, fuera del backend Rust. Coincide además con la realidad física: el contenedor de
Fly.io no tiene el repo del cliente, ni red a su Postgres, ni Claude Code instalado.

- `apps/backend/Cargo.toml` no tiene ninguna dependencia de LLM (solo `reqwest` genérico).
- `claude` CLI verificado en la máquina: **v2.1.233**.
- `apps/mcp` **no existe en este checkout** — el MCP es el paquete npm externo
  `@smart-coder-labs/nexusmind-mcp`, en otro repo.

### El precedente correcto ya está escrito

`apps/backend/src/bin/import_sdd.rs:1-20` resolvió exactamente este problema y documenta el
porqué:

> *"El importador LEE el árbol `openspec/` de disco y ESCRIBE al artifact store. Esas dos
> mitades nunca han estado en la misma máquina: el checkout de un desarrollador tiene
> `openspec/` y no tiene base de datos de producción; el contenedor de Fly.io tiene la base de
> datos y no tiene checkout. Así que la mitad del filesystem debe poder empujar por HTTP."*

El runner de migración es el mismo patrón: **binario local que escanea, infiere con
`claude -p`, y hace POST de candidatos**.

---

## 5. Estado por fuente

### 5.1 Repos git

| | |
|---|---|
| Qué existe | `POST /v1/code/index` (`api/code.rs:357`) acepta `repo_url` (clona) o `root_path`. `src/indexer/{walker,chunker,tree_sitter_chunker}` → chunks + embeddings + grafo de símbolos. Búsqueda en `/v1/code/search`, `/locate`, `/graph`. |
| Qué falta | **Toda la historia**. El walker recorre el árbol de trabajo actual; no lee commits, PRs, tags, blame ni CHANGELOG. Y el destino es el índice de código, no memories/conventions. |
| Reutilizable | La clonación autenticada y el cifrado de token (`code_projects.github_token_encrypted`, `token_cipher`), `github_connections` (ya con PK por cliente tras v58). |

### 5.2 Memorias de Claude Code

| | |
|---|---|
| Qué existe | `POST /v1/admin/memories/import` (`api/admin.rs:1732`): batch JSON, admin-only. **Persiste de inmediato**: sin staging, sin revisión, sin provenance, sin idempotencia por origen. `import_conventions_from_text`. |
| Qué falta | Todo el conector: leer `~/.claude/projects/*/memory/*.md`, `CLAUDE.md`, `AGENTS.md`, `.cursor/rules`, y el formato de frontmatter con `[[wikilinks]]`. |
| Reutilizable | El flujo de harness (`build_harness_manifest_from_path` → `create_harness` → `publish_harness_version`) ya sabe empaquetar configuración de agentes desde una ruta local. El conector de config debería apoyarse ahí en vez de reimplementar el escaneo. |

### 5.3 Postgres / Supabase

| | |
|---|---|
| Qué existe | `src/backup/` — va en **dirección contraria**: espeja SQLite → Postgres vía `BACKUP_DATABASE_URL` para backup/restore. No lee bases externas. Supabase solo aparece en `apps/landing` (schema de waitlist). |
| Qué falta | El conector entero: leer `information_schema`/`pg_catalog`, `COMMENT ON`, constraints, índices, vistas, RLS policies, y `supabase/migrations/*.sql` si están en el repo. |
| Reutilizable | La gestión de connection string y pooling de `backup/client.rs` es el patrón a copiar para una conexión read-only. |

### 5.4 Documentos del repo

| | |
|---|---|
| Qué existe | 161 archivos `.md` entre `docs/` y `openspec/` en este mismo repo. APIs de destino completas: `/v1/memory/*`, `/v1/conventions/*`, `/v1/tasks/*`, `/v1/sdd/*`. |
| Qué falta | El escáner + la clasificación. `bin/import_sdd.rs` cubre **solo** el backfill de `openspec/changes/**` al artifact store — no clasifica prosa ni extrae convenciones ni tareas. |

---

## 6. La colisión sobre "todo vectorizado e indexado"

Requisito del usuario: los artefactos migrados **y los documentos** deben quedar vectorizados e
indexados, y alimentar el grafo de conocimiento.

Hay una decisión previa deliberada en contra de meter docs en el índice de código —
`apps/backend/src/indexer/walker.rs:36-44`:

> *"Documentación (`.md`), datos y config (`.json`, `.yaml`, `.toml`, …) están deliberadamente
> EXCLUIDOS: dominan los resultados de búsqueda de código con prosa que no es código
> (`README.md`, `AGENTS.md`) o ruido generado a máquina, sin aportar señal de código.
> `language_for_ext` sigue reconociendo varias de esas extensiones **para otros llamadores
> (p. ej. `MarkdownChunker`)**, así que la puerta code-only vive en el walker y no en la
> detección de lenguaje."*

Eso no es un obstáculo, es el diseño ya previsto: **un índice de documentación separado**. Y la
pieza cara ya está construida:

- **`MarkdownChunker` existe y está testeado** — `apps/backend/src/indexer/chunker.rs:111`,
  con tests en las líneas 471-533. **No lo llama nadie**, porque el walker no le entrega `.md`.
- `src/embed/mod.rs` expone `embed_one` / `embed_batch` / `serialize` / `cosine` (fastembed,
  nomic-embed-text 768-dim, local — coherente con BYOM).
- `memory_embeddings` existe desde `run_v4` (`migrations.rs:1726`): las memories ya se
  vectorizan por la vía normal. **Migrar por la API de memoria las vectoriza gratis.**

Falta: un walker de documentación (o un flag en el actual) que alimente al `MarkdownChunker`,
y su propia tabla de chunks para no contaminar el corpus de código.

### Grafo de conocimiento

Son **dos grafos distintos**, y conviene no confundirlos:

- **Grafo de memoria** — `GET /v1/memory/graph` (`api/memory.rs`), `MemGraphEdge { from_id,
  to_id, edge_type }` (`models/types.rs:1158`). `edge_type` es texto libre. Resuelve familias de
  proyectos y colorea por proyecto.
- **Grafo de código** — `GET /v1/code/graph`, símbolos y llamadas del `tree_sitter_chunker`.

La migración alimenta el **de memoria**. Pregunta abierta: cómo se derivan hoy las aristas
(no encontré el generador en `queries.rs`) y si el conector debe emitirlas explícitamente
—p. ej. traduciendo los `[[wikilinks]]` de las memorias de Claude Code a aristas reales—
o dejar que se deriven por similitud.

---

## 7. Decisiones tomadas (Cesar, 2026-08-15)

| # | Decisión | Consecuencia |
|---|---|---|
| D1 | **Épica**: 1 change core + 1 por conector | 5 carpetas en `openspec/changes/`, cada una mergeable sola |
| D2 | **Runner: binario Rust local + `claude -p`** | Nuevo `apps/backend/src/bin/`; el backend nunca llama a un LLM; BYOM intacto |
| D3 | **DB: esquema y metadatos por defecto**, con **opción de que el usuario indexe información** | Schema-only es el default seguro; la ingesta de datos es opt-in explícito y necesita su propio gate de PII |
| D4 | **Destinos: memories + conventions + tasks + SDD artifacts + grafo**, y **todo vectorizado e indexado** | Obliga a `run_v60` (rebuild del CHECK de `destination_kind`) y a un índice de documentación |

Heredado de la planificación de julio y **no renegociado**:

- **Nunca importar en silencio.** Todo pasa por staging con revisión y aprobación humana.
  Misma razón que `promote_memory` es manual: una fuga aquí es un incumplimiento contractual,
  no un bug.
- **Idempotente.** Re-correr no duplica, vía `source_identity` derivado de la procedencia.
- **Trazable.** Cada artefacto guarda de qué fuente/commit/doc salió.

---

## 8. Preguntas abiertas (no bloquean el proposal del core)

1. **D3 — el opt-in de datos**: ¿qué gate lo protege? Propuesta a validar en `spec.md`:
   allowlist explícita de tablas + `LIMIT` + redacción de PII obligatoria + attestation
   firmada por el operador, reusando `provenance_kind='client_attested'` que v56 ya tiene.
2. **Aristas del grafo**: ¿el conector las emite explícitamente o se derivan? Bloquea el
   diseño del destino `graph`, no el del pipeline.
3. **Mapeo a `capability` para destinos SDD**: `save_sdd_artifact` exige `capability` para
   `kind='spec'`. ¿Se infiere con el LLM o lo elige el humano en la revisión?
   Recomendación: lo propone el LLM, lo confirma el humano — es un nombre que vive para siempre.
4. **`code_projects.project_id` es 1:1 con `projects`** (aplicado en código, no por constraint,
   según `u2s-client-model/design.md`). Si un cliente tiene 8 repos, ¿son 8 proyectos?
   Afecta cómo se agrupan los runs de `git-history`.
5. **Presupuesto de tokens del runner.** `claude -p` sobre 161 documentos no es gratis. La
   ironía de que un proyecto cuyo objetivo es *reducir* consumo de tokens arranque quemándolos
   merece un límite explícito y medición vía `usage_events` (v59 ya existe).

---

## 9. Evidencia — índice de archivos

| Archivo | Por qué importa |
|---|---|
| `apps/backend/src/db/migrations.rs:422-560` | `run_v56`: esquema de staging completo, sin usar |
| `apps/backend/src/indexer/walker.rs:36-44,168-182` | Exclusión deliberada de docs del corpus de código |
| `apps/backend/src/indexer/chunker.rs:111-243` | `MarkdownChunker` construido y testeado, sin llamador |
| `apps/backend/src/embed/mod.rs` | Embeddings locales (fastembed) |
| `apps/backend/src/bin/import_sdd.rs:1-20` | Precedente de "escanea local, empuja por HTTP" |
| `apps/backend/src/api/admin.rs:1732` | Import de memories actual, sin staging ni provenance |
| `apps/backend/src/api/code.rs:357` | `POST /v1/code/index`, clonado de repos |
| `apps/backend/src/backup/mod.rs` | Postgres, pero en dirección contraria |
| `apps/backend/src/api/router.rs:78-340` | Superficie de destinos: memory, conventions, tasks, sdd |
| `docs/ENGINEERING_PROCESS.md:14` | Principio BYOM |
| `openspec/changes/u2s-client-model/proposal.md:69` | La ingesta declarada fuera de alcance |
