# Plan: ejecución de workflows observable de principio a fin

**Estado: entregas 0–5 implementadas.** La 0 está en producción (`6411373`); las
1–5 están en la rama `feat/workflow-observability-1-5` (ver «Implementación de las
entregas 1–5»). **Sin migración:** todo se apoya en `factory_workflow_events`
(`seq` + `payload_json`) y los artefactos existentes.
**Base inspeccionada:** main `e3716af`; revisado de nuevo contra main `17fc776`
tras el bloqueo `adapter_os_error_HTTPError` (ver «Incidente que motivó la entrega 0»).

## Objetivo

Abrir un workflow y seguir toda su ejecución, en vivo y después de terminar:
qué hizo cada agente, qué herramientas llamó, qué comandos corrieron, qué
respondieron los contenedores/servicios y qué ocurrió en backend y runner.
Sin depender de SSH ni de un JSON final que pierde el contexto del fallo.

«Todos los logs» significa los relacionados con esa ejecución y autorizados para
el usuario; no un volcado global del servidor ni datos de otras organizaciones.
No incluye razonamiento privado del modelo, secretos o cuerpos de autenticación.

## Qué existe y qué falta

- `ExecutionDetail.tsx` muestra fases, resultados, artefactos y eventos, pero no
  ofrece un transcript de agente por fase ni una consola unificada por intento.
- Runs individuales ya tienen `TranscriptView` y `RunLogs`, con parsing de
  mensajes/herramientas. Se reutilizarán: no crear otro visor de chat.
- El runner captura salida del contenedor y archivos `agent.stdout.log`,
  `agent.stderr.log` y logs de checks. Sube artefactos después de la ejecución.
- `execution.monitor()` publica colas de salida cada cinco segundos; puede saltar
  bytes intermedios si hay mucho output. No es un stream completo verificable.
- Un error anterior al contenedor o durante cleanup/upload puede dejar sin subir
  el contexto más importante. Los checks acumulan stdout hasta terminar.
- El endpoint de eventos acepta solo `kind` y `message`, con 4096 bytes y una cuota
  de unos 3000 eventos/run. No identifica estructuradamente origen/intento/revisión.
- El detalle devuelve los últimos 1000 eventos, sin navegación del historial completo.
- Artefactos ya distinguen fase e intento y tienen digest: 1 MiB por archivo y
  32 MiB totales por run. No basta con aumentar estas cuotas indiscriminadamente.
- Los logs de backend/systemd/Kubernetes no están enlazados a esta vista.
- Las llamadas HTTP del runner al backend (claim, lease, eventos, artefactos,
  relay de modelo, publicación, complete) no tienen diagnóstico propio. Si la
  respuesta de error no es JSON se pierden status, ruta y hora; el step queda como
  `adapter_os_error_HTTPError` con una solución genérica de «operación del sistema».
- `monitor()` envía `log.chunk` y renueva el lease en el mismo bucle y por la misma
  ruta de error: un 502 puntual en un log o la cuota `event_quota` abortan el step.
- Los handlers del backend responden siempre JSON `{code, error}` vía `fail()`, pero
  Traefik (502/503/504) y los rechazos de axum (413 por límite de body, 404/405 sin
  cuerpo) no. Un runner más nuevo que el backend recibe un 404 vacío indistinguible.
- `hold` no significa «no reintentar»: un fallo con `hold: false` en una fase
  Check/Review encola una **reparación del agente**. El runner ponía `hold: false`
  a 408/429/5xx con cuerpo JSON, así que un backend caído podía disparar una
  reparación de código que no estaba roto. No existe reintento automático de fase.

## Incidente que motivó la entrega 0

Resultado bloqueado observado el 2026-11-06 en el workspace Kasymir (48 repos):
`{"error":"adapter_os_error_HTTPError","exception_type":"HTTPError","hold":true}`,
con `evidence: []` y `heads` iguales a la entrada (no se escribió nada).

Cadena causal en `runner.py`: `urllib.error.HTTPError` hereda de `OSError` con
`errno=None`, así que `exception_diagnostic` usa el nombre de la clase. El bloque
posterior que extrae `code` del cuerpo solo actúa si el cuerpo es JSON; si no,
`except (ValueError, OSError): pass` conserva el diagnóstico genérico. Como el
backend siempre responde JSON, el error vino del borde (Traefik) o de un rechazo de
axum, no de un handler. Sin status ni request ID no es posible saber cuál sin SSH.

