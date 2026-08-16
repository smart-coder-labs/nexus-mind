# Apply Progress — Knowledge Migration Core

> **Change**: `knowledge-migration-core`
> **Branch**: `sdd/knowledge-migration`
> **Started**: 2026-08-15

---

## PR 1 — Esquema (T-01 … T-04) — ✅ completo

| Tarea | Estado | Resultado |
|---|---|---|
| T-01 Test del guard de emptiness | ✅ done 2026-08-15 | 2 tests. Escrito primero; falló con `cannot find function run_v60` antes de existir la implementación. |
| T-02 `run_v60` + `run_all` | ✅ done 2026-08-15 | Migración + 9 tests. |
| T-03 Bump de aserciones de versión | ✅ done 2026-08-15 | **30 sitios**, no ~26. |
| T-04 Tipos de migración | ✅ done 2026-08-15 | 11 tipos + 7 tests. |

### Verificación

| Gate | Resultado |
|---|---|
| `cargo test` | **1058 lib + 46 integración + 60 en otros binarios de test — 0 fallos** |
| `cargo clippy -- -D warnings` | limpio (rc=0) |
| Archivos tocados | 3: `src/db/migrations.rs`, `src/models/types.rs`, `tests/integration_test.rs` |
| Diff | +1045 / −55 |

Tests netos añadidos: **+18** en la librería (1040 → 1058).

### Qué quedó en el árbol

- `run_v60` — guard de emptiness sobre las 5 tablas de v56, `DROP`+`CREATE` en orden inverso
  de FK, 3 tablas de documentación, 3 triggers de scope/inmutabilidad, 5 índices (uno parcial).
- 11 tests de migración (`run_v60_*`) más 1 test de v56 reescrito.
- 11 tipos en `models/types.rs` (`DestinationKind`, `SourceKind`, `MigrationRun`,
  `MigrationCandidate`, `CreateMigrationRunRequest`, `CandidateInput`, `StageCandidatesRequest`,
  `StageResult`, `ReviewVerdict`, `ReviewActionRequest`, `RunReport` + `RunReportEntry`).
- Nada montado en `router.rs`: **el comportamiento de la aplicación no cambia.**

---

## Desviaciones respecto al plan, y por qué

### 1. `cargo fmt` reformatea el repo entero — no lo corras sin acotar

`openspec/config.yaml` lista `cargo fmt` como gate de calidad, pero **el repo no está
rustfmt-clean**: un `cargo fmt` a secas produjo un diff de **55 archivos**, la mayoría sin
relación alguna con este change (`api/code.rs` solo ya eran +575/−205).

Se revirtió por completo y se reaplicaron únicamente los tres archivos de la tarea. El código
nuevo se escribió ya formateado y `cargo clippy -- -D warnings` pasa igual.

**Para el equipo**: el CI corre clippy, no `cargo fmt --check` (`.github/workflows/ci.yml`), que
es por lo que la deriva de formato ha podido acumularse. Formatear todo es un change propio y
merece su propio PR; colarlo dentro de otro hace irrevisable el diff real.

### 2. Dos tests de v56 codificaban justo el contrato que v60 revierte

No fue una sorpresa, pero sí trabajo no planificado:

- **`migration_run_scope_allows_only_v1_destination_matrix`** afirmaba que un run de
  `convention` con `project_id` debía ser rechazado. v60 elimina esa restricción a propósito —
  `conventions.project_id` existe y es nullable (`migrations.rs:1372`), así que v56 prohibía una
  fila que la tabla destino siempre aceptó. **No se borró**: se reescribió como
  `migration_candidates_mix_destinations_within_one_project_scoped_run`, que pinea el contrato
  nuevo (el run lleva scope, el candidato lleva destino, y la convención de proyecto es legal)
  y conserva el caso negativo del destino desconocido. El doc comment explica qué se revirtió.
- **`migration_provenance_is_org_scoped_and_review_actions_are_append_only`** sigue siendo
  válido en intención; solo sus `INSERT` usaban `destination_kind` en el run. Actualizados.

