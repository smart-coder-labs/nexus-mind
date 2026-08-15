# Proposal — Knowledge Migration Core (pipeline, staging, revisión e indexación)

> **Change**: `knowledge-migration-core`
> **Status**: proposed
> **Owner**: backend
> **Date**: 2026-08-15
> **Depends on**: `u2s-client-model` (v58, merged), `usage-metrics` (v59, merged)
> **Blocks**: `knowledge-migration-repo-docs`, `-git-history`, `-claude-memories`, `-db-schemas`

---

## 1. Intent

### Problema

Una consultoría que adopta NexusMind arranca con la base vacía. Todo el conocimiento que ya
tiene —decisiones en ADRs, convenciones en `CLAUDE.md`, trabajo pendiente en roadmaps, el
porqué de cada decisión enterrado en mensajes de commit, el modelo de datos real viviendo en el
esquema de Postgres del cliente— es invisible para los agentes. El cerebro empieza amnésico
justo cuando más se necesita: en el onboarding.

Hoy la única vía de entrada es `POST /v1/admin/memories/import` (`api/admin.rs:1732`), y no
sirve para esto por tres razones que no son de implementación sino de contrato:

1. **Persiste de inmediato.** No hay revisión. Un import equivocado ya está dentro.
2. **No tiene procedencia.** El registro no sabe de qué documento, commit o tabla salió, así
   que no se puede auditar, ni corregir en el origen, ni re-correr.
3. **No es idempotente.** Correrlo dos veces duplica.

Para un cerebro de empresa que custodia material de varios clientes bajo NDA, esas tres
carencias no son incomodidades: son la diferencia entre una herramienta usable y un incidente.

### Por qué ahora

1. **La dependencia ya está pagada.** `u2s-client-model` (v58) introdujo `clients`,
   `client_members` y el aislamiento por cliente; `usage-metrics` (v59) introdujo
   `usage_events`. La migración necesitaba ambos y ambos están en `main`.
2. **Media obra ya está hecha y se está pudriendo.** `run_v56` creó el esquema de staging
   completo —runs, candidatos, acciones de revisión append-only, provenance idempotente,
   versionado optimista— y **no lo usa nadie**. Cada migración nueva que pasa por encima
   aumenta la probabilidad de que alguien lo rediseñe desde cero sin verlo.
3. **`MarkdownChunker` lleva construido y testeado desde el trabajo de code-search, sin
   llamador** (`indexer/chunker.rs:111`). La indexación de documentación cuesta cablearla, no
   escribirla.
4. **El coste crece con el tiempo.** Cada día sin migración es más conocimiento que el equipo
   sigue re-explicando a mano a cada agente.

### Success looks like

- Un operador corre un comando local, revisa una lista de candidatos en el admin, aprueba lo
  que sirve, y ese conocimiento queda dentro de NexusMind **atribuido a su cliente**,
  vectorizado y buscable.
- **Nada entra sin que un humano lo apruebe.** Ni una memoria, ni una convención, ni una
  tarea, ni una herramienta ejecutable.
- Correr la misma migración dos veces produce **cero duplicados** y un reporte que dice
  exactamente qué se saltó y por qué.
- Cada artefacto migrado responde a "¿de dónde salió esto?" con un documento, un commit o una
  tabla concreta.
- El backend **no llama a ningún LLM**. Se puede desplegar sin credenciales de modelo y el core
  sigue funcionando (BYOM, `docs/ENGINEERING_PROCESS.md:14`).
- Un candidato rechazado **no vuelve a aparecer** en la siguiente corrida sin una acción
  explícita de re-staging.

---

## 2. Scope

### In scope

1. **`run_v60`** — extiende el esquema muerto de v56 en vez de reemplazarlo:
   - `migration_runs.client_id` — FK nullable a `clients` (NULL = proyecto interno), con el
     mismo trigger de coherencia org que ya tiene `project_id`.
   - `migration_runs.source_kind` — `CHECK IN ('repo-docs','git-history','claude-memories','db-schema')`.
   - **Recreación guardada de las 5 tablas de v56.** `destination_kind` **se mueve del run al
     candidato** y se amplía a `('memory','convention','task','sdd_artifact','harness',
     'harness_config_review')`. v56 lo puso en `migration_runs`, asumiendo un destino por run;
     un escaneo de `docs/` produce memories, conventions, tasks y SDD en la misma pasada, así
     que la asunción no sobrevive. Las tablas están vacías (esquema muerto), y la migración
     **verifica esa precondición y aborta si encuentra filas** antes de recrear nada.
     Ver `design.md` §3.
   - `migration_candidates.destination_hint` — JSON con lo que el destino necesita y el
     candidato genérico no tiene (`capability` para specs, `priority`/`assignee` para tasks,
     el manifiesto tipado completo para harnesses).
   - `migration_candidates.indexed_at` — cuándo quedó vectorizado el destino, o NULL.
   - `doc_chunks` + `doc_chunk_embeddings` — el corpus de documentación, **separado del de
     código a propósito** (ver §3).
2. **API de migración** (`/v1/migrations/*`) — crear run, subir candidatos en lote, listar para
   revisión, aprobar/rechazar con `expected_version`, commitear los aprobados, cancelar,
   y reporte del run.
