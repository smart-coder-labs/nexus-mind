# Tasks: Knowledge Migration Core

## Process — strict TDD applies

`openspec/config.yaml` declares `strict_tdd: true` with `tdd_scope: backend_and_admin`, and **this change honors it**. Test first in every task.

The waiver granted to `u2s-client-model` (owner decision, 2026-08-13) does not carry over. There is one task where the ordering is not a preference but the whole point: **T-01, the emptiness guard of `run_v60`**. That test is what stands between a `DROP TABLE` and someone's data. It is written first, it is watched to fail, and only then does the migration exist.

---

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | 2600–3200 |
| 400-line budget risk | Very high |
| Chained PRs recommended | Yes |
| Suggested split | 6 PRs (see units) |
| Delivery strategy | auto-chain |
| Chain strategy | stacked-to-main |

Decision needed before apply: No
Chained PRs recommended: Yes
Chain strategy: stacked-to-main
400-line budget risk: Very high

### Suggested Work Units

| Unit | Goal | Likely PR | Notes |
|------|------|-----------|-------|
| 1 | `run_v60` + tipos | PR 1 | **Aterriza el esquema sin cablear rutas.** Cero cambio de comportamiento; revertir es quitar una migración que nadie usa. |
| 2 | Queries + API de staging y revisión | PR 2 | Deja el pipeline usable por `curl` antes de que exista ningún conector. |
| 3 | Commit a los 6 destinos | PR 3 | **La unidad de riesgo.** Toca `store/sqlite.rs`, que hoy usa toda escritura de memoria. Cada test existente debe seguir verde. |
| 4 | Índice documental | PR 4 | Independiente de 2 y 3; podría adelantarse si conviene. Cablea `MarkdownChunker`. |
| 5 | Runner + trait de conector | PR 5 | Único punto con dependencia del CLI externo. |
| 6 | UI de revisión | PR 6 | Aditivo; nada existente cambia de forma. |

---

## Phase 1: Esquema (PR 1)

- [x] T-01: Test del guard de emptiness de `run_v60` — **primero, antes de escribir la migración**
  - Files: `src/db/migrations.rs` (módulo de tests)
  - Scope: `run_v60_aborts_when_v56_tables_have_rows`. Monta una BD en v59, inserta **una** fila en cada una de las cinco tablas de v56 (un caso por tabla), corre `run_v60` y exige `Err` con el nombre de la tabla y el conteo en el mensaje.
  - Por qué primero: es el test que decide si una migración destruye datos o se niega a correr. Verlo fallar antes de que exista `run_v60` es la única forma de saber que prueba algo.
  - Gate: el test debe fallar por "función no existe", no por otra razón.

- [x] T-02: Implementar `run_v60` + llamada desde `run_all`
  - Files: `src/db/migrations.rs`
  - Scope: guard de emptiness; `DROP` + `CREATE` de las 5 tablas de v56 en orden inverso de FK; forma nueva según `design.md` §3.2; tablas `doc_documents`, `doc_chunks`, `doc_chunk_embeddings` (§3.3); triggers de scope de cliente e inmutabilidad; índices, incluido el parcial `idx_migration_candidates_pending_index`.
  - Tests: `run_v60_is_idempotent`, `run_v60_run_has_no_destination_kind_column`, `run_v60_candidate_accepts_six_destination_kinds`, `run_v60_candidate_rejects_unknown_destination_kind`, `run_v60_client_scope_trigger_aborts_cross_org`, `run_v60_scope_is_immutable_after_insert`, `run_v60_provenance_unique_blocks_second_commit`, `run_v60_review_actions_reject_update_and_delete`, `run_v60_creates_doc_tables`.
  - Nota sobre idempotencia: seguir el patrón de `run_v58_is_idempotent` — forzar `PRAGMA user_version = 59` antes de re-ejecutar, porque si no el guard de versión devuelve temprano y el test no prueba nada.

