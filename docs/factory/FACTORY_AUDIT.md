# Auditoría funcional y de navegación de Factory

## Alcance

**Auditoría previa a la consolidación.** Sus pendientes describen la base `98b86c8`, no el estado del trabajo local posterior. Para la implementación, permisos y limitaciones actuales, consulta [Factory conectada](FACTORY_CONNECTED.md). Nada de esa consolidación está desplegado todavía.

Revisión estática del admin y sus rutas, consultas, workers y políticas, sobre `feat/factory-admin-guided-setup` (base local `98b86c8`). No es una nueva aceptación end-to-end ni un análisis de frecuencia de uso en producción. No se ejecutaron modelos, merges, despliegues ni cambios productivos para esta auditoría.

Distinguir tres cosas: implementación real, configuración operativa y uso efectivo por usuarios. La existencia de una API no demuestra las otras dos. El rediseño y runtime guiado siguen sin desplegar; las conexiones agente-catálogo/harness por fase aún están pendientes. GitHub Issues en entradas está en desarrollo, no validado como terminado.

## Diagnóstico principal

Factory reúne dos motores que todavía no están integrados completamente:

1. Agentes autónomos: definiciones por plantilla, revisiones, schedules, worker del backend, runs y findings.
2. Workspaces/workflows: runner remoto, inventario confirmado, DAG, snapshots, fases, artefactos y publicaciones.

Ambos tienen piezas reales. El problema de producto es presentar sus pantallas como si fueran un único sistema: existen dos historiales, dos intake y varias fuentes de configuración de ejecución. No recomiendo borrar el motor anterior sin migrar sus funciones ni sus datos.

## Sección por sección

| Sección | Uso comprobado en código | Limitación | Recomendación |
|---|---|---|---|
| Agentes | Crear, validar, habilitar, duplicar, ejecutar y programar definiciones por plantilla. | Todavía no es el catálogo que seleccionan las fases del workflow. La compatibilidad depende de plantilla, executor y credenciales. | Mantener como catálogo único y conectar referencias versionadas desde workflows. |
| Workspaces & workflows | Inventario, autorización de repositorios, DAG, leases, aprobación, evidencia y publicación. | El runner de Kasymir tiene preparación validada, no aceptación completa de modelos y QA. El formulario actual aún no reutiliza catálogo/harness por fase. | Mantener como centro de orquestación; separar setup de la operación diaria. |
| Runs | Historial real de agentes, logs, transcripciones, verificación, cancelación y continuación. | Consulta runs autónomos; no es el historial global de workflows. | Unificar la vista con filtros y detalles específicos de cada tipo; no borrar logs ni evidencias. |
| Findings | Resultados persistidos, resolución, issues y seguimiento de entregas. | Mezcla hallazgos técnicos con lead generation, contenidos y LinkedIn. No representa todos los resultados de fases del workflow. | Mantener para QA/review; separar resultados de marketing o filtrar por dominio. |
| Needs a human | Decisiones de merge, agentes bloqueados, resultados por revisar, tareas pendientes, feedback y costes. | El digest consulta el circuito anterior; no agrega las aprobaciones y bloqueos de workflows. Además mezcla bandeja con analítica. | Mantener una bandeja de aprobaciones global; mover costes y análisis fuera de las decisiones urgentes. |
| Intake | Fuentes y credenciales reales, historial, creación de tareas, selección de resolver y watchdog de Slack. | Es otro circuito respecto a las entradas directas de workflows: crea tareas y usa un resolver, en vez de apuntar al DAG. | Unificar fuentes y elegir destino explícito: workflow o agente. Migrar, no eliminar antes de preservar funciones. |
| Autonomy policies | Persistencia y evaluación por acción; una autorización de fix no implica autorización de merge. El workflow consulta este motor. | Complejidad elevada para operación cotidiana; requiere explicar alcance, prioridades y aprobaciones. | Mantener la funcionalidad, dentro de Configuración → Permisos y aprobaciones. |
| Decision model | Registros shadow, métricas por clase y etiquetas humanas, con infraestructura Jev. | No es necesario para ejecutar un workflow normal; requiere modelo/datos. El consentimiento se comparte con caminos opt-in de autoarranque, sujetos a políticas y floors. | Mover a Laboratorio/Avanzado. No describir toda integración Jev como incapaz de actuar bajo cualquier configuración. |
| Factory settings | Salud del worker, Pi OAuth, pausa de agentes autónomos, retención y sandbox bot. | No es un panel global del runner remoto. La pausa y la retención existentes no cubren automáticamente las tablas de workflows. | Mantener y ampliar a Configuración → Infraestructura; corregir etiquetas de alcance. |
| Evaluaciones | API de conjuntos congelados, solicitudes, estados y resultados baseline/specialist. | Exige runner y datasets externos instalados; el endpoint bloquea si faltan. No equivale a tests/build/QA del repositorio. | Mantener como Laboratorio, visible según capacidades; no como requisito para iniciar tareas normales. |
| Modelos y roles | Guarda preferencias y muestra estado del gateway. | El getter devuelve `execution_ready: false`; las preferencias no están conectadas a ejecución por rol. Los caminos Pi/Nexus son independientes. | No venderlo como configuración operativa. Integrar o sustituir por selección efectiva de harness/modelo; ocultar la sección independiente mientras esté sin cablear. |
| Plantillas (ruta existente, sin entrada principal en sidebar) | Catálogo/esquemas reales para crear agentes. | Se solapa con la creación desde Agentes. | Conservar como biblioteca dentro de Agentes, sin añadir otro menú principal. |