## Experiencia que vamos a entregar

### Vista general

Timeline de fases y dependencias, con estado, inicio/fin/duración, intento,
revisión candidata, repositorios/SHA, agente/revisión, executor/modelo e imagen.
Distinguir fase pendiente, espera de aprobación, bloqueo, fallo, cancelación y
runner sin heartbeat. Ninguna espera debe parecer una ejecución activa silenciosa.

### Al abrir una fase

1. **Resumen:** qué iba a hacer, estado, código de error, causa concreta, repo y
   archivo/comando afectados, salida del proceso y siguiente acción necesaria.
2. **Conversación:** mensajes y herramientas del agente, como en runs individuales.
3. **Consola:** stdout/stderr, comandos, instalación/build/tests y eventos del runtime.
4. **Servicios e infraestructura:** logs de aplicación/dependencias, backend y
   runner pertinentes, reinicios/evicciones y fallos de conectividad correlacionados.
5. **Artefactos:** logs completos conservados, evidencias, capturas, resultados y diffs.

Selector de intento y revisión; filtros por origen, repositorio, servicio y severidad;
búsqueda, timestamps UTC, seguir en vivo/pausar y descarga del historial redactado.
Una consola combinada no debe sustituir la posibilidad de ver cada origen por separado.

## Implementación por entregas

### 0. Fallos del plano de control: visibles y no fatales

Va primero porque hoy bloquea ejecuciones reales y es pequeña: solo runner y una
capa del router, sin migraciones ni cambios de UI.

- **Diagnóstico HTTP del runner.** Todo `HTTPError`/`URLError` de `api.call` produce
  `adapter_http_<status>` (o `adapter_network_<tipo>` sin respuesta) con método,
  plantilla de ruta sin IDs ni query (`/runs/:run/steps/:step/events`), si el cuerpo
  era JSON, `code` del backend cuando exista y el request ID. Nunca headers ni cuerpo
  crudo. Solución fija por clase: borde/backend no disponible (502/503/504),
  versión incompatible (404/405), tamaño (413), cuota (429), lease (409/410).
- **Request ID obligatorio.** El runner envía `X-Request-Id` por llamada y lo
  imprime en su journal junto al diagnóstico; el backend lo registra en el span de
  `tracing` de la petición. Con él se localiza la línea en Traefik y backend.
- **Logs best-effort.** En `monitor()` y `log_event`, un fallo al enviar `log.chunk`
  o `event_quota` no interrumpe el proceso: se cuenta, se marca hueco y se sigue.
  Solo un lease perdido (o su renovación fallida tras reintentos) detiene el step.
- **Reintento en la capa HTTP.** Llamadas idempotentes (lease, eventos, artefactos
  con digest, `complete` con journal) reintentan 408/429/502/503/504 y errores de
  red con backoff acotado por el lease restante. Esto no es un reintento del step
  ni del workflow; los modelos, comandos y publicaciones nunca se repiten.
- **Todo fallo del plano de control mantiene `hold: true`** (nunca dispara una
  reparación) y añade `retryable` para indicar si reintentar la fase es seguro.
- **Respuestas JSON en todo `/v1/factory/runner/*`.** Fallback de ruta y manejo de
  rechazos de axum (`route_not_found`, `method_not_allowed`, `body_too_large`,
  `invalid_json`) con el mismo `ApiError`, para que un desfase de versiones tenga nombre.
- **Versión del runner.** El runner informa su versión de contrato en `/claim`; el
  backend rechaza versiones incompatibles con `runner_version_unsupported` antes de
  entregar trabajo, en lugar de fallar a mitad de un step.

**Aceptación:** con fallos inyectados (502 HTML, 404 vacío, 413 de axum, `event_quota`,
timeout de red) el step muestra status, ruta y request ID; un 502 puntual en un
`log.chunk` no cambia el resultado del step; un 503 sostenido termina sin `hold`;
ningún diagnóstico contiene tokens, URLs autenticadas ni cuerpos de respuesta.

