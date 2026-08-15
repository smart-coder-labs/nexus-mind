# Proposal — Conector: documentos del repo

> **Change**: `knowledge-migration-repo-docs`
> **Status**: proposed
> **Owner**: backend + tooling
> **Date**: 2026-08-15
> **Depends on**: `knowledge-migration-core`

---

## 1. Intent

### Problema

El conocimiento mejor escrito de un equipo suele estar en Markdown y ser invisible para los
agentes. Este repo es su propio caso de prueba: **161 archivos `.md`** entre `docs/` y
`openspec/`, incluyendo ADRs, `ENGINEERING_PROCESS.md` (que fija principios de ingeniería que
un agente debería respetar y hoy no conoce), `ARCHITECTURE.md`, `PRD.md`, roadmaps con trabajo
pendiente sin fichar, y specs SDD completas.

Un agente que trabaja sobre este repo no sabe que existe el principio BYOM hasta que alguien se
lo pega en el prompt. Eso es exactamente el fallo que NexusMind existe para eliminar.

### Por qué es el primer conector

Es el más barato de los cuatro y el que valida el pipeline entero: los cuatro tipos de destino
aparecen en documentos de repo, no hay credenciales que gestionar, no hay red que atravesar, y
**el corpus de validación está en este mismo checkout**. Si el core no funciona aquí, no
funciona en ningún sitio.

### Success looks like

- Correr el conector sobre este repo produce candidatos plausibles para los cuatro destinos, y
  un humano reconoce el contenido sin abrir el archivo original.
- Los ADRs llegan como `decision`; `ENGINEERING_PROCESS.md` produce **conventions**, no
  memories; los roadmaps con casillas sin marcar producen **tasks**; `openspec/changes/**`
  llega al artifact store como SDD.
- Re-correrlo sin cambios produce **cero candidatos nuevos**.
- Editar un documento y re-correr produce **un** candidato: el del documento editado.

---

## 2. Scope

### In scope

1. **Escáner de documentación** — recorre rutas dadas respetando `.gitignore`, admite `.md`
   (y `.mdx`), con includes/excludes configurables. Reusa la maquinaria de `ignore` que
   `walker.rs` ya monta; **no** reusa `CODE_EXTENSIONS`, que es el allowlist opuesto.
2. **Troceado por documento** — vía el `MarkdownChunker` existente (`chunker.rs:111`), que ya
   parte por encabezados y conserva la jerarquía de secciones. Un documento largo produce
   varios candidatos, uno por sección relevante, no un candidato gigante.
3. **Clasificación con `claude -p`** — cada sección → destino propuesto + tipo + título +
   contenido normalizado + `destination_hint` + confianza + **cita literal de la fuente**.
4. **Reglas de mapeo por defecto** (el LLM las puede desviar, el humano decide):

   | Patrón de origen | Destino propuesto |
   |---|---|
   | `docs/adr/ADR-*.md` | memory `type=decision` |
   | Principios, reglas, "siempre/nunca", guías de estilo | **convention** |
   | Casillas `- [ ]` sin marcar en roadmaps y task breakdowns | **task** |
   | `openspec/changes/**` | **sdd_artifact** (kind por nombre de archivo) |
   | `openspec/specs/**` | fuera de alcance — es la spec viva, no se migra |
   | Prosa de arquitectura y contexto | memory `type=architecture` |

5. **Indexación** — cada documento escaneado va también a `doc_chunks` con embeddings,
   **independientemente de si sus candidatos se aprueban**. Buscar un documento y aceptar una
   afirmación como conocimiento del equipo son dos actos distintos.

### Out of scope

- Notion, Confluence, Google Docs — exigen OAuth y modelo de credenciales.
- `openspec/specs/**` (la especificación viva): la mantiene `sdd-archive`, no un importador.
- Traducción de idioma. Los docs de este repo mezclan español e inglés; se migran como están.

---

## 3. Approach

```
migrate-knowledge --source repo-docs --path . --client acme --project acme-billing \
                  [--include 'docs/**' --exclude 'docs/marketing/**'] [--dry-run]
```

`source_identity` = `repo-docs:{repo}:{path}#{section_anchor}:{blob_sha}`

El `blob_sha` va dentro a propósito: editar el documento cambia la identidad, el candidato se
vuelve a proponer, y el humano ve que cambió. El ancla de sección permite que un documento de
40 secciones no sea un todo-o-nada.

### Rationale

- **Sección, no archivo.** `ENGINEERING_PROCESS.md` tiene principios que son convenciones y
  tablas de stack que son contexto. A nivel de archivo, o pierdes las convenciones o ensucias
  el corpus.
- **Indexar siempre, migrar solo lo aprobado.** El índice de documentación es recuperación; el
  conocimiento migrado es afirmación con autoridad. Confundirlos es como tratar un borrador
  como una decisión.
- **`--dry-run` obligatorio en la primera pasada de un repo nuevo.** Estima documentos,
  secciones y tokens antes de gastar nada.

---

## 4. Risks & open questions

| Riesgo | Mitigación |
|---|---|
| **Documentación obsoleta migrada como verdad vigente.** `ENGINEERING_PROCESS.md` dice "Versión 2.0, Mayo 2026" y menciona `sqlite-vss`, cuando el código usa `fastembed`. | El candidato lleva la fecha del documento y su último commit; la UI de revisión marca lo que no se toca hace >6 meses. El humano decide; la máquina no adivina qué envejeció. |
| **Ruido de marketing y research.** `docs/marketing/`, `docs/research/` no son conocimiento de ingeniería. | Excludes por defecto para esas rutas, sobreescribibles. |
| **Documentos que se contradicen entre sí.** | Fuera de alcance detectarlo automáticamente. Se registra como pregunta abierta para un "detector de conflictos" posterior, apoyado en `find_duplicate_memories` que ya existe. |
| **Casillas `- [ ]` que ya se hicieron pero nadie marcó** → tareas fantasma. | Las tasks propuestas entran con `status=backlog` y la cita de origen; el revisor las cierra en el acto si ya están hechas. |

**Pregunta abierta:** los `openspec/changes/**` de este repo ya los backfilleó
`bin/import_sdd.rs`. Hay que decidir si este conector los ignora (evitando dos caminos hacia el
mismo destino) o si lo sustituye. Recomendación: **ignorarlos en v1** y dejar `import_sdd`
como está — sustituirlo es un refactor que no aporta conocimiento nuevo.
