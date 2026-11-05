# Publicación: Pi administrado · 2026-11-05

## Desplegado y comprobado

- SHA: `ea81f936853f4979abbc9704650c7e0668698a60`.
- [Factory workflows release 42006713831](https://github.com/smart-coder-labs/nexus-mind/actions/runs/42006713831): siete jobs correctos, incluidos verificación, imágenes, backup/rollout del backend y publicación del admin.
- Backend/worker: `ghcr.io/smart-coder-labs/nexusmind-backend@sha256:2d2597f220f5a9be758befa8ec1932d4e4dc37f7e15acd71824904b2325c74e4`; esquema **97**, sin migración nueva.
- Admin servido coincide byte a byte con el artefacto CI: `assets/index-p_MlMvW-.js`, SHA256 `0927cb96f419d9275bc93dad269cc8c91105b48fdf03ac7a98ce48fa0f3f8fdf`.
- Runner: `/home/ubuntu/.local/lib/nexusmind/runner-ea81f93/`; servicio activo, `NRestarts=0`. Backup privado del JSON y servicio: `/home/ubuntu/.local/state/nexusmind/runtime-backups/20261105T101534Z/`.
- El SDK se construyó automáticamente desde la receta revisada en ARM64, sin contexto de repositorio ni credenciales. ID local: `sha256:7400ba364509574839c96cc5df264e856a603ffe47eea2ba5c5adc7a6cba1463`.
- Activados únicamente `allow_backend_models` y `auto_provision_agent_runtime` en el JSON privado existente. No se copiaron archivos OAuth, sesiones ni credenciales del backend al runner. No se modificaron repositorios, permisos, schedules, colas ni revisiones de agentes.
- La organización autorizada explícitamente obtiene `GET /v1/factory/runner/model-runtime → ready=true`, `authentication=backend_oauth`, harness `pi`.
- Un POST al modelo sin lease activo recibió **409 lease_conflict**, sin inferencia ni reserva de cuota. No se creó una ejecución de aceptación ni se simuló un lease productivo.

## Pruebas

- Biblioteca Rust local: **1866 correctas, 4 ignoradas**; integraciones: **26 correctas**. `cargo check --locked --all-targets` correcto.
- Python final: **65 correctas**, con SDK Docker real, dependencias Node offline y proveedor de modelo simulado. Pruebas del puente repetidas con CLI Pi real y, en Oracle, SDK ARM64: **6 correctas** por ejecución.
- Admin: **133 correctas**, TypeScript y build correctos. Vista previa ilustrativa y read-only revisada a 390 y 1440 px, sin overflow ni errores JS finales. La primera vista previa falló por dependencias compartidas/cacheadas; se instaló el lockfile en el worktree independiente y se reinició Vite antes de contar la revisión visual.
- Se comprobó también que el SDK del backend resuelve el OAuth existente mediante `ModelRuntime.getAuth`, emitiendo únicamente un booleano al operador.
- Estos checks **no acreditan** una inferencia real a través de una ejecución productiva ni el QA de Kasymir. El proveedor de las pruebas de contenedor era un fixture.

El [CI general 42006722674](https://github.com/smart-coder-labs/nexus-mind/actions/runs/42006722674) sigue fallando en los mismos cinco lints Clippy de la base: `automation/worker.rs:2204`, `db/workflows.rs:635,650`, `db/workflow_execution.rs:106`, `db/queries.rs:18634`. Se comparó con [41785355226](https://github.com/smart-coder-labs/nexus-mind/actions/runs/41785355226), anterior a este cambio: mismas ubicaciones y errores. No se desactivó Clippy ni se afirma que todo el CI del repositorio esté verde.

## Lo que sigue bloqueado, sin maquillaje

**La revisión guardada sigue siendo 8 y la aplicada sigue siendo 0.** Se preservaron las selecciones del usuario:

| Fase | Harness / modelo | Infraestructura |
|---|---|---|
| Investigación | Claude / Opus | Sigue sin imagen/conexión/red; no se cambió a Pi de forma silenciosa. |
| Tests de regresión | Pi / gpt-6-luna | SDK automático, conexión `backend`, `network=none`. |
| Implementación | Pi / gpt-6.1-sol | SDK automático, conexión `backend`, `network=none`. |
| Revisión independiente | Pi / gpt-6-luna | SDK automático, conexión `backend`, `network=none`. |

El rechazo `runtime_image_not_allowed` que permanece corresponde a la **fase Claude de investigación**, no a las tres fases Pi ni al SDK Node. El perfil completo no se marca como aplicado parcialmente. La lista histórica `image:` no identifica la fase: no se debe interpretar como que falta descargar de nuevo el SDK Pi.

Para elegir Pi también en investigación, el admin ofrece **«Usar Pi del backend en las cuatro fases»**. Esa acción muestra el cambio de modelo antes de guardar y conserva checks/servicios y los modelos Pi ya elegidos. No requiere escribir imágenes ni copiar una sesión. La publicación no pulsó ese botón ni sustituyó el modelo Claude/Opus elegido por el usuario.

Además siguen faltando **tests, QA funcional, diseño/accesibilidad, seguridad y servicios**. Solo constan build de AppUI y typecheck de accounting. AppUI continúa teniendo una dependencia privada sin conector autorizado. Instalar un harness no inventa estas configuraciones ni sus resultados. Los presets completos, el alcance por proyecto y el diagnóstico de persistencia de formularios siguen pendientes; no se presentan como entregados aquí.

No se arrancó el workflow completo, no se publicaron PRs de aceptación, no se aprobó ni fusionó código y no se desplegaron aplicaciones Kasymir.
