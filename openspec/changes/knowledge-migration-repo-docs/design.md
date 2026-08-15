# Design — Repo Docs Connector

> **Change**: `knowledge-migration-repo-docs`
> **Status**: proposed
> **Owner**: backend + tooling
> **Date**: 2026-08-15
> **Depends on**: `knowledge-migration-core` (merged in this branch)

Asume leídos `proposal.md` y el delta spec. Este documento describe cómo se implementa el
conector sobre el pipeline que el core ya dejó montado.

---

## 1. Qué es realmente nuevo

El core dejó construido casi todo lo caro. Conviene ser explícito sobre qué se reusa, porque el
tamaño aparente de este change no corresponde a su tamaño real:

| Pieza | Estado |
|---|---|
| Recorrido del árbol respetando `.gitignore` | **existe** — `indexer::doc_walker::walk_docs` |
| Troceado por encabezados | **existe** — `MarkdownChunker` (`chunker.rs:111`) |
| Persistencia e indexación del corpus documental | **existe** — `db::doc_queries`, `indexer::index_documents` |
| Staging, revisión, commit, provenance | **existe** — `db::migration_queries`, `api::migrations` |
| Contrato del conector, presupuesto, dry-run, push HTTP | **existe** — `bin/migrate_knowledge.rs` |
| **Escaneo por sección con identidad estable** | nuevo |
| **Reglas de mapeo origen → destino** | nuevo |
| **Prompt de clasificación y su fallback determinista** | nuevo |

Lo nuevo son unas trescientas líneas de conector más sus tests. Todo lo demás es cableado.

---

## 2. Dónde vive

`bin/migrate_knowledge.rs` es un binario, y un conector que necesita `walk_docs` y el
`MarkdownChunker` necesita la librería. Ambos están disponibles: el binario ya depende del
crate (`nexusmind::…`), igual que `import_sdd.rs`.

El conector vive en `src/migration/repo_docs.rs` — módulo de librería, no del binario — por dos
razones concretas:

1. **Testabilidad sin proceso.** Un conector dentro del binario solo se prueba con
   `cargo test --bin`, que no ve el resto de la suite. En la librería comparte los helpers de
   test que ya existen.
2. **Los otros tres conectores van a querer lo mismo.** `src/migration/` es donde vivirán, y
   crear la carpeta con el primero evita una mudanza cuando llegue el segundo.

`bin/migrate_knowledge.rs` pasa a importarlo y a registrarlo en `connector_for`.

Consecuencia: el trait `Connector` y sus tipos (`SourceItem`, `CandidatePayload`, `ScanOptions`)
**se mueven del binario a `src/migration/mod.rs`**. El binario los re-exporta para no romper sus
propios tests. Es un movimiento, no una reescritura.

---

## 3. La unidad de escaneo es la sección

### 3.1 Por qué no el archivo

`docs/ENGINEERING_PROCESS.md` contiene, en el mismo archivo, principios que son convenciones de
equipo y tablas de stack que son contexto de arquitectura. A nivel de archivo hay que elegir: o
se pierden las convenciones o se ensucia el corpus con tablas. A nivel de sección, cada mitad va
a su sitio.

El coste es que un documento produce N candidatos en vez de uno, y la cola de revisión crece. Se
compensa ordenando por confianza y agrupando por documento en la UI, que ya existe.

### 3.2 Reutilizar el chunker, no reimplementarlo

`MarkdownChunker::chunk` ya parte por encabezados, respeta los fences de código —de modo que un
`# comment` dentro de un bloque no abre una sección falsa— y devuelve `RawChunk { symbol,
start_line, end_line, content }`, donde `symbol` es el título de la sección.

El conector lo llama y traduce `RawChunk` → `SourceItem`. No hay parseo de Markdown propio.

### 3.3 Identidad

```
repo-docs:{repo}:{path}#{anchor}:{sha256(section_content)[..16]}
```

- `repo` es el nombre del directorio raíz, no una ruta absoluta: una identidad no puede llevar
  el home del operador dentro.
- `anchor` sale del mismo `slugify(heading, start_line)` que usa `doc_queries`, así que el ancla
  del candidato y la del chunk indexado coinciden — un revisor puede saltar del uno al otro.
- El hash es del contenido de **la sección**, no del archivo. Editar la sección 3 no cambia la
  identidad de la 1 ni la de la 5, así que un rescan tras un cambio pequeño propone una cosa y
  no cuarenta.

---

## 4. Las reglas de mapeo

Deterministas, evaluadas en orden, primera que casa gana:

```rust
fn propose_destination(path: &str, section: &Section) -> Proposal {
    if path.contains("/openspec/changes/")      { return sdd_artifact_from(path); }
    if is_under_adr(path)                       { return memory("decision"); }
    if let Some(items) = unchecked_tasks(section) { return task_from(items); }
    if reads_like_a_rule(section)               { return convention(); }
    memory("architecture")
}
```