## Hallazgos prioritarios

### P0 — Alcance engañoso de «Pause all agents»

`FactorySettings.tsx` habla de detener todos los agentes de la organización. El ajuste escribe `organizations.autonomous_agents_enabled`; las consultas del worker autónomo lo comprueban y sus runs se cancelan. No encontré esa comprobación en el control plane/claim de workflows. No debe presentarse como parada global de toda Factory mientras existan ambos motores sin una pausa común.

La retención implementada elimina tablas `autonomous_agent_*`, no los artefactos y runs `factory_workflow_*`. Tampoco debe anunciarse como retención de toda Factory.

### P1 — Modelo guardado no equivale a modelo ejecutable

`db/factory_model_config.rs::StoredModelConfig::new` fija `execution_ready: false`. `automation/worker.rs::execute_claim` consulta esa señal para resolver/reviewer con Claude/Codex y bloquea si el gateway por roles no está cableado. Pi/Nexus siguen caminos propios; no heredan automáticamente las preferencias globales. Es una integración incompleta, no una mera pantalla duplicada.

### P1 — Dos historiales y una bandeja incompleta

`RunsPage.tsx` usa `listAutonomousAgentRuns`; los workflows usan otro catálogo/detalle. `db/factory_ops.rs::human_digest` agrega decisiones y runs del circuito anterior, no las aprobaciones de workflows. Un operador puede ver una bandeja vacía y tener trabajo bloqueado en el otro motor.

### P1 — Dos intake con destinos distintos

`FactoryIntake.tsx` usa fuentes/items del intake anterior. `automation/factory_intake.rs` crea una tarea NexusMind y deriva al resolver. `automation/workflow_intake.rs` inicia directamente un workflow identificado por su definición. Son funciones reales pero no una configuración única. Debe existir una sola fuente con destino visible y deduplicación coherente.

### P2 — Laboratorio mezclado con operación

Shadow decisions, métricas de clasificación y comparaciones de specialists son útiles para mejorar autonomía, pero no ayudan a completar el primer fix en Kasymir. Ocultarlos del menú diario no significa quitarles APIs, datos o pruebas.

## Menú recomendado

**Operación**
- Workflows (workspace activo y configuración inicial dentro).
- Agentes (plantillas dentro).
- Entradas (destino: agente o workflow).
- Ejecuciones (ambos motores).
- Hallazgos (QA/review, con filtros de dominio).
- Pendientes (aprobaciones y bloqueos de ambos motores).

**Administración**
- Configuración: runners, harnesses, credenciales/conectores, modelos por defecto, presupuestos, políticas, pausa y retención con alcance explícito.

**Laboratorio, opcional**
- Evaluaciones.
- Modelo de decisión/shadow.

No recomiendo eliminar funciones de seguridad, datos históricos o el motor autónomo. Lo innecesario es mantener accesos principales duplicados y formularios que aparentan configurar un sistema distinto del que ejecuta.

## Orden de trabajo

1. Corregir alcance de pausa/retención; definir parada global segura.
2. Conectar catálogo de agentes, harnesses y modelos efectivos con snapshots por ejecución, sin copiar credenciales.
3. Unificar ejecuciones y bandeja de pendientes.
4. Unificar fuentes/intake con destino explícito.
5. Consolidar configuración y mover laboratorio fuera del camino diario.
6. Solo después, retirar accesos redundantes conservando compatibilidad y datos.

## Evidencia principal

- Navegación: `apps/admin/src/components/Layout.tsx`, `apps/admin/src/App.tsx`.
- Historial: `apps/admin/src/pages/factory/runs/RunsPage.tsx` y `.../workflows/WorkflowsPage.tsx`.
- Bandeja: `apps/admin/src/pages/FactoryDigest.tsx`, `apps/backend/src/db/factory_ops.rs`.
- Preferencias sin gateway: `apps/backend/src/db/factory_model_config.rs`, `apps/backend/src/automation/worker.rs::execute_claim`.
- Pausa/retención: `apps/admin/src/pages/factory/FactorySettings.tsx`, `apps/backend/src/db/queries.rs::patch_autonomous_agent_org_settings` y `cleanup_autonomous_agent_retention`; comparar con `db/workflows.rs` y `db/workflow_execution.rs`.
- Intake: `apps/admin/src/pages/FactoryIntake.tsx`, `apps/backend/src/automation/factory_intake.rs`, `apps/backend/src/automation/workflow_intake.rs`.
- Evaluaciones: `apps/backend/src/api/factory_evals.rs`.
- Políticas y opt-in Jev: `apps/backend/src/factory/workflow_policy.rs`, `apps/admin/src/pages/factory/DecisionModel.tsx` y `ShadowRouterPanel.tsx`.
