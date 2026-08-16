# Design — Knowledge Migration Core

> **Change**: `knowledge-migration-core`
> **Status**: proposed
> **Owner**: backend
> **Date**: 2026-08-15

Este documento es el plano de implementación. Asume leídos `proposal.md` (el porqué) y los tres
delta specs (el qué). Describe el cómo: el rediseño de v60, la frontera transaccional del
commit, las firmas Rust, el contrato del runner y el orden TDD.

Dos decisiones de v56 no sobreviven al contacto con los requisitos, y ambas se corrigen aquí
(§11 lista las diferencias respecto al proposal).

---

## 1. Architecture overview

```
   MÁQUINA LOCAL                                  BACKEND (Fly.io)
   ─────────────                                  ────────────────
   migrate-knowledge
      │
      │ Connector::scan()          ← filesystem, git, psql
      ▼
   Vec<SourceItem>                 source_identity determinista
      │
      │ Classifier::classify()     ← `claude -p --output-format json`
      │   (fallback determinista si no hay CLI o falla el parseo)
      ▼
   Vec<Candidate>                  destination_kind + hint + excerpt + confidence
      │
      │ POST /v1/migrations/{run}/candidates
      ▼
   ══════════════════════════ FRONTERA ═══════════════════════════
                                     │
                          migration_candidates (staged)
                                     │
                          revisión humana (admin UI)
                                     │  approve/reject + expected_version
                                     ▼
                          POST /v1/migrations/{run}/commit
                                     │
                       ┌─────────────┼─────────────┐
                       ▼             ▼             ▼
                    memory      convention      harness ...
                       └─────────────┼─────────────┘
                                     ▼
                          migration_provenance + outcomes
                                     │
                          vectorización (post-commit, best-effort)
```

La propiedad crítica: **el LLM y el material del cliente viven a la izquierda de la frontera;
la verdad persistida y el gate humano viven a la derecha.** El backend no necesita el repo, ni
la base del cliente, ni credenciales de modelo, y eso es verificable con un test que despliega
el backend sin ninguna variable de modelo y corre el pipeline entero.

---

## 2. File-by-file change list

| Archivo | Cambio |
|---|---|
| `db/migrations.rs` | `run_v60` — recreación guardada de las 5 tablas de v56 + 3 tablas de documentación |
| `db/migration_queries.rs` | **nuevo** — runs, candidatos, acciones de revisión, provenance, outcomes, reporte |
| `db/doc_queries.rs` | **nuevo** — documentos, chunks, embeddings, búsqueda, reconciliación |
| `db/queries.rs` | extraer `store_memory_with_audit` (§4.2); `set_candidate_indexed` |
| `models/types.rs` | `MigrationRun`, `MigrationCandidate`, `DestinationKind`, `SourceKind`, `DestinationHint`, `ReviewAction`, `RunReport`, DTOs de request |
| `api/migrations.rs` | **nuevo** — handlers de run, staging, revisión, commit, reporte |
| `api/docs.rs` | **nuevo** — búsqueda documental y estado de indexación |
| `api/router.rs` | montar `/v1/migrations/*` y `/v1/docs/*` |
| `api/middleware.rs` / RBAC | registrar `migration:read`, `migration:write`, `migration:review` |
| `indexer/doc_walker.rs` | **nuevo** — walker que sí admite `.md` y alimenta al `MarkdownChunker` |
| `indexer/mod.rs` | `index_documents()` junto a la indexación de código existente |
| `bin/migrate_knowledge.rs` | **nuevo** — runner, trait `Connector`, adaptador de `claude -p`, conector `noop` |
| `apps/admin/src/pages/Migrations.tsx` | **nuevo** — cola de revisión |
| `apps/admin/src/{types.ts,api/client.ts,App.tsx,Layout.tsx}` | tipos y rutas del admin |

---

## 3. `run_v60` — recreación guardada, no rebuild incremental

### 3.1 Por qué recrear en vez de alterar

Las cinco tablas de v56 están **vacías en toda instalación existente**: son esquema muerto, sin
un solo llamador (`grep` sobre `apps/backend/src` y `apps/admin/src` no encuentra referencias
fuera de `migrations.rs`). Y los cambios necesarios no son aditivos:

| Tabla | Qué hay que cambiar | ¿ALTER sirve? |
|---|---|---|
| `migration_runs` | **quitar** `destination_kind` y su CHECK cruzado; añadir `client_id`, `source_kind` | No — SQLite no quita columnas con CHECK cruzado |
| `migration_candidates` | añadir `destination_kind` NOT NULL con CHECK, `destination_hint`, `source_excerpt`, `confidence`, `indexed_at` | Parcialmente |
| `migration_provenance` | ampliar el CHECK de `destination_kind` | No — SQLite no altera CHECKs |
| `migration_review_actions`, `migration_outcomes` | nada propio, pero cuelgan de las anteriores por FK | — |

Tres rebuilds parciales entrelazados por FK son más frágiles y más difíciles de leer que una
recreación limpia. La recreación es segura **solo** porque las tablas están vacías, así que
esa precondición se verifica en la propia migración:

```rust
pub fn run_v60(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 60 { return Ok(()); }

    // Precondición: v56 es esquema muerto. Si alguien tiene datos, la suposición
    // que justifica recrear es falsa y hay que parar en vez de destruirlos.
    for t in ["migration_runs", "migration_candidates", "migration_review_actions",
              "migration_provenance", "migration_outcomes"] {
        let exists: bool = conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1", [t], |_| Ok(true)
        ).optional()?.unwrap_or(false);
        if !exists { continue; }
        let n: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))?;
        if n != 0 {
            anyhow::bail!(
                "run_v60: {t} contiene {n} filas; v60 asume que el staging de v56 nunca se usó. \
                 Migra esos datos a mano antes de continuar."
            );
        }
    }
    // DROP + CREATE de las 5 tablas, en orden inverso de FK, dentro de la misma transacción.
    ...
    conn.execute_batch("PRAGMA user_version = 60;")?;
    Ok(())
}
```

Fallar ruidosamente ante datos inesperados es el punto entero del guard. Un `DROP TABLE`
silencioso sobre datos reales sería el peor fallo posible de esta migración.

### 3.2 Forma resultante

```sql
CREATE TABLE migration_runs (
  id             TEXT PRIMARY KEY,
  org_id         TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  client_id      TEXT REFERENCES clients(id)  ON DELETE RESTRICT,  -- NULL = interno
  project_id     TEXT REFERENCES projects(id) ON DELETE RESTRICT,
  source_kind    TEXT NOT NULL CHECK(source_kind IN
                   ('repo-docs','git-history','claude-memories','db-schema','noop')),
  status         TEXT NOT NULL DEFAULT 'staging' CHECK(status IN
                   ('staging','in_review','committing','completed','cancelled')),
  source_ref     TEXT,        -- repo, ruta o base, ya redactado
  runner_version TEXT,        -- versión del CLI de inferencia usado
  attestation    TEXT NOT NULL DEFAULT '{}',
  created_by     TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
  created_at     TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**`destination_kind` desaparece del run.** Era la asunción "un run = un tipo de destino", y no
sobrevive al primer caso de uso real: un escaneo de `docs/` produce memories, conventions,
tasks y artefactos SDD **en la misma pasada**. Mantenerla obligaría a escanear cuatro veces el
mismo árbol, cuadruplicando el coste en tokens para satisfacer una restricción de esquema.

Con ella se va el CHECK cruzado `(destination_kind='convention' AND project_id IS NULL)`, que
además contradecía el modelo real: `conventions.project_id` existe y es nullable
(`migrations.rs:1372`). v56 prohibía convenciones de proyecto que la tabla destino sí soporta.

Se conservan los dos triggers de coherencia org↔project de v56, y se añade el gemelo para
`client_id`:

```sql
CREATE TRIGGER migration_runs_client_scope_insert
BEFORE INSERT ON migration_runs
WHEN NEW.client_id IS NOT NULL AND NOT EXISTS (
  SELECT 1 FROM clients WHERE id = NEW.client_id AND org_id = NEW.org_id)