- [x] T-03: Bump de las aserciones de versión hardcodeadas
  - Files: `src/db/migrations.rs`, `tests/integration_test.rs`
  - Scope: **son 30 sitios, no uno** (medido al aplicarlo): 27 de una línea y **2 multilínea** en `migrations.rs`, más `tests/integration_test.rs:330`. Las multilínea no las alcanza una sustitución de una sola línea y solo aparecen al correr los tests. Dos aserciones NO deben cambiar: `run_v59_is_idempotent` (fuerza la BD a 58) y el assert del propio guard de T-01.
  - Gate: `cargo test` verde antes de abrir el PR.

- [x] T-04: Tipos de migración
  - Files: `src/models/types.rs`
  - Scope: `DestinationKind`, `SourceKind`, `MigrationRun`, `MigrationCandidate`, `ReviewVerdict`, `StageCandidatesRequest`, `CandidateInput`, `ReviewActionRequest`, `RunReport`, `StageResult`.
  - Detalle no negociable: `ReviewActionRequest::expected_version` es `i64`, **no** `Option<i64>` (`design.md` §5). Un test lo pinea: `review_request_without_expected_version_fails_to_deserialize`.
  - Tests: roundtrip de serde por tipo; `destination_kind_rejects_unknown_string`; `source_kind_serializes_kebab_case`.

**Gate PR 1**: `cargo test` verde; `cargo clippy -- -D warnings` limpio. Nada montado en el router: el comportamiento de la app no cambia.

---

## Phase 2: Staging y revisión (PR 2)

- [x] T-05: `db/migration_queries.rs` — runs
  - Files: `src/db/migration_queries.rs` (nuevo)
  - Scope: `create_run`, `get_run`, `list_runs_visible`, `set_run_status`, `cancel_run`.
  - `list_runs_visible` **reusa** `VISIBLE_PROJECT_IDS` + la pertenencia por cliente de v58 (`u2s-client-model/design.md` §3). No se escribe un predicado de visibilidad nuevo.
  - Tests: `create_run_rejects_project_from_other_org`, `create_run_accepts_null_client_as_internal`, `list_runs_hides_other_clients`, `cancel_run_leaves_committed_candidates_untouched`.

- [x] T-06: `db/migration_queries.rs` — staging de candidatos
  - Scope: `stage_candidates` (lote), `list_candidates`, `get_candidate`.
  - Devuelve por candidato: `staged` | `skipped(reason)` | `rejected(reason)`. Un candidato inválido **no** aborta el lote.
  - Tests: `stage_rejects_duplicate_source_identity_in_run`, `stage_rejects_unknown_destination_kind`, `stage_skips_previously_rejected_identity`, `stage_partial_batch_reports_per_candidate`.

- [x] T-07: `db/migration_queries.rs` — acciones de revisión
  - Scope: `apply_review_action` con concurrencia optimista; `list_review_actions`.
  - Contrato: comparar `expected_version`; si no casa, **registrar** una acción `stale_version` con `expected_version` y `resulting_version` y devolver conflicto. Si casa, aplicar e incrementar `version`.
  - Tests: `review_with_stale_expected_version_is_rejected_and_recorded`, `review_increments_candidate_version`, `review_records_actor_and_authorization`, `restage_appends_action_without_erasing_rejection`.

- [x] T-08: RBAC — `migration:read`, `migration:write`, `migration:review`
  - Files: `src/api/middleware.rs` (o donde viva el registro de permisos), `src/db/queries.rs` si los roles se siembran
  - Scope: tres permisos distintos. `migration:review` **no** se implica desde `migration:write` (`design.md` §8).
  - Tests: `review_without_permission_records_permission_denied`, `write_permission_does_not_grant_review`.

- [x] T-09: `api/migrations.rs` — handlers de run, staging y revisión
  - Files: `src/api/migrations.rs` (nuevo), `src/api/router.rs`
  - Scope: `POST /v1/migrations`, `POST|GET /v1/migrations/:id/candidates`, `POST /v1/migrations/:id/review`, `POST /v1/migrations/:id/cancel`, `GET /v1/migrations`, `GET /v1/migrations/:id/report`.
  - Aprobación en lote: rechazar el lote entero si contiene algún `client_attested`, identificando cuáles (spec — Constrained Batch Approval).
  - Tests (integración): `batch_approval_refuses_when_client_attested_present`, `batch_approval_succeeds_for_verified_manifest`, `report_explains_every_skip`, `rejected_candidate_is_not_restaged_by_identical_rescan`.