3. **Commit atómico por candidato a los seis destinos** — memory, convention, task, sdd_artifact,
   **harness** y **harness_config_review**, reusando las capas de persistencia existentes
   (`/v1/memory`, `/v1/conventions`, `/v1/tasks`, `/v1/sdd/artifacts`, `/v1/harnesses` +
   `/v1/harnesses/:id/versions`, `/v1/harness-config-reviews`) en lugar de escribir SQL nuevo
   por destino. Los dos últimos los usa hoy solo `knowledge-migration-claude-memories`, pero
   el destino es del pipeline: cualquier conector futuro que encuentre herramientas las emite
   igual. La atomicidad es **por candidato, con el lote reanudable**: un candidato que falla no
   revierte los ya commiteados ni bloquea los siguientes (`design.md` §4.1).
4. **Indexación** — lo commiteado se vectoriza **después del commit y en modo best-effort**,
   no dentro de la transacción: embeber es CPU-bound y bloquearía a los escritores. Memories
   por la vía normal (`memory_embeddings`); documentos por un walker de documentación nuevo que
   alimenta al `MarkdownChunker` existente. Un artefacto puede quedar persistido sin vector
   (`indexed_at IS NULL`) y la reconciliación lo recoge después (`design.md` §4.3).
5. **Runner local** — `apps/backend/src/bin/migrate_knowledge.rs`: escanea, invoca `claude -p`
   para la inferencia, hace POST de candidatos. Trae el trait de conector que los cuatro
   changes de fuente implementan; **no trae ningún conector real** salvo un `noop` para tests.
6. **UI de revisión** — `apps/admin/src/pages/Migrations.tsx`: lista de candidatos con su
   contenido, procedencia, destino propuesto, y aprobar/rechazar en lote.
7. **Contabilidad de tokens del runner** — cada invocación a `claude -p` reporta a
   `POST /v1/usage` (v59). Un proyecto cuyo objetivo declarado es reducir consumo de tokens
   tiene que medir el suyo.

### Out of scope

- **Los cuatro conectores reales.** Cada uno es su propio change; este solo define el contrato
  que implementan.
- **Aristas explícitas del grafo de conocimiento.** El destino `graph` queda para un change
  posterior: primero hay que resolver cómo se derivan hoy las aristas de memoria
  (`exploration.md` §6, pregunta abierta 2). Las memories migradas entran al grafo por la vía
  que ya usan las demás.
- **Reindexado del corpus de código.** El índice de código no se toca.
- **Ingesta de filas de negocio.** El opt-in de datos (D3) se especifica en
  `knowledge-migration-db-schemas`, con su propio gate de PII.
- **Migración desde Notion / Confluence / Google Docs.** Estaban en el plan de julio; sin
  fuente local, exigen OAuth y un modelo de credenciales que no toca este change.
- **Auto-aprobación por umbral de confianza.** Deliberadamente fuera: rompe el invariante.

---

## 3. Approach

### Forma

```
[máquina local]                                    [backend Fly.io]
  migrate-knowledge --source repo-docs \
      --client acme --project acme-billing
        │
        ├─ scan     → SourceItem { source_identity, raw, meta }
        │              (source_identity determinista: p.ej. sha256 del path+commit)
        │
        ├─ infer    → claude -p --output-format json
        │              devuelve Candidate tipado: destino, contenido, hint, confianza
        │              (el LLM PROPONE; nunca decide qué entra)
        │
        └─ push     → POST /v1/migrations/{run}/candidates  ──►  migration_candidates
                                                                    status='staged'
                                                                         │
                                        admin UI: revisión humana ◄──────┤
                                                                         │
                       POST /v1/migrations/{run}/commit  ──► memory / convention / task / sdd
                                                            + embeddings + migration_provenance
```

**La frontera es el punto entero del diseño**: el LLM y el filesystem del cliente viven a la
izquierda; la verdad persistida y el gate humano viven a la derecha. El backend nunca necesita
el repo, ni la base del cliente, ni una API key de modelo.

### Idempotencia

`source_identity` es determinista por procedencia y lo calcula el conector, no el LLM:

| Fuente | `source_identity` |
|---|---|
| repo-docs | `repo-docs:{repo}:{path}:{blob_sha}` |
| git-history | `git:{repo}:{commit_sha}` |
| claude-memories | `claude:{host_scope}:{relpath}:{content_sha}` |
| db-schema | `pg:{database}:{schema}.{object}:{ddl_sha}` |

`migration_provenance UNIQUE(org_id, destination_kind, source_identity)` (ya en v56) hace el
resto: un segundo commit del mismo origen al mismo tipo de destino **falla en la base de
datos**, no en una comprobación aplicativa que alguien puede olvidar. Como el hash del
contenido entra en la identidad, un documento editado es un candidato **nuevo** y se vuelve a
revisar — que es lo correcto: cambió, merece ojos.

### Por qué el corpus de documentación va separado

