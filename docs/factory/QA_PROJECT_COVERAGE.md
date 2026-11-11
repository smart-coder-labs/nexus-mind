# QA por proyecto e histórico durable

Entrega aislada sobre `main` (`d0014a27900ff32f190891df7a637cbf82c96b83`). No activa agentes, publica bugs de prueba ni despliega. Las dependencias workflows/runtime ya están en esa base; no se incluyen sus copias locales antiguas.

## Alcance

- QA conserva el target web y permite elegir proyecto y repositorios de diagnóstico autorizados. Los comandos fijados siguen usando un checkout concreto.
- Varios repos pueden vincularse al mismo proyecto. Un vínculo indexado no concede autoridad: los grants explícitos y el snapshot por run limitan lectura y publicación. Antes de publicar se revalidan membresía, grant, credencial y capacidad actuales; un destino ambiguo queda en triage.
- Escenarios `passed`, `failed` y `blocked`, inventario conocido y plan por intento se guardan fuera de la retención de runs. Los checkpoints estructurados conservan progreso antes del final; pendientes y bloqueos no equivalen a cobertura ejecutada.
- La selección usa historia acotada, áreas no ejecutadas y controles críticos; no afirma cobertura exhaustiva. Un bloqueo sin cambios no debe repetirse en bucle.
- Historia paginada por agente/proyecto/aplicación/entorno y resultados por run en las vistas existentes. El destino manual se elige del snapshot autorizado, no de un texto libre.
- Las entregas fijan repositorio y revisión originales, usan claims y recuperan issues por marker antes de otro POST. La búsqueda falla cerrada si no puede completar el escaneo acotado.
- Se redactan valores conocidos antes de certificar escenarios, eventos, findings y transcripciones. Filas anteriores sin procedencia verificada quedan en cuarentena.

## Registro de migraciones: base publicada 98 → QA 110

No reutilizar números de los experimentos locales. `main` ya tiene migraciones ajenas a QA hasta **98**. Su código y semántica se conservan.

| Número local no publicado | Número de esta entrega | Función / efecto |
|---|---|---|
| 91 | 99 | `db::qa_coverage::migrate`: resultados durables |
| 92 | 100 | misma función: destino fijo de entregas |
| 93 | 101 | misma función: marca de redacción de resultados |
| 94 | 102 | misma función: marca de redacción de transcripciones |
| 95 | 103 | misma función: grants y snapshots de repos |
| 96 | 104 | misma función: marcas de eventos y findings |
| 97 | 105 | misma función: checkpoints por intento |
| 98 | 106 | misma función: inventario y planes |
| 99 | 107 | misma función: identidad del resultado por intento |
| 100 | 108 | misma función: blocker del checkpoint y claims de entrega |
| 101 | 109 | `db::migrations::run_v109`: `requested_by`, backfill desde creador |
| 102 | 110 | `db::migrations::run_v110`: orden global de observaciones |

`db::migrations::run_all` registra QA después de `workspace_secrets::migrate` (98), seguido por 109 y 110. No presupone tablas QA por el `user_version` de otro módulo. El bootstrap de QA crea también las columnas/tabla de secuencia; 110 ordena los hechos históricos por fecha, checkpoint antes de final en empate y después id de origen. El replay reutiliza el orden original; el inventario deriva del hecho más reciente, mientras el plan conserva el resultado del **propio intento**, incluso con inventario ausente.

## Validación local y gates

- `db::migrations::tests::qa_upgrade_from_real_main98_matches_fresh110_and_is_idempotent` reproduce las 95 funciones publicadas de la base 98, comprueba ausencia inicial de QA, upgrade completo, igualdad con bootstrap nuevo, `requested_by`, esquema factory intacto y rerun sin cambiar hechos/secuencia.
- `tests/qa_coverage_replay_test.rs` conserva los cinco probes R3 (sólo renumeración 101/102 → 109/110), más replay con plan, inventario presente/ausente y preservación de hechos, tiempos y secuencia.
- La regresión fuente `replayed_final_keeps_original_order_and_inventory_precedence` incluye plan y sus dos variantes. `v110_backfill_orders_checkpoint_and_result_events_globally` cubre el backfill canónico.

**Producción: migración aplicada por el usuario.** El antiguo requisito local «schema102» no identifica este esquema: en la línea publicada de esta PR se requiere **110 y sus tablas/columnas exactas**, no sólo un entero. No ejecutar migration runner, rollout/restart ni backend nuevo (el arranque invoca `run_all`) antes de aplicación/confirmación independiente por el usuario y verificación de compatibilidad. No se proporciona aquí un comando de producción automático.

La PR debe permanecer **draft**, pendiente de verificación acotada de los dos ajustes de integración (binding del estado y registro de migraciones), CI/review/protecciones y aplicación de migración por el usuario. No autoriza merge, deploy, activación QA ni pruebas pagas/live.