**Gate PR 2**: el pipeline es ejercitable con `curl` de punta a punta salvo el commit. Aserción explícita: ningún candidato ha llegado a ningún destino todavía.

---

## Phase 3: Commit (PR 3)

- [x] T-10: Extraer `queries::store_memory_with_audit`
  - Files: `src/db/queries.rs`, `src/store/sqlite.rs`
  - Scope: mover el núcleo de `MemoryStore::store` (validación de sesión + `upsert_memory` + `log_audit`) a una función que acepta `&Connection` y **no** toma el lock ni embebe. `SqliteStore::store` pasa a ser `lock` + esa función + vectorizar (`design.md` §4.2).
  - **Refactor puro: cero cambio de comportamiento.** Toda la suite existente de memoria debe seguir verde sin tocar un solo test. Si algún test necesita cambiar, el refactor está mal.
  - Tests: los existentes son la prueba. Añadir `store_memory_with_audit_writes_audit_row` y `store_memory_with_audit_works_inside_a_transaction`.

- [x] T-11: Dispatch de destinos
  - Files: `src/db/migration_queries.rs`
  - Scope: `write_destination(&Transaction, &MigrationRun, &MigrationCandidate) -> Result<String>` con los seis brazos de la tabla de `design.md` §4.2. Validación del `destination_hint` **por destino, en commit-time**: `validate_typed_harness_manifest` para `harness`; `capability` obligatoria para `sdd_artifact` de kind `spec`.
  - Tests: uno por destino que verifica que el registro aterriza donde debe; `commit_harness_rejects_invalid_manifest_without_creating_harness`; `commit_sdd_spec_without_capability_fails_candidate`.

- [x] T-12: Bucle de commit
  - Files: `src/db/migration_queries.rs`, `src/api/migrations.rs`
  - Scope: `POST /v1/migrations/:id/commit` según el pseudocódigo de `design.md` §4.4 — puerta de idempotencia, transacción por candidato, `rollback` que no deja provenance parcial, outcome por candidato.
  - Tests: `commit_only_processes_approved_candidates`, `commit_is_atomic_per_candidate_and_batch_is_resumable`, `commit_skips_when_provenance_exists_and_continues_batch`, `commit_failure_leaves_no_provenance_row`, `commit_writes_audit_row_per_destination`, `commit_twice_produces_no_duplicate_destination`.
  - `commit_is_atomic_per_candidate_and_batch_is_resumable` es el test central de la unidad: lote de 3 donde el del medio falla → 2 commiteados, 1 `failed`, y re-correr commitea solo el que faltaba.

- [x] T-13: Aislamiento por cliente — criterio de aceptación
  - Files: `tests/` (integración)
  - Scope: no es una tarea de implementación sino de prueba, y no es opcional. El entregable de este pipeline escribe conocimiento de clientes bajo NDA; la única evidencia de que el cliente A no ve lo del B es un test que lo intenta.
  - Tests: `commit_memory_is_invisible_to_other_client` sobre cuatro superficies — search, list, context, y `GET /v1/migrations`. Cada denegación deja fila de auditoría.

- [x] T-14: Vectorización posterior al commit
  - Files: `src/db/migration_queries.rs`, `src/db/queries.rs` (`set_candidate_indexed`)
  - Scope: tras cerrar cada transacción, embeber best-effort y sellar `indexed_at`. Nunca dentro de la transacción (`design.md` §4.3).
  - Tests: `commit_succeeds_without_embed_service_and_leaves_indexed_at_null`, `commit_with_embed_service_sets_indexed_at`.

**Gate PR 3**: suite completa verde, incluidos los ~1010 tests de lib preexistentes. T-10 es refactor puro y cualquier test de memoria que cambie es una señal de alarma, no un ajuste.