Dos detalles que parecen menores y no lo son:

**`unchecked_tasks` solo mira casillas sin marcar.** Una casilla marcada es trabajo hecho, y
proponerla como tarea crea trabajo fantasma que alguien tiene que cerrar a mano. El spec lo
exige explícitamente.

**`reads_like_a_rule` es un heurístico, y se declara como tal.** Busca imperativos y absolutos
—"siempre", "nunca", "debe", "no se debe", "must", "never"— en el encabezado o en las primeras
líneas. Acierta en `ENGINEERING_PROCESS.md` y falla en prosa que describe una regla sin
enunciarla. **Por eso el LLM puede desviarlo y el humano decide**: el heurístico ordena la cola,
no la cierra.

---

## 5. El prompt

Un prompt por sección, con la sección entera y su ruta, pidiendo JSON del `CandidatePayload`.
Tres instrucciones cargan todo el peso:

1. **"Propón, no decidas."** El destino que devuelve es una sugerencia que un humano revisará.
2. **"El extracto debe ser literal."** Si parafrasea, el revisor pierde la única forma de juzgar
   sin abrir el archivo.
3. **"Si la sección no contiene conocimiento reutilizable, dilo."** Un conector que produce un
   candidato por sección sin filtrar convierte 161 documentos en mil candidatos y la revisión en
   un trabajo que nadie hace. El LLM puede devolver `skip` con su razón.

La identidad **la estampa el conector después**, sobrescribiendo lo que el modelo devuelva —
como ya hace `ClaudeCli::classify`. Un modelo que parafrasee la identidad no puede romper la
idempotencia.

---

## 6. El fallback

`fallback()` devuelve siempre `Some`: ninguna sección se pierde por no tener clasificador.

- destino: el que dan las reglas de §4;
- título: el encabezado de la sección, o la primera línea si no lo hay;
- contenido: la sección entera;
- extracto: las primeras líneas de la sección, verbatim;
- confianza: `None` — sin modelo no hay puntuación, y fingir una sería peor que no darla.

Esto es lo que hace utilizable `--no-llm`, que es el modo que necesita un cliente cuyo NDA
prohíba enviar material a un tercero.

---

## 7. Exclusiones

Por defecto, además de las que ya aplica `doc_walker` (`CHANGELOG`, licencias, `node_modules`):

| Ruta | Por qué |
|---|---|
| `docs/marketing/**`, `docs/research/**` | no es conocimiento de ingeniería |
| `openspec/specs/**` | es la especificación viva; la mantiene `sdd-archive`, no un importador |
| `openspec/changes/archive/**` | changes cerrados; migrarlos duplicaría lo que ya está en el artifact store |

**Se reportan como excluidos, no se omiten en silencio.** Un run que dice "escaneé 40 documentos"
cuando había 161 es un run que miente; el reporte distingue escaneados de excluidos y por qué.

`openspec/changes/**` en curso **sí** se escanea, pero ver §9: hay una decisión abierta ahí.

---

## 8. Tests — orden TDD

1. `scan_splits_a_document_into_sections` — y un documento sin encabezados da una unidad.
2. `identity_is_stable_across_rescans` / `editing_one_section_changes_only_its_identity`.
3. `identity_excludes_absolute_paths`.
4. `adr_path_proposes_a_decision_memory`.
5. `unchecked_checklist_item_proposes_a_task` / `checked_items_propose_no_task`.
6. `rule_shaped_section_proposes_a_convention`.
7. `openspec_change_proposes_an_sdd_artifact`.
8. `plain_prose_falls_back_to_an_architecture_memory`.
9. `every_candidate_carries_a_verbatim_excerpt`.
10. `fallback_produces_a_candidate_for_every_unit`.
11. `default_excludes_skip_marketing_research_and_living_specs`.
12. `excluded_documents_are_reported_not_omitted`.
13. `dry_run_reports_counts_and_classifies_nothing`.
14. **`scanning_this_repository_produces_plausible_candidates`** — el conector corrido sobre
    `docs/` de este mismo repo. No afirma un número exacto (el corpus cambia); afirma que
    `ENGINEERING_PROCESS.md` produce al menos una convención y que ningún candidato lleva una
    ruta absoluta. Es el test que convierte "debería funcionar" en "funciona sobre 161 archivos
    reales".

---

## 9. Decisión abierta

**`openspec/changes/**` ya lo backfilleó `bin/import_sdd.rs` al artifact store.** El proposal
recomendaba ignorarlos en v1 para no tener dos caminos hacia el mismo destino.

Implementado así: se escanean para el **índice documental** (son documentación y buscarlos es
útil) pero **no producen candidatos `sdd_artifact`**. Un flag `--include-sdd` los habilita para
quien migre un repo ajeno, donde `import_sdd` nunca corrió.

Es reversible y no bloquea nada; si al usarlo resulta que el flag sobra, se quita.