### 3. Fueron 30 aserciones de versión, no ~26

- 27 de una línea (`get_user_version(&conn), 59`) en `migrations.rs`
- **2 multilínea** que la sustitución de una línea no alcanzó y solo aparecieron al correr los
  tests — `run_all_sets_user_version_to_43` y `run_all_sets_user_version_to_59`
- 1 en `tests/integration_test.rs:330`

Dos **no** debían cambiar y se preservaron:
- `run_v59_is_idempotent` fuerza la BD a 58 y corre solo v59 → 59 sigue siendo lo correcto.
- El assert del propio guard (`"an aborted v60 must not advance user_version"`) → 59 a propósito.

Además, `run_all_sets_user_version_to_59` se renombró a `..._to_60`: un test cuyo nombre miente
sobre lo que comprueba es peor que no tenerlo.

### 4. El guard inserta filas huérfanas con `PRAGMA foreign_keys = OFF`

Las cinco tablas cuelgan unas de otras por FK, así que "una fila en `migration_candidates`"
implicaría también una en `migration_runs`. Para probar cada tabla **aislada**, el helper
desactiva las FK alrededor del insert. Es deliberado: el guard debe dispararse con una fila
huérfana, no solo con un grafo bien formado.

El test también comprueba que tras el abort la fila **sigue ahí** y `user_version` **no avanzó**
— porque "aborta" y "aborta sin destruir nada" no son la misma garantía.

---

## PR 2 — Staging y revisión (T-05 … T-09) — ✅ completo

`db/migration_queries.rs` (nuevo) + `api/migrations.rs` (nuevo) + RBAC + 6 rutas.
**30 tests** (18 data layer, 12 handlers).

Decisiones tomadas al implementar:

- **La visibilidad no se reescribió.** `user_can_view_run` compone
  `queries::user_can_view_client` con la vista `project_visibility` que ya existía. El
  `VISIBLE_PROJECT_IDS` que citaba el design **no existe** en el repo — el mecanismo real son esas
  dos piezas. Un run sin cliente ni proyecto es trabajo interno y lo ve toda la org.
- **`admin` no es `super_user`.** El fixture de operador tuvo que ser `super_user`: admin es
  privilegiado para permisos pero sigue acotado por membresía en las lecturas
  (`viewer_scope`, igual que `api::clients`). Un admin que no pertenece a ningún cliente no puede
  abrir un run contra él, y eso es correcto.
- **Clippy obligó a agrupar parámetros.** `record_action` (9 argumentos) y `apply_review_action`
  (8) pasaron a `ActionRecord` y `ReviewRequest`. A esa anchura, intercambiar dos `Option<&str>`
  seguía compilando — y el trail de revisión es evidencia.

## PR 3 — Commit (T-10 … T-14) — ✅ completo

`store_memory_with_audit` extraída + dispatch de 6 destinos + bucle de commit + aislamiento +
vectorización posterior. **10 tests nuevos.**

- **T-10 fue refactor puro y se verificó como tal**: 1088 tests verdes y **ni un solo test tuvo
  que cambiar**. Ese era el criterio.
- **Hallazgo que cambió el diseño**: `log_audit` y `upsert_sdd_artifact` abren su propia
  transacción. Envolverlas en la nuestra no daba atomicidad — **apagaba la auditoría en silencio**,
  porque el fallo del `BEGIN` anidado lo traga el `let _ = log_audit(..)`. Lo detectó
  `commit_writes_audit_row_per_destination` fallando con 0 filas. Ver `design.md` §4.4.

## PR 4 — Índice documental (T-15 … T-18) — ✅ completo

`indexer/doc_walker.rs` + `db/doc_queries.rs` + `index_documents()` + `api/docs.rs`.
**19 tests nuevos.**

- **`indexer/walker.rs` no se tocó.** El walker de documentación es un hermano con el allowlist
  invertido, no un flag en el existente.