**Implementada** en la rama `feat/workflow-control-plane-diagnostics`: módulo
`scripts/factory/control_plane.py`, `RunnerApi.call` con request ID y reintentos,
`log_event` best-effort, capability `runner-contract:1` y middleware
`runner_diagnostics` en el backend. Desviación respecto al texto inicial: el
contrato viaja como capability (el cuerpo de `/claim` usa `deny_unknown_fields`, y
un campo nuevo rompería un backend antiguo) y un 413 de axum se nombra
`body_too_large` en el extractor JSON además del middleware.

### 1. Contrato de diagnóstico e historial completo

- Identidad común: `org_id`, `run_id`, `step_id`, `attempt`, `input_revision` y
  repo/SHA; añadir servicio/contenedor/proceso cuando existan. El request ID de la
  entrega 0 es obligatorio en cada registro originado por una llamada al backend.
- El backend deriva organización, fase, intento y revisión de la autorización y
  lease; no acepta que el productor cambie esos valores arbitrariamente.
- Registros con secuencia de ingestión, hora de origen/recepción, origen, stream,
  severidad y payload tipado. No confiar en el reloj de distintos hosts para ordenar.
- Endpoint paginado por cursor para logs/eventos/transcripts; filtros dentro del
  run autorizado, sin devolver todo dentro del detalle en cada refresco.
- Conservar el contrato antiguo y probar ambos formatos. Actualizar backend antes
  que runner; validar allowlist y límites reales para no repetir `invalid_event`.
- Añadir causa estructurada a bloqueos tempranos: por ejemplo repo, lockfile y clave
  no portable, o rama solicitada/inexistente, no solo el mensaje genérico.
- Separar presupuesto de logs del de eventos de control: el output no debe impedir
  guardar un resultado, una cancelación o un diagnóstico de error.

**Aceptación:** consultar más de 1000 eventos sin perder los iniciales; distinguir
inequívocamente dos intentos y dos revisiones; rechazar acceso cruzado entre orgs.

### 2. Captura fiable de todas las fases

- Instrumentar preparación de repositorios/snapshot, dependencias, agentes,
  herramientas, checks, build/arranque de servicios, healthchecks, publicación,
  revisión y cleanup. Capturar también fallos anteriores al contenedor.
- Hacer streaming incremental de checks, stdout y stderr, no solo del resultado.
- Guardar los eventos públicos del executor como JSONL estructurado; adaptar los
  formatos soportados al visor existente, sin convertirlos en texto ilegible.
- Recolectar logs del contenedor y servicios antes de destruirlos; guardar exit
  code, señal, timeout y estado final/OOM cuando el runtime lo proporcione.
- Persistir y subir diagnóstico en rutas de éxito, excepción, timeout y cancelación.
  El fallo de exportación no debe ocultar la causa original.
- Lectura secuencial por offsets, sin saltos al tail. Cola local privada y acotada
  para tolerar caída temporal del API; reenvío idempotente para no duplicar líneas.
- Los reenvíos de logs nunca vuelven a ejecutar modelos, comandos ni publicaciones.
- Indicar explícitamente huecos, truncamiento, límites y pérdida por caída abrupta.

**Aceptación:** agente y check muestran salida mientras siguen corriendo; los
logs sobreviven a un fallo/cancelación; tras reconectar no hay duplicación ni
huecos silenciosos. Probar el error del lockfile antes de llegar a `npm ci`.

### 3. Integración del panel

- Extender `ExecutionDetail`, no crear otra pantalla operativa desconectada.
- Reutilizar `TranscriptView`/`RunLogs` con adaptadores de datos pequeños.
- Cargar datos incrementalmente por fase/intento y solo al abrirlos; polling con
  cursor como primera opción, sin introducir WebSockets para este caso.
- Listas acotadas/renderizado progresivo; seguimiento automático que se pausa
  cuando el usuario navega hacia arriba y conserva posición al llegar más datos.
- Mostrar historial antiguo disponible y marcar «no registrado en esta versión»
  cuando no existe transcript. No inventar ni reconstruir contenido inexistente.
- Diferenciar «no hay salida», «cargando», «sin permiso», «conexión perdida»,
  «registro incompleto» y «fase no ejecutada».
- Descarga paginada de logs redactados; snapshot/SHA e IDs incluidos en el paquete
  de diagnóstico para poder reproducir y compartir el problema.