BEGIN SELECT RAISE(ABORT, 'migration client must belong to run organization'); END;
```

Y la inmutabilidad exigida por el spec, que en SQLite es un trigger, no una convención:

```sql
CREATE TRIGGER migration_runs_scope_immutable
BEFORE UPDATE OF org_id, client_id, project_id, source_kind ON migration_runs
WHEN OLD.client_id IS NOT NEW.client_id OR OLD.project_id IS NOT NEW.project_id
  OR OLD.org_id <> NEW.org_id OR OLD.source_kind <> NEW.source_kind
BEGIN SELECT RAISE(ABORT, 'migration run scope is immutable'); END;
```

```sql
CREATE TABLE migration_candidates (
  id                  TEXT PRIMARY KEY,
  run_id              TEXT NOT NULL REFERENCES migration_runs(id) ON DELETE CASCADE,
  source_identity     TEXT NOT NULL,
  destination_kind    TEXT NOT NULL CHECK(destination_kind IN
                        ('memory','convention','task','sdd_artifact',
                         'harness','harness_config_review')),
  destination_hint    TEXT NOT NULL DEFAULT '{}',
  content             TEXT NOT NULL,
  source_excerpt      TEXT,                 -- cita literal mostrada en la revisión
  confidence          REAL,                 -- solo ordena la cola; nunca autoriza
  normalized_metadata TEXT NOT NULL DEFAULT '{}',
  attestation         TEXT NOT NULL DEFAULT '{}',
  provenance_kind     TEXT NOT NULL DEFAULT 'client_attested'
                        CHECK(provenance_kind IN ('client_attested','verified_manifest')),
  status              TEXT NOT NULL DEFAULT 'staged' CHECK(status IN
                        ('staged','approved','rejected','committing','committed',
                         'skipped','failed','cancelled')),
  version             INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  indexed_at          TEXT,                 -- NULL = persistido pero sin vector
  created_at          TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(run_id, source_identity)
);
CREATE INDEX idx_migration_candidates_run_status
  ON migration_candidates(run_id, status, id);
CREATE INDEX idx_migration_candidates_pending_index
  ON migration_candidates(indexed_at) WHERE indexed_at IS NULL AND status = 'committed';