---

## Phase 4: Índice documental (PR 4)

- [x] T-15: `indexer/doc_walker.rs`
  - Files: `src/indexer/doc_walker.rs` (nuevo)
  - Scope: `DOC_EXTENSIONS = ["md","mdx"]`; reusa la maquinaria de `ignore` del walker de código con el allowlist invertido; excludes por defecto (`node_modules`, `CHANGELOG.md`, `LICENSE*`).
  - **`indexer/walker.rs` no se toca.** Es code-only por decisión documentada (`walker.rs:36-44`).
  - Tests: `doc_walker_admits_markdown_and_respects_gitignore`, `doc_walker_excludes_code_files`, `doc_walker_applies_default_excludes`.

- [x] T-16: `db/doc_queries.rs` + cableado del `MarkdownChunker`
  - Files: `src/db/doc_queries.rs` (nuevo), `src/indexer/mod.rs`
  - Scope: `upsert_document`, `replace_chunks`, `store_chunk_embedding`, `search_docs`, `list_pending_index`, `reconcile_embeddings`. `index_documents()` junto a la indexación de código.
  - `MarkdownChunker` (`chunker.rs:111`) ya existe y está testeado — **solo se cablea, no se escribe**.
  - Tests: `doc_chunks_preserve_heading_path`, `reindexing_same_document_replaces_chunks_without_duplicates`, `reconciliation_vectorizes_pending_and_updates_state`.

- [x] T-17: Test de no-regresión del corpus de código
  - Files: `tests/` (integración)
  - Scope: `code_search_results_unchanged_after_doc_indexing`. Indexar código, capturar resultados y ranking de N consultas, indexar documentación, repetir, exigir igualdad exacta.
  - Es la regresión concreta que este diseño existe para no reintroducir: READMEs rankeando por encima de handlers reales.

- [x] T-18: `api/docs.rs`
  - Files: `src/api/docs.rs` (nuevo), `src/api/router.rs`
  - Scope: `GET /v1/docs/search`, `GET /v1/docs/index-status`.
  - Tests: `docs_search_returns_no_code_chunks`, `index_status_reports_pending_count`.

**Gate PR 4**: T-17 verde es la condición de merge. Si el ranking de código cambia, el corpus no está realmente separado.

---

## Phase 5: Runner (PR 5)

- [x] T-19: Trait `Connector` y tipos del runner
  - Files: `src/bin/migrate_knowledge.rs` (nuevo)
  - Scope: `Connector` con `source_kind`, `scan`, `classify_prompt`, `fallback`; `SourceItem`, `ScanOptions`, `ClassifierOutput` (`design.md` §6.1).
  - `fallback` es parte del trait, no un opcional: un conector que solo funciona con LLM no funciona sin red, sin CLI, o bajo un NDA que prohíba enviar el material.

- [x] T-20: Conector `noop`
  - Scope: candidatos fijos, sin filesystem ni LLM. Es lo que hace testeable el pipeline en CI sin instalar Claude Code en el runner de GitHub Actions.
  - Tests: `noop_connector_stages_and_commits_end_to_end`.

- [x] T-21: Adaptador de `claude -p`
  - Scope: `classify_one` como **única** función que conoce el formato del CLI (`design.md` §6.2). Salida validada contra esquema tipado; `parse_usage` devuelve `Option`.
  - Tests con fixtures de salida real del CLI: `classifier_parses_valid_envelope`, `classifier_parse_failure_marks_item_failed_and_continues`, `usage_parse_failure_does_not_fail_the_item`, `cli_version_is_recorded_on_run`.

- [x] T-22: Presupuesto, dry-run y reporte de uso
  - Scope: `--dry-run` (scan completo, cero clasificaciones, estimación); `--max-tokens N` que aborta dejando lo stageado intacto; `POST /v1/usage` por invocación.
  - Tests: `dry_run_performs_zero_classifications`, `token_budget_exceeded_aborts_leaving_staged_intact`, `runner_reports_usage_with_client_and_project`.