**Aceptación:** desde la fase bloqueada se llega al comando/mensaje causante sin
SSH ni inspección manual de JSON; resultado y transcript no mezclan revisiones.

### 4. Logs de backend, runner e incidentes de infraestructura

- Registrar diagnósticos estructurados de operaciones del backend vinculadas al
  run: claim/lease, relay/modelo, políticas, API GitHub, evidencias y publicación.
- Correlacionar con `tracing` y los spans existentes; no duplicar ciegamente todo
  el log del proceso en cada ejecución ni persistir headers/bodies con credenciales.
- Capturar del runner sus eventos de ciclo de vida y conectividad. Los fallos HTTP
  de la entrega 0 se muestran en la fase como «backend no disponible a las HH:MM»
  o «versión incompatible», enlazados por request ID y ventana temporal a reinicios
  del pod o respuestas 5xx del ingress cuando el recolector operativo las tenga. Si se requiere
  journal externo, limitarlo al servicio y periodo autorizados, con aprobación operativa.
- Para reinicios de pods, OOM, evicción/DiskPressure o desconexión del host, adjuntar
  diagnósticos mediante un recolector operativo con alcance explícito y mínimo.
  El backend no obtiene por defecto permisos de `kubectl`, SSH, sudo ni Docker socket.
- Distinguir un incidente global coincidente de una causa confirmada del fallo.
  Un API caído debe aparecer como pérdida de conexión en el navegador; la evidencia
  recuperable se enlaza cuando vuelva el servicio.
- Logs de infraestructura compartida requieren filtrado y permisos de operador;
  nunca exponer logs de otras organizaciones desde una ejecución.

**Aceptación:** identificar un lease vencido, fallo de relay, contenedor detenido
u OOM y un reinicio del backend sin confundirlos con errores de tests.

### 5. Retención, seguridad y despliegue

- Redactar antes de persistir o transmitir: credenciales GitHub/modelos/MCP,
  cookies, authorization, URLs autenticadas y variables secretas. Probar secretos
  divididos entre fragmentos, no solo una línea completa.
- Renderizar logs como datos no confiables, sin HTML ejecutable, secuencias de
  terminal peligrosas ni instrucciones que concedan autoridad al agente.
- Primera entrega reutiliza almacenamiento/eventos/artefactos actuales con límites
  visibles. Para volumen sostenido, reutilizar el cliente de object storage existente
  con destino **privado** aprobado y metadatos en SQLite, no blobs ilimitados en DB.
  No usar el bucket público de imágenes para transcripts privados.
- Retención explícita y limpieza de colas/cache/objetos; métricas de espacio,
  bytes y cobertura. No repetir presión de disco por logs y copias ilimitadas.
- Acordar/mostrar el presupuesto de captura por run: «completo» solo si no hubo
  pérdidas. Si se llega al límite, conservar eventos críticos y señalar la pérdida,
  sin presentar la observabilidad degradada como cobertura completa.
- No alterar límites de gasto, políticas, gates, presupuestos ni reintentos del workflow.
  Los reintentos de transporte de la entrega 0 son de llamadas HTTP idempotentes
  dentro del mismo intento; no crean intentos nuevos ni repiten trabajo del agente.
- Si hacen falta tablas/índices o infraestructura nuevos: presentar la migración y
  configuración exactas al usuario; **el usuario aplica la migración de producción**.
  No operar el migration runner.
- Desplegar backend compatible, admin y runner de forma coordinada. Activar primero
  con una ejecución de prueba aislada, no reintentar automáticamente runs reales.

## Implementación de las entregas 1–5

Decisión principal: **ninguna tabla ni índice nuevos.** La tabla de eventos ya tiene
secuencia de ingestión (`seq`, índice `(run_id, seq)`) y un payload JSON libre, así
que no hizo falta que el usuario aplicara una migración de producción.

- **1. Contrato e historial** (`db/workflow_logs.rs`). `POST …/events` acepta campos
  opcionales `source`, `stream`, `severity`, `repository`, `offset`, `origin_ts` y
  `data`; el backend añade `attempt`, `input_revision`, `request_id` y `received_at`
  a partir del lease y la cabecera, nunca del productor. Un reenvío con el mismo
  `offset` se descarta (ventana de los últimos 400 eventos). Presupuestos separados:
  6000 eventos de salida (`log.chunk`, `service.log`, `dependency.log`) y 3000 de
  control por run; al agotarse se deja un marcador `capture.truncated`.
  `GET /v1/factory/workflow-runs/:id/events` pagina por cursor (`after`, `before`,
  `limit`, `step`, `attempt`, `class`) y devuelve la cobertura y el contexto (inicio del
  backend, heartbeat del runner). El detalle del run ya no incluye la salida.
  Los bloqueos llevan `stage` y, para errores con código (p. ej. lockfile), `code`.