`walker.rs:36-44` documenta que los `.md` se sacaron del índice de código porque READMEs y
`AGENTS.md` rankeaban por encima de handlers reales. Meterlos de vuelta reintroduciría
exactamente ese bug. Un corpus aparte (`doc_chunks`) da búsqueda de documentación sin degradar
la de código, y es la salida que el propio comentario anticipa al mencionar `MarkdownChunker`.

### Rationale

- **Extender v56, no rediseñarlo.** Modela ya el gate humano, la idempotencia y la concurrencia
  optimista. El coste de aprovecharlo es un rebuild de dos tablas; el de ignorarlo es rehacer
  el mismo razonamiento peor.
- **El LLM propone, el humano dispone.** La confianza que devuelva `claude -p` es un criterio
  de **ordenación** en la UI de revisión, nunca un permiso de escritura.
- **Commit a través de las APIs de destino, no por SQL directo.** Las reglas de negocio de una
  memory (scoping, embeddings, audit) viven en su capa. Un camino de escritura paralelo
  significa que la migración se salta el aislamiento por cliente, y eso es precisamente lo que
  no puede pasar.
- **Un candidato `harness` no sustituye el gate de instalación.** La aprobación de migración
  decide si algo pasa a ser herramienta del equipo; la aprobación de instalación
  (`requires_approval`, ya en el modelo de harness) decide si corre en la máquina de quien la
  recibe. Son dos preguntas distintas con dos responsables distintos, y colapsarlas dejaría
  hooks ejecutables entrando sin que su destinatario opine.
- **Runner en Rust, dentro de este repo.** Comparte tipos con el backend, se compila en el
  mismo CI, y sigue el precedente de `import_sdd.rs`. En TypeScript viviría en el repo del MCP
  y habría que coordinar dos releases.
- **`client_id` desde el día uno.** Una migración que escribe conocimiento de cliente sin
  cliente asociado deshace el aislamiento que v58 acaba de construir.

---

## 4. Risks & open questions

| Riesgo | Mitigación |
|---|---|
| **El LLM alucina una convención que nadie acordó** y un revisor cansado la aprueba en lote. | El candidato muestra siempre la cita literal de la fuente junto a la propuesta. Aprobar en lote exige que todos los candidatos del lote tengan procedencia verificada; los `client_attested` se aprueban de uno en uno. |
| **Recreación de las 5 tablas de v56 en v60.** SQLite no altera CHECKs ni quita columnas con CHECK cruzado. | Las tablas están **vacías en toda instalación existente** (esquema muerto). La migración lo **verifica y aborta ruidosamente** si encuentra filas, en vez de destruirlas. Recrear es trivial hoy y caro en seis meses. |
| **Coste en tokens del runner.** 161 documentos solo en este repo. | `--dry-run` que estima antes de gastar; batching por documento; reporte obligatorio a `usage_events`; tope configurable que aborta el run. |
| **Fuga entre clientes.** Un run apuntado al cliente equivocado escribe material de A en B. | `client_id` obligatorio en el run (o NULL explícito para interno), inmutable tras la creación por trigger, con trigger de coherencia org. Los tests de aislamiento son criterio de aceptación, no opcionales. |
| **`claude -p` devuelve JSON inválido o cambia de formato entre versiones.** | Salida validada contra un esquema tipado; un candidato que no parsea se registra como `failed` con su salida cruda y no aborta el run. Versión del CLI registrada en el run. |
| **La revisión se convierte en el cuello de botella** y nadie migra nada. | Agrupación por fuente y por tipo de destino, aprobar en lote con las condiciones de arriba, y orden por confianza. Si aun así no se usa, el problema es de producto y hay que saberlo pronto. |

### Preguntas abiertas (no bloquean spec/design)

1. **`capability` para destinos `sdd_artifact`.** `save_sdd_artifact` la exige para `kind=spec`.
   Recomendación: la propone el LLM y **la confirma el humano** — es un nombre que sobrevive al
   change que lo creó.
2. **Un cliente con 8 repos, ¿son 8 proyectos?** `code_projects.project_id` es 1:1 con
   `projects` por convención aplicativa (`u2s-client-model/design.md`). Afecta cómo se agrupan
   los runs de `git-history`, no este pipeline.
3. **Retención de candidatos rechazados.** Se quedan para no re-proponerlos, pero pueden
   contener material que el cliente pidió no guardar. Propuesta: purga por run con audit,
   conservando solo `source_identity` para seguir suprimiéndolos.

---

## 5. Plan de entrega

| PR | Contenido | Verificable por |
|---|---|---|
| 1 | `run_v60` + tipos + tests de migración | `cargo test run_v60_*` |
| 2 | Queries + API `/v1/migrations/*` (staging y revisión, sin commit) | tests de integración |
| 3 | Commit atómico por candidato a los 6 destinos + provenance | tests de idempotencia y aislamiento |
| 4 | Walker de documentación + `doc_chunks` + embeddings (cablea `MarkdownChunker`) | tests de indexación |
| 5 | Runner `migrate_knowledge` + trait de conector + conector `noop` | tests del runner |
| 6 | UI de revisión en admin | vitest + tsc |

`strict_tdd: true` para backend y admin (`openspec/config.yaml`): test primero en los seis.