- [x] T-23: Verificación de BYOM de punta a punta
  - Files: `tests/` (integración)
  - Scope: `backend_pipeline_succeeds_with_no_model_credentials`. Levantar el backend sin **ninguna** variable de modelo, stagear vía API, revisar, commitear. Verifica el requisito "Backend Model Independence" y el principio de `docs/ENGINEERING_PROCESS.md:14`.

**Gate PR 5**: CI verde sin Claude Code instalado en el runner. Si el pipeline necesita el CLI para pasar tests, la frontera no está donde el diseño dice.

---

## Phase 6: UI de revisión (PR 6)

- [x] T-24: Tipos y cliente de API en admin
  - Files: `apps/admin/src/types.ts`, `apps/admin/src/api/client.ts`
  - Tests: vitest de serialización; `tsc -b` limpio.

- [x] T-25: `pages/Migrations.tsx`
  - Files: `apps/admin/src/pages/Migrations.tsx` (nuevo), `App.tsx`, `Layout.tsx`
  - Scope: lista de runs; cola de candidatos agrupada por fuente y destino, ordenada por confianza; panel de candidato con contenido, **cita literal de origen**, destino propuesto y hint; aprobar/rechazar individual y en lote.
  - Tests: `queue_orders_by_confidence`, `batch_button_disabled_when_client_attested_present`, `candidate_panel_shows_source_excerpt`, `version_conflict_shows_reload_prompt`.

- [x] T-26: Copy de los dos gates
  - Scope: donde un candidato `harness` se aprueba, el texto debe distinguir las dos preguntas: aprobar la migración es *"esto pasa a ser herramienta del equipo"*; la instalación la aprueba después quien la recibe. Sin ese texto, un revisor asume que está autorizando la ejecución.
  - Tests: `harness_candidate_shows_install_gate_notice`.

**Gate PR 6**: `tsc -b` rc=0, `npm run build`, vitest verde.

---

## Gates globales

| Gate | Comando |
|---|---|
| Backend tests | `cargo test --manifest-path apps/backend/Cargo.toml` |
| Backend lint | `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings` |
| Backend format | **NO correr `cargo fmt` a secas.** El repo no está rustfmt-clean: reformatea 55 archivos y ahoga el diff. Escribir el código ya formateado; el CI valida clippy, no formato. Ver `apply-progress.md` §1. |
| Admin tests | `cd apps/admin && npm run test` |
| Admin types | `cd apps/admin && npx tsc -b` |
| Admin build | `cd apps/admin && npm run build` |

**Revisión adversarial antes del PR** (`~/.claude/CLAUDE.md` — Code Review): `judgment-day` o `arch-review` sobre cada PR de este change. No es exento: no hay diff trivial aquí.

---

## Riesgos de ejecución

| Riesgo | Dónde muerde | Mitigación |
|---|---|---|
| **T-10 rompe la escritura de memorias.** `SqliteStore::store` es el camino de toda memoria del sistema. | PR 3 | Refactor puro con la suite existente como red. Si un test de memoria necesita cambiar, revertir y repensar. |
| **Las 30 aserciones de versión** se descubren a mitad del PR 1. | PR 1 | T-03 las hace explícitas. Ya pasó en `u2s-client-model`. Confirmado al aplicar: 2 son multilínea y solo aparecen al correr los tests. |
| **Un conector real se cuela en el core.** | PR 5 | Solo `noop`. Cualquier conector con filesystem o red pertenece a su propio change. |
| **El coste de la primera pasada real.** 161 documentos solo en este repo. | post-merge | `--dry-run` obligatorio antes del primer run de verdad. |
| **El commit resulta más lento de lo tolerable** con transacción por candidato. | PR 3 | Medir con el `noop` a 500 candidatos antes de dar por buena la unidad. Si duele, la salida es batching por lotes pequeños, no volver a todo-o-nada. |
| **Espacio en disco.** El `target/` de cargo con fastembed y tree-sitter es grande, y este change añade compilaciones. | cualquiera | Ya bloqueó una sesión (2026-08-15). Vigilar antes de un `cargo build` largo. |