- **2. Captura** (`log_stream.py`, `container_entry.py`). Lectura secuencial por
  offsets, sin saltos al tail, cortada en líneas y nunca dentro de un secreto. Hay
  una cola local acotada (512 KiB) mientras el API no responde; lo que se pierde
  se cuenta como `gap_bytes`. Los checks, `npm ci` y el stderr del agente se emiten
  en vivo. Cada proceso deja `command.finished` (exit code, señal, duración, timeout,
  posible OOM con 137, bytes y huecos). Ante una excepción, timeout o cancelación se
  suben las colas de los logs como artefactos `failure-*`, sin ocultar el error
  original. La salida ya no se duplica como tail tras la ejecución.
- **3. Panel** (`StepObservability.tsx`). «Ver ejecución» en cada fase, con pestañas:
  Resumen, Conversación (`TranscriptView` sobre las líneas JSON del stream), Consola
  (filtros, búsqueda, seguir/pausar, cargar anteriores) y Artefactos, más selector de
  intento y descarga redactada. Polling con cursor cada 3 s, solo mientras la fase
  está abierta y en ejecución. Estados distintos para cargando, sin permiso,
  conexión perdida, sin salida, no registrado y fase no ejecutada.
- **4. Backend, runner e infraestructura.** El middleware registra
  `backend.rejected` (ruta, status, código, request ID) en la fase, solo si lo pide
  el runner dueño del run, con un máximo de 100 por run. El runner resume en
  `runner.connectivity` los fallos transitorios de la fase y añade
  `runner.diagnostic` al bloquearse. El recolector de incidentes
  (`infra_incidents.py`) es opcional y está **desactivado** salvo `infra_collector`
  en la configuración privada; reenvía solo una lista cerrada de razones de eventos
  de Kubernetes, marcadas como coincidentes y nunca como causa confirmada. El panel
  indica si el backend se reinició durante el run o si el runner perdió el heartbeat.
- **5. Retención y seguridad.** Redacción de formas de credencial en el runner y otra
  vez en el backend (Bearer, cabeceras, URLs con usuario, tokens GitHub/Anthropic/
  OpenAI/Slack/NexusMind, AWS, claves privadas), con pruebas de secretos partidos
  entre fragmentos. El panel elimina secuencias de terminal y bytes de control, y
  React nunca interpreta HTML. Cada hora se borra la salida de runs terminados con
  más de 14 días (`FACTORY_WORKFLOW_LOG_RETENTION_DAYS`) y queda un marcador
  `capture.pruned`; los resultados, evidencias y artefactos se conservan. El runner
  borra los checkouts de runs terminados o ya no asignados con más de 7 días
  (`checkout_retention_days`) y los de runs bloqueados con más de 14
  (`blocked_checkout_retention_days`); si no puede leer el estado, conserva el
  checkout. La copia de cada fase (`step-*`, con su `node_modules`) se borra al
  terminar la fase, y los tarballs npm sin uso salen de `.node-cache` con la
  misma retención. El checkout de un run comparte por hardlink los objetos Git
  del workspace en vez de copiar el historial.
  «Registro completo» solo aparece si no hubo pérdidas, truncado ni retención.

**Desviaciones:** el reenvío de logs usa memoria acotada, no una cola en disco: si
el runner muere, lo no enviado queda en los artefactos `failure-*` o se pierde, y se
declara como hueco. No se añadió object storage privado: con los límites actuales
basta la base de datos. Las búsquedas filtran el historial cargado en el navegador,
no el servidor.

## Archivos principales previstos