- **`MarkdownChunker` solo se cableó** — llevaba construido y testeado desde el trabajo de
  code-search sin un solo llamador.
- `code_search_results_unchanged_after_doc_indexing` compara el corpus de código byte a byte
  antes y después de indexar documentación. Es la condición de merge del PR.
- Las rutas se guardan **relativas al root del escaneo**: una ruta absoluta llevaría el directorio
  home del operador a un corpus que lee toda la organización.

## PR 5 — Runner (T-19 … T-23) — ✅ completo

`bin/migrate_knowledge.rs` con el trait `Connector`, el conector `noop`, el adaptador de
`claude -p`, presupuesto y dry-run. **11 tests.**

- **Solo `noop` existe.** `connector_for` rechaza los cuatro reales por nombre explicando dónde
  viven, y un test lo mantiene así.
- **`fallback` es parte del trait, no un opcional.** Un conector que solo funciona con LLM deja de
  funcionar sin red, sin CLI, y bajo un NDA que prohíba enviar el material.
- **Toda la dependencia del CLI vive en `parse_candidate` + `parse_usage`**, cubiertas por
  fixtures del envoltorio. Si una versión futura cambia el formato, rompe un test aquí.
- **La identidad la estampa el conector, no el clasificador**: `classify` sobrescribe
  `source_identity` con la del item. Un modelo que parafrasee no puede romper la idempotencia.

## PR 6 — UI de revisión (T-24 … T-26) — ✅ completo

`pages/Migrations.tsx` + tipos + 6 métodos de cliente + ruta y navegación. **10 tests.**

- **Bug real encontrado por los tests**: `loadCandidates` limpiaba el estado que su llamador
  acababa de escribir, así que el aviso de conflicto de versión y el resultado del commit
  desaparecían al instante. El reset pasó a ser explícito.
- **T-26 (el copy de los dos gates) tiene su propio test.** Sin ese texto un revisor asume que
  está autorizando la ejecución de un hook en la máquina de otro.

---

## Revisión adversarial — 4 hallazgos, todos corregidos

Corrida antes del PR según el protocolo del repo. Detalle y severidades en `verify-report.md` §6.

| Severidad | Hallazgo | Corrección |
|---|---|---|
| 🟠 Mayor | Un fallo al publicar la versión dejaba un **harness huérfano** sin versión publicada. | `create_harness` + `publish_harness_version` envueltos en una transacción — ninguna de las dos abre la suya, verificado. Es el caso **inverso** al de §4.4 y conviene decirlo: los destinos que gestionan su propia transacción no se envuelven; los que no, sí. |
| 🟠 Mayor | El run se marcaba `completed` con candidatos aún por revisar. | `completed` ahora exige cola vacía; si queda algo, vuelve a `in_review`. |
| 🟡 Menor | Cancelar un run ya completado reescribía su estado. | Rechazado con `run_already_completed`. |
| 🟡 Menor | El conteo de pendientes de indexar reportaba su propio `LIMIT`. | `count_pending_index`, un `COUNT` de verdad. |

Cada corrección lleva su test de regresión. **Veredicto: aprobado, sin bloqueantes.**

---

## Verificación final

Ver `verify-report.md` para la cobertura requisito a requisito. Resumen:

| Gate | Resultado |
|---|---|
| `cargo test` | 1121 lib + 46 integración + 11 runner + 60 otros — 0 fallos |
| `cargo clippy -- -D warnings` | limpio |
| `npm run test` (admin) | 261 passed |
| `npx tsc -b` / `npm run build` | rc=0 / ✓ |

---

## Pendiente

- [x] Revisión adversarial — hecha, 4 hallazgos corregidos.
- [ ] Commit y PR.
- [ ] Los cuatro conectores reales, cada uno en su change (`-repo-docs`, `-git-history`,
      `-claude-memories`, `-db-schemas`), todavía en fase `propose`.
- [ ] **La pregunta del NDA sigue sin responder.** No bloquea este change; bloquea
      `--include-data` en `db-schemas` y el modo LLM de `git-history`.