```

`migration_provenance` conserva su forma y amplía el CHECK a los seis destinos. Su
`UNIQUE(org_id, destination_kind, source_identity)` es el mecanismo de idempotencia y **no se
toca**: es una restricción de base de datos, no una comprobación aplicativa que alguien pueda
olvidar en una rama.

`migration_review_actions` y `migration_outcomes` se recrean idénticas, incluidos los triggers
append-only (`RAISE(ABORT)` en UPDATE y DELETE) que ya eran correctos.

### 3.3 El corpus de documentación

```sql
CREATE TABLE doc_documents (
  id            TEXT PRIMARY KEY,
  org_id        TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  client_id     TEXT REFERENCES clients(id)  ON DELETE RESTRICT,
  project_id    TEXT REFERENCES projects(id) ON DELETE CASCADE,
  path          TEXT NOT NULL,
  content_sha   TEXT NOT NULL,
  scanned_at    TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(org_id, project_id, path)
);
CREATE TABLE doc_chunks (
  id           TEXT PRIMARY KEY,
  document_id  TEXT NOT NULL REFERENCES doc_documents(id) ON DELETE CASCADE,
  heading_path TEXT NOT NULL DEFAULT '',   -- "Engineering Process > Principios"
  anchor       TEXT NOT NULL,
  ordinal      INTEGER NOT NULL,
  content      TEXT NOT NULL,
  UNIQUE(document_id, anchor, ordinal)
);
CREATE TABLE doc_chunk_embeddings (
  chunk_id  TEXT PRIMARY KEY REFERENCES doc_chunks(id) ON DELETE CASCADE,
  embedding BLOB NOT NULL
);
```

Tabla propia, no `code_chunks` con una bandera. El corpus de código se pobló bajo la premisa de
que solo contiene código (`walker.rs:36-44`), y el trabajo de precisión de code-search se hizo
sobre esa premisa. Una columna `is_doc` obligaría a añadir un filtro en cada consulta de código
existente, y la que se olvide reintroduce el bug de ranking que ya se pagó una vez.

---

## 4. La frontera transaccional del commit

### 4.1 Por qué el commit es atómico por candidato, no por lote

`MemoryStore::store` (`store/sqlite.rs:62`) toma su propio `self.db.lock()` y no acepta un
handle de transacción. Envolver N llamadas en una transacción externa exigiría refactorizar la
capa de store entera.

Y aunque se pudiera, **no debería hacerse**. Un lote de 40 candidatos donde el número 17 falla
tiene dos semánticas posibles:

- *Todo o nada*: un candidato malo bloquea 39 buenos, y el operador no tiene forma de avanzar
  salvo rechazarlo.
- *Atómico por candidato*: 16 quedan dentro, el 17 se marca `failed` con su código de error, y
  los 23 restantes siguen. Re-correr el commit reintenta solo lo que falta, porque
  `migration_provenance` bloquea el re-commit de lo ya hecho.

La segunda es la correcta para una migración: **progreso parcial reanudable**. Es la misma
razón por la que la idempotencia se diseñó primero.

### 4.2 Una función de destino, dos llamadores

El requisito "Destination Persistence Reuse" prohíbe un camino de escritura paralelo. Pero
`SqliteStore::store` mezcla cuatro cosas: validar sesión, `upsert_memory`, `log_audit` y
vectorizar. El commit necesita las tres primeras dentro de su transacción y la cuarta fuera
(§4.3).

Solución: extraer el núcleo, sin duplicarlo.

```rust
// db/queries.rs — el núcleo transaccional, sin embeddings y sin lock.
pub fn store_memory_with_audit(
    conn: &Connection,          // puede ser una &Transaction
    org_id: &str,
    user_id: &str,
    req: &StoreMemoryRequest,
) -> Result<Memory> {
    if let Some(ref sid) = req.session_id {
        if !validate_session_ownership(conn, org_id, sid)? {
            anyhow::bail!("invalid_session_id:{sid}");
        }
    }
    let memory = upsert_memory(conn, org_id, user_id, req)?;
    let _ = log_audit(conn, org_id, user_id, "store", "memory", Some(&memory.id), /* … */);
    Ok(memory)
}
```

`SqliteStore::store` pasa a ser `lock` + `store_memory_with_audit` + vectorizar. El commit de
migración llama a `store_memory_with_audit` dentro de su transacción. Un solo cuerpo, dos
llamadores: no hay deriva posible entre el camino normal y el migrado.

Los otros cinco destinos ya exponen funciones que aceptan `&Connection` y sirven tal cual:

| `destination_kind` | Función |
|---|---|
| `memory` | `queries::store_memory_with_audit` *(extraída)* |
| `convention` | `queries::create_convention` (`queries.rs:14855`) |
| `task` | `queries::create_task` (`queries.rs:6801`) |
| `sdd_artifact` | `queries::upsert_sdd_artifact` (`queries.rs:8042`) |
| `harness` | `queries::create_harness` (`:1673`) + `publish_harness_version` (`:1778`) |
| `harness_config_review` | `queries::create_harness_config_review` (`:2057`) |

`log_audit` es un wrapper delgado sobre `insert_audit_log_chained` (`queries.rs:1428`), así que
toda escritura migrada entra en la cadena hash por tenant sin trabajo extra.

### 4.3 La vectorización va fuera de la transacción

Embeber es CPU-bound: decenas de milisegundos por texto. Mantener una transacción de escritura
abierta mientras se embebe bloquea a todos los escritores de SQLite durante todo el lote.

Por eso el orden es: transacción (destino + provenance + outcome) → commit → vectorizar
best-effort → `UPDATE migration_candidates SET indexed_at = …`.

Consecuencia honesta, y por eso el spec la exige explícitamente: **un candidato puede quedar
`committed` con `indexed_at IS NULL`**. Ocurre cuando no hay servicio de embeddings configurado
o cuando falla. El artefacto existe y es correcto; solo no es buscable por similitud todavía.
El índice parcial `idx_migration_candidates_pending_index` hace que encontrarlos sea barato, y
la reconciliación reusa el patrón de `bin/backfill_embeddings.rs`, que ya existe justamente
porque este caso siempre pudo ocurrir.

Prometer "todo queda vectorizado" como invariante duro sería mentir sobre una capa que ya es
best-effort por diseño (`store/sqlite.rs:96` — *"never fail the store call"*).

### 4.4 Descubierto al implementar: las funciones de destino ya abren su propia transacción

El diseño de arriba pedía **una transacción por candidato** envolviendo la escritura al destino
y el bookkeeping. **No es posible**, y el motivo merece quedar escrito porque es una trampa que
se dispara en silencio:

`log_audit` (a través de `insert_audit_log_chained`, `queries.rs:1350`) y `upsert_sdd_artifact`
llaman a `conn.unchecked_transaction()` internamente. SQLite no tiene transacciones anidadas, así
que el `BEGIN` interno falla. En `store_memory_with_audit` ese fallo lo traga el
`let _ = log_audit(..)` — es decir, la transacción externa **no hacía atómico el commit: apagaba
la auditoría**. Una memoria migrada habría aterrizado sin una sola fila de audit, y nadie se
habría enterado hasta que alguien preguntara.

Lo detectó `commit_writes_audit_row_per_destination`, que falló con 0 filas.

**El orden pasa a llevar la garantía**, y está elegido para fallar del lado seguro:

1. escribir el destino (cada destino gestiona su propia atomicidad);
2. solo entonces, provenance + status + outcome, juntos en una transacción.

Un fallo del destino deja por tanto **cero filas de provenance**, que es el invariante que
importa: un candidato fallido sigue siendo reintentable. La ventana contraria —destino escrito y
provenance no— es estrecha (el único fallo esperable es la violación de UNIQUE, que es en sí la
señal de idempotencia) y se reporta como `provenance_write_failed` en vez de esconderse.

> **El caso inverso, encontrado en la revisión adversarial**: un destino que **no** abre su
> propia transacción y necesita más de una escritura sí debe envolverse. `create_harness` +
> `publish_harness_version` son dos escrituras y un harness sin versión publicada no es un
> harness — nadie puede instalarlo y nada lo apunta. Van en una transacción. La regla completa
> es: *envuelve lo que no se envuelve solo; nunca envuelvas lo que ya lo hace.*

### 4.5 Pseudocódigo del commit

```rust
for cand in approved_candidates(run_id)? {
    // 1. Puerta de idempotencia, antes de gastar trabajo.
    if provenance_exists(&conn, org, cand.destination_kind, &cand.source_identity)? {
        record_outcome(&conn, &cand, Skipped, Some("already_committed"))?;
        continue;
    }
    // 2. El destino, fuera de toda transacción nuestra (§4.4).
    let dest_id = match write_destination(&conn, &run, &cand) {
        Ok(id) => id,
        Err(e) => { record_outcome(&conn, &cand, Failed, Some(&error_code(&e)))?; continue; }
    };
    // 3. El bookkeeping, esto sí atómico: ninguna de las tres sentencias abre
    //    una transacción propia.
    let tx = conn.unchecked_transaction()?;
    let booked = insert_provenance(&tx, org, cand.destination_kind, &cand.source_identity,
                                   &cand.id, &dest_id)
        .and_then(|_| set_status(&tx, &cand.id, Committed))
        .and_then(|_| record_outcome_in(&tx, &cand, Committed, None));
    match booked {
        Ok(()) => { tx.commit()?; to_index.push((cand.id, dest_id)); }
        Err(e) => { drop(tx); record_outcome(&conn, &cand, Failed, Some("provenance_write_failed"))?; }
    }
}
// 4. Vectorización posterior, best-effort.
for (cand_id, dest_id) in to_index { /* embed → store_embedding → set_candidate_indexed */ }
```

La comprobación de idempotencia del paso 1 es un atajo, no la garantía. La garantía es el
`UNIQUE` de `migration_provenance`: si dos commits corren en paralelo, uno recibe la violación
de unicidad y se registra como `skipped`. Confiar solo en el `SELECT` previo sería una condición
de carrera clásica.

---

## 5. Modelos Rust

```rust
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DestinationKind {
    Memory, Convention, Task, SddArtifact, Harness, HarnessConfigReview,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind { RepoDocs, GitHistory, ClaudeMemories, DbSchema, Noop }

#[derive(Debug, Deserialize, Clone)]
pub struct StageCandidatesRequest {
    pub candidates: Vec<CandidateInput>,     // lote; máximo configurable
}

#[derive(Debug, Deserialize, Clone)]
pub struct CandidateInput {
    pub source_identity: String,
    pub destination_kind: DestinationKind,
    pub content: String,
    #[serde(default)] pub destination_hint: serde_json::Value,
    #[serde(default)] pub source_excerpt: Option<String>,
    #[serde(default)] pub confidence: Option<f32>,
    #[serde(default)] pub normalized_metadata: serde_json::Value,
    #[serde(default)] pub provenance_kind: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ReviewActionRequest {
    pub candidate_id: String,
    pub action: ReviewVerdict,               // Approved | Rejected | Restaged
    pub expected_version: i64,               // obligatorio: sin él no hay concurrencia optimista
    #[serde(default)] pub reason: Option<String>,
    #[serde(default)] pub request_correlation_id: Option<String>,
}
```

`expected_version` es `i64`, no `Option<i64>`. Hacerlo opcional invitaría a omitirlo, y el
requisito de concurrencia optimista dejaría de tener efecto justo cuando importa: dos revisores
sobre la misma cola.

`destination_hint` es `serde_json::Value` y se valida **por destino** en el momento del commit,
no al stagear. Un hint de `harness` se valida con `validate_typed_harness_manifest`
(`types.rs:1924`); uno de `sdd_artifact` exige `capability` cuando el kind es `spec`. Validar
antes serviría de poco: entre staging y commit media una revisión humana que puede corregirlo.

---

## 6. El runner

### 6.1 El trait

```rust
pub trait Connector {
    fn source_kind(&self) -> SourceKind;

    /// Enumera unidades de origen. NO llama a ningún LLM.
    fn scan(&self, opts: &ScanOptions) -> anyhow::Result<Vec<SourceItem>>;

    /// Prompt de clasificación para una unidad. El conector conoce su dominio.
    fn classify_prompt(&self, item: &SourceItem) -> String;

    /// Candidato determinista sin LLM. Devuelve None si el origen necesita criterio.
    /// Es lo que permite `--no-llm` y lo que salva un run cuando el CLI falla.
    fn fallback(&self, item: &SourceItem) -> Option<CandidateInput>;
}

pub struct SourceItem {
    pub source_identity: String,   // determinista, calculado por el conector
    pub display_origin: String,    // legible para el revisor
    pub raw: String,
    pub meta: serde_json::Value,
}
```

`fallback` no es un adorno. Un conector que solo funciona con LLM es un conector que no funciona
cuando el CLI cambia de formato, cuando no hay red, o cuando un NDA prohíbe enviar el material.
`claude-memories` tiene un fallback fuerte (el frontmatter local ya trae el tipo);
`git-history` tiene uno débil (prefiltro determinista y contenido crudo).

### 6.2 El adaptador de `claude -p`

Toda la dependencia del CLI vive en **una** función:

```rust
fn classify_one(prompt: &str, cli: &ClaudeCli) -> anyhow::Result<ClassifierOutput> {
    let out = Command::new(&cli.bin)
        .args(["-p", prompt, "--output-format", "json"])
        .output()?;
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    Ok(ClassifierOutput {
        candidate: parse_candidate(&envelope)?,   // validado contra esquema tipado
        usage:     parse_usage(&envelope),        // Option: si cambia el formato, se pierde
    })                                            //   la métrica, no el run
}
```

Tres propiedades deliberadas:

1. **Aislamiento del formato.** Si una versión del CLI cambia el envoltorio JSON, rompe un test
   de fixture en esta función, no el pipeline. `runner_version` queda grabado en el run para
   poder correlacionar.
2. **Un fallo de parseo no aborta el run.** El item se registra como `failed` con la salida
   cruda adjunta, y el runner sigue. Un run de 500 documentos no puede morir por el documento
   número 3.
3. **La métrica de tokens es best-effort, el candidato no.** Perder la contabilidad es molesto;
   perder el candidato es perder trabajo.

### 6.3 Presupuesto y dry-run

`--dry-run` corre `scan()` completo y **cero** clasificaciones: reporta unidades, bytes y una
estimación de tokens. `--max-tokens N` aborta el run al superarse, dejando lo ya stageado
intacto y reanudable — un run abortado no es un run perdido, porque el staging ya está en el
backend.

---

## 7. El índice de documentación

`indexer/walker.rs` es code-only por decisión explícita y **no se modifica**. Se añade
`indexer/doc_walker.rs` como hermano:

```rust
const DOC_EXTENSIONS: &[&str] = &["md", "mdx"];

pub fn walk_docs(root: &Path, opts: &DocWalkOptions) -> anyhow::Result<Vec<FileMeta>>
```

Reusa la maquinaria de `ignore` (`.gitignore`, `.git/`, `node_modules`, `target`) que el walker
de código ya monta, con el allowlist invertido. Alimenta al `MarkdownChunker`
(`chunker.rs:111`), que ya parte por encabezados y mantiene la jerarquía de secciones, y que
hasta hoy no tenía llamador.

Excludes por defecto, sobreescribibles: `**/node_modules/**`, `**/CHANGELOG.md`,
`**/LICENSE*`, y las rutas de marketing y research que `knowledge-migration-repo-docs` declara.

---

## 8. Superficie HTTP y RBAC

| Método y ruta | Permiso | Notas |
|---|---|---|
| `POST /v1/migrations` | `migration:write` | crea el run; scope inmutable desde aquí |
| `POST /v1/migrations/:id/candidates` | `migration:write` | lote; devuelve stageados/saltados/rechazados |
| `GET  /v1/migrations/:id/candidates` | `migration:read` | cola de revisión, filtrable por destino y estado |
| `POST /v1/migrations/:id/review` | `migration:review` | acciones con `expected_version` |
| `POST /v1/migrations/:id/commit` | `migration:review` | commitea los aprobados |
| `POST /v1/migrations/:id/cancel` | `migration:write` | cancela lo pendiente; lo commiteado no se toca |
| `GET  /v1/migrations/:id/report` | `migration:read` | conteos y motivo por candidato no commiteado |
| `GET  /v1/migrations` | `migration:read` | listado, filtrable por cliente y fuente |
| `GET  /v1/docs/search` | `memory:read` | búsqueda documental |
| `GET  /v1/docs/index-status` | `memory:read` | pendientes de vectorizar |

`migration:review` es un permiso **distinto** de `migration:write`. Quien corre el escaneo y
quien decide qué entra al cerebro de la empresa no tienen por qué ser la misma persona, y en
una consultoría normalmente no lo son.

La visibilidad de runs y candidatos pasa por el predicado de cliente/proyecto de v58
(`VISIBLE_PROJECT_IDS` + `client_members`, `u2s-client-model/design.md` §3). No se escribe un
predicado nuevo: el aislamiento entre clientes ya tiene una única definición canónica y
duplicarla es cómo se envían fugas.

---

## 9. Tests — orden TDD

`strict_tdd: true` para backend y admin. Test primero en los seis PRs.

**PR 1 — `run_v60`** (`db/migrations.rs`, módulo de tests)
1. `run_v60_aborts_when_v56_tables_have_rows` — el guard, primero. Es el test que evita una
   pérdida de datos, y va antes que la funcionalidad.
2. `run_v60_is_idempotent` — correrla dos veces no cambia nada.
3. `run_v60_candidate_accepts_six_destination_kinds` / `..._rejects_unknown_kind`.
4. `run_v60_run_has_no_destination_kind_column`.
5. `run_v60_client_scope_trigger_aborts_cross_org`.
6. `run_v60_scope_is_immutable_after_insert`.
7. `run_v60_provenance_unique_blocks_second_commit`.
8. `run_v60_review_actions_reject_update_and_delete`.
9. `integration_test.rs` — actualizar el `assert_eq!(version, 59)` a 60. *(Rompió en el change
   anterior por olvidarlo; ahora es un paso explícito.)*

**PR 2 — staging y revisión**
10. `stage_rejects_duplicate_source_identity_in_run`.
11. `stage_rejects_unknown_destination_kind`.
12. `review_with_stale_expected_version_is_rejected_and_recorded`.
13. `review_increments_candidate_version`.
14. `review_without_permission_records_permission_denied`.
15. `batch_approval_refuses_when_client_attested_present`.
16. `rejected_candidate_is_not_restaged_by_identical_rescan`.

**PR 3 — commit**
17. `commit_only_processes_approved_candidates`.
18. `commit_is_atomic_per_candidate_and_batch_is_resumable` — el corazón de §4.1.
19. `commit_skips_when_provenance_exists_and_continues_batch`.
20. `commit_failure_leaves_no_provenance_row`.
21. `commit_writes_audit_row_per_destination`.
22. `commit_memory_is_invisible_to_other_client` — aislamiento, criterio de aceptación.
23. `commit_harness_rejects_invalid_manifest_without_creating_harness`.
24. `commit_succeeds_without_embed_service_and_leaves_indexed_at_null`.

**PR 4 — índice documental**
25. `doc_walker_admits_markdown_and_respects_gitignore`.
26. `doc_walker_excludes_code_files`.
27. `code_search_results_unchanged_after_doc_indexing` — la regresión que este diseño existe
    para no reintroducir.
28. `doc_chunks_preserve_heading_path`.
29. `reconciliation_vectorizes_pending_and_updates_state`.

**PR 5 — runner**
30. `noop_connector_stages_and_commits_end_to_end`.
31. `classifier_parse_failure_marks_item_failed_and_continues`.
32. `usage_parse_failure_does_not_fail_the_item`.
33. `dry_run_performs_zero_classifications`.
34. `token_budget_exceeded_aborts_leaving_staged_intact`.
35. `backend_pipeline_succeeds_with_no_model_credentials` — verifica BYOM de punta a punta.

**PR 6 — admin** — vitest sobre la cola: orden por confianza, bloqueo del lote con
`client_attested`, conflicto de versión mostrado al revisor, y visibilidad de la cita de origen.

---

## 10. Notas operativas y rollout

1. **PR 1 aterriza el esquema sin cablear rutas.** Nada cambia de comportamiento; si algo va
   mal, revertir es quitar una migración que nadie usa todavía.
2. **PR 2 y 3 dejan el pipeline usable sin runner**, vía la API. Se puede validar con `curl`
   antes de que exista ningún conector.
3. **El conector `noop`** —candidatos fijos, sin filesystem ni LLM— es lo que hace testeable el
   pipeline en CI sin instalar Claude Code en el runner de GitHub Actions.
4. **Coste de la primera pasada real**: 161 documentos en este repo. Correr `--dry-run` y
   revisar la estimación antes del primer run de verdad no es opcional.
5. **`migration_runs.source_ref` va redactado en origen.** Nunca contiene un DSN, ni un token,
   ni una ruta de home con nombre de usuario.

---

## 11. Diferencias respecto a la revisión 2 del `proposal.md`

Escribir el design descubrió dos suposiciones falsas del proposal. Ambas se corrigieron también
en el proposal (revisión 3), y quedan registradas aquí porque el porqué importa:

| Proposal decía | Realidad verificada | Qué se hace |
|---|---|---|
| "Rebuild de `migration_candidates` y `migration_provenance` para ampliar `destination_kind`" | `migration_candidates` **no tiene** `destination_kind`; está en `migration_runs`, que asumía un destino por run | Recreación guardada de las 5 tablas; `destination_kind` **se mueve** del run al candidato (§3.1, §3.2) |
| "Commit **transaccional** a los seis destinos" | `MemoryStore::store` toma su propio lock; y todo-o-nada es además la semántica equivocada para una migración | **Atómico por candidato**, lote reanudable (§4.1) |

Y una precisión que el proposal dejaba implícita: la vectorización es **best-effort y posterior
al commit**, no parte de él (§4.3). Un candidato puede quedar `committed` sin vector, y para eso
existen `indexed_at` y la reconciliación.

**Tercera corrección, descubierta al implementar el PR 3** (§4.4): ni siquiera la escritura al
destino puede ir dentro de una transacción nuestra, porque `log_audit` y `upsert_sdd_artifact`
abren la suya. Envolverlas no daba atomicidad: apagaba la auditoría en silencio. El orden
—destino primero, bookkeeping transaccional después— es lo que sostiene el invariante.

**Cuarta, de la revisión adversarial** (§4.4, nota): la regla completa tiene dos mitades.
Un destino que abre su propia transacción no se envuelve; uno que necesita varias escrituras y
no abre ninguna —`create_harness` + `publish_harness_version`— sí debe envolverse, o un fallo a
mitad deja un harness sin versión publicada.