- `apps/backend/src/api/workflow_execution.rs` — ingestión autorizada de eventos.
- `apps/backend/src/api/workflows.rs` — consultas de diagnóstico por run.
- `apps/backend/src/db/workflows.rs` — historial/cursor y contexto por fase.
- `apps/backend/src/db/workflow_execution.rs` — metadatos de logs/artefactos.
- `apps/backend/src/automation/worker.rs`, `factory/otel.rs` — correlación y errores.
- `scripts/factory/execution.py`, `runner.py`, `container_entry.py` — captura y envío;
  en la entrega 0, `RunnerApi.call`, `exception_diagnostic`, `monitor()` y `log_event`.
- `apps/backend/src/api/router.rs` — fallback y rechazos JSON bajo `/v1/factory/runner/*`
  y propagación de `X-Request-Id` (entrega 0).
- `scripts/factory/test_runner.py`, `test_execution.py` — inyección de 502/404/413/429,
  `event_quota` y timeouts de red (entrega 0).
- `scripts/factory/application_environment.py`, `application_node.py`,
  `managed_agent.py`, `model_relay.py` — diagnóstico de servicios/build/relay.
- `apps/admin/src/pages/factory/workflows/ExecutionDetail.tsx`, `types.ts` y
  `WorkflowsPage.tsx` — navegación y detalle por fase.
- `apps/admin/src/pages/factory/runs/Transcript.tsx`, `RunLogs.tsx` — reutilización.
- `apps/admin/src/api/client.ts` — paginación incremental y descarga.
- Tests existentes de backend, runner y panel — ampliar, no crear otro framework.

## Prueba de cierre

Fixture sin gasto de modelos que recorra investigación → tests → implementación
→ verificación → entorno → QA → publicación/revisión simuladas, con dos intentos
y una reparación. Verificar logs de agentes/herramientas/comandos/servicios, un
fallo temprano, un timeout, cancelación y reconexión. Comprobar descarga completa,
redacción, permisos y comportamiento con límites. Ninguna aprobación o evidencia
se hereda de la revisión anterior. Incluir caída del plano de control a mitad de
fase: 502 del borde durante logs y lease, 404 por runner más nuevo que el backend y
`event_quota`, comprobando que cada uno aparece con nombre y no como `HTTPError`.

Después, una prueba real autorizada sobre un repo/commit concreto, anotando qué
fuentes se capturaron y cuáles no. No prometer recuperación de logs históricos
que nunca se guardaron o ya fueron eliminados.

## Registro de esta sesión

- **Goal:** preparar el plan antes de implementar mientras el usuario prueba el
  lockfile que ya llevó a master.
- **Accomplished:** inspección de endpoints, captura del runner, artefactos y visores
  existentes; plan y criterios de aceptación documentados en este archivo.
- **Discoveries:** tails con saltos, historial limitado a 1000 eventos, falta de
  contexto estructurado y cuotas actuales impiden afirmar captura completa.
- **Next steps:** aprobar el plan y ejecutar las entregas 1–5, preservando las
  ejecuciones actuales. El arreglo de hooks automáticos está aparte, aún con cambios
  locales sin validar en `/tmp/nexusmind-reviewer-auto-hooks`; este plan no lo da
  por terminado ni desplegado. El fallback de rama del reviewer sigue pendiente.
- **Relevant files:** este documento es el único archivo creado para el plan;
  ningún cambio de aplicación, configuración o producción en esta sesión de planificación.

## Revisión 2026-11-06: fallos del plano de control

- **Goal:** explicar el bloqueo `adapter_os_error_HTTPError` y ajustar el plan.
- **Accomplished:** trazada la causa en `runner.py`; añadida la entrega 0 y ajustes
  en las entregas 1, 4 y 5, archivos previstos y prueba de cierre.
- **Discoveries:** un `HTTPError` sin cuerpo JSON no viene de un handler del backend
  (Traefik o rechazo de axum); `monitor()` hace fatal un fallo de log; los errores
  transitorios quedan en `hold`. La causa exacta del incidente no se confirmó: hace
  falta el journal del runner y los logs del ingress de esa hora.
- **Causa confirmada después:** el pod del backend fue desalojado por presión de
  disco (ephemeral-storage) a las 22:47:13 UTC; el runner recibió el error del
  ingress a las 22:47:19. El backend no volvió hasta ~00:00 UTC.
- **Next steps:** entrega 0 implementada (ver arriba); después, 1–5 como estaban.
- **Relevant files:** solo este documento; sin cambios de código ni de producción.
