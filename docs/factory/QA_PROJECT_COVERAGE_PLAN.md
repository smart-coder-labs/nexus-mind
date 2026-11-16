# QA por proyecto e histórico de cobertura

Estado: plan revisado contra el checkout local; sin implementación ni cambios en producción.

## Objetivo

Configurar QA con proyecto + aplicación/entorno, no con un repo único. Elegir en cada tanda qué probar usando un histórico persistente por agente, y publicar bugs en el repo donde se corrigen.

App UI y Backoffice pueden pertenecer al mismo proyecto sin compartir automáticamente cobertura: son aplicaciones distintas. El histórico debe aislar organización, agente, aplicación y entorno; un cambio de proyecto/target no hereda cobertura incompatible.

## Hallazgos del código actual

- `apps/admin/src/pages/AutonomousAgentWizard.tsx`: QA usa por defecto un target `web_application`. El repo de checkout es opcional en el formulario; no hay selector de proyecto para este flujo. El selector genérico ofrece `project`, pero no constituye resolución multi-repo para QA.
- `apps/backend/src/api/autonomous_agents.rs`: el esquema de la plantilla QA sigue exponiendo `repository` y un workflow con checkout.
- `apps/backend/src/db/queries.rs`, validación de QA: activar salida `github_issue` exige `config.repository` (`github_configuration_required`).
- `apps/backend/src/automation/worker.rs`, `deliver_findings` y `retry_one_delivery`: la publicación/reintento de issues toma un repo de configuración, no un destino razonado por hallazgo. También hay publicación manual: hay que mantener el mismo criterio en todas las rutas.
- `apps/backend/src/automation/worker.rs`, `fixed_prompt`: el contrato QA sólo pide `summary` y `findings`. No registra escenarios correctos ni pendientes. El prompt limita la observación a UI autenticada; actualmente un fallo de login se devuelve como finding.
- `apps/backend/src/db/queries.rs`: ya existen runs por agente, eventos, findings y entregas; no hay que construir otro sistema de ejecuciones.
- `apps/admin/src/pages/factory/runs/RunDetail.tsx`: ya muestra resultados QA y findings; se debe ampliar esta vista.
- `apps/backend/src/db/queries.rs`, `link_code_project_to_project`: rechaza explícitamente vincular un segundo `code_project` al mismo proyecto, con un test que exige esa restricción. La migración que añade `code_projects.project_id` crea un índice normal, no único. Revisar consumidores que asuman singular antes de cambiarlo.
- La limpieza de runs antiguos puede borrar ejecuciones sin findings: las pruebas correctas no pueden ser la única copia de cobertura dentro de logs/eventos sujetos a retención.

El checkout tiene numerosos cambios previos, incluidos archivos de este plan. No sobrescribirlos ni tratarlos como parte de esta entrega.

## Entrega 1 — Alcance proyecto y repos asociados

1. Usar el proyecto canónico (`project_id`) como alcance de QA, conservando URL, credenciales y entorno en el target web existente.
2. Permitir varios repos por proyecto reutilizando `code_projects.project_id`; no crear otra tabla de vínculos si no hace falta.
3. Revisar consultas/API/interfaz que interpreten la relación como uno-a-uno y actualizar el test que hoy rechaza el segundo repo.
4. Distinguir repos indexados de repos accesibles para lectura/publicación. No asumir que todos tienen URL GitHub o credenciales válidas.
5. Resolver la lista autorizada en servidor por organización/proyecto y registrar su snapshot por ejecución. Ni el prompt ni la salida del modelo pueden ampliar esa lista.
6. En el wizard QA sustituir el repo obligatorio para publicación por un selector de proyecto y mostrar sus repos asociados. Mantener el target web como alcance de navegación.
7. Conservar compatibilidad de definiciones existentes: no adivinar proyecto por nombre ni reasignar agentes automáticamente. Pedir elección explícita al editar/migrar cada QA.
8. Mantener `allowlisted_command`: necesita un repo de ejecución concreto, elegible entre los del proyecto, sin convertirlo en destino obligatorio de todos los bugs. La rotación exploratoria no modifica comandos fijados por el operador.

Archivos principales:
- `apps/admin/src/pages/AutonomousAgentWizard.tsx`
- `apps/admin/src/types.ts`, `apps/admin/src/api/client.ts`
- `apps/backend/src/api/autonomous_agents.rs`
- `apps/backend/src/db/queries.rs`
- `apps/backend/src/models/types.rs`

## Entrega 2 — Registro durable de pruebas

Añadir almacenamiento QA específico y pequeño, enlazado con las ejecuciones existentes. Los logs no serán la fuente de verdad de cobertura.

Datos mínimos por escenario ejecutado:
- Organización, agente, proyecto, aplicación/entorno y ejecución/intento de origen.
- Clave estable de módulo/escenario, descripción y variante/rol relevante.
- Resultado `passed`, `failed` o `blocked`; evidencia/referencia de pasos y hallazgos relacionados.
- Momento de ejecución. Versión desplegada sólo cuando sea observable; no usar el SHA de un checkout como prueba de qué versión corre en la URL.

Guardar también el plan de la tanda y pendientes. `planned`/`not_run` no equivale a ejecutado. Un fallo comprobado sí es cobertura ejecutada, pero nunca un caso superado; un bloqueo no completa cobertura.

Ampliar el contrato JSON de QA y validarlo en servidor. Registrar checkpoints estructurados durante la ejecución para no perder todo si se agota el presupuesto o falla el resultado final; aprovechar eventos existentes como transporte, persistiendo sólo checkpoints válidos e idempotentes. Guardar estos resultados antes de intentar entrega externa: GitHub caído no borra pruebas.

La cobertura debe sobrevivir al archivado/limpieza normal de logs y runs. Definir retención y referencias para que la limpieza actual no la elimine ni quede bloqueada por nuevas claves foráneas. Conservar resumen durable de cobertura aunque se poden evidencias/transcripciones antiguas.

Archivos principales:
- `apps/backend/src/db/migrations.rs` y módulo de consultas QA dedicado
- `apps/backend/src/db/mod.rs`, `apps/backend/src/models/types.rs`
- `apps/backend/src/automation/worker.rs` (contrato, checkpoints, evaluación y persistencia)
- `apps/backend/src/api/autonomous_agents.rs` (consulta paginada por agente)

## Entrega 3 — Selección de la siguiente tanda

Mantener inventario incremental de módulos/escenarios descubiertos en la aplicación y estado de cobertura. Permitir alcance inicial del operador; no exigir catalogar manualmente toda la aplicación. Descubrir áreas nuevas no cuenta como haberlas probado.

Antes de ejecutar, cargar resumen durable y detalle reciente acotado, no todas las transcripciones. Prioridad inicial simple:
1. Áreas/escenarios todavía no ejecutados.
2. Pendientes o bloqueados cuyo impedimento haya cambiado, sin insistir eternamente en el mismo bloqueo.
3. Variantes y flujos entre módulos, con casos límite y roles autorizados.
4. Regresiones de fixes confirmados y escenarios antiguos/de mayor riesgo.

No congelar esta secuencia: reservar controles críticos cuando corresponda, aunque falten áreas nuevas. Si no se conoce la versión desplegada o el estado de un fix, no afirmar que fue revalidado.

El agente propone un plan acotado con motivo de selección y usa los presupuestos existentes. No se añade un motor combinatorio ni otra cola/scheduler. No afirmar «cobertura total»: sólo «áreas conocidas recorridas». Normalizar claves de escenarios para que cambios de redacción no parezcan cobertura nueva.

Archivos principales:
- Consultas QA de la entrega 2
- `apps/backend/src/automation/worker.rs` (contexto y plan)

## Entrega 4 — Clasificación y publicación por hallazgo

Separar dos fases:
1. Observación: reproducir el fallo en UI autenticada, recoger evidencia y resultado esperado/obtenido.
2. Clasificación: inspeccionar código/contratos de los repos autorizados para determinar dónde se corrige ese fallo ya observado.

Ajustar el prompt para permitir diagnóstico de código sin permitir inventar bugs a partir de código ni saltarse el login. No habilitar shell arbitrario para obtener repos: lectura con herramientas/control de alcance existentes o adaptación mínima del contexto de lectura. No clonar todos los repos para cada tanda.

Por hallazgo guardar destino propuesto, evidencia de responsabilidad y estado de clasificación. El servidor verifica pertenencia al proyecto, permisos y formato antes de publicar. Si no hay evidencia suficiente: `needs_triage`, sin repo por defecto. Un bloqueo de acceso/infraestructura se registra como bloqueo, no genera automáticamente una issue de producto.

Buscar duplicados entre repos candidatos usando fingerprint y evidencia. Si la corrección requiere varios repos, crear issues relacionadas por responsabilidad, conservando un único hallazgo de origen. Fijar el destino en la entrega persistida para que reintentos, publicación manual y cambios posteriores de configuración no lo redirijan. Mantener creación idempotente y manejo del caso «issue creada, respuesta perdida».

Archivos principales:
- `apps/backend/src/automation/worker.rs` (`deliver_findings`, publicación manual y `retry_one_delivery`)
- `apps/backend/src/automation/connectors.rs` (reutilizar búsqueda/markers/creación existentes)
- Consultas/modelos de findings y deliveries; extender JSON existente si basta

## Entrega 5 — Interfaz e histórico

Ampliar la navegación/vistas existentes, no crear un panel paralelo:
- Por agente: tandas con módulos, escenarios, resultados, pendientes y motivo del plan.
- En `RunDetail`: plan versus ejecución real, bloqueos, evidencias, hallazgos, repo destino y enlace a issue.
- Resumen de cobertura por aplicación/entorno: nunca probado, ejecutado, bloqueado y última prueba.
- Hallazgos pendientes de clasificación con opción de destino manual autorizado.

Archivos principales:
- `apps/admin/src/pages/factory/runs/RunsPage.tsx`
- `apps/admin/src/pages/factory/runs/RunDetail.tsx`
- `apps/admin/src/pages/factory/findings/` y vista de agente existente
- `apps/admin/src/types.ts`, `apps/admin/src/api/client.ts`

## Pruebas de aceptación

- Proyecto con dos repos aceptado; otro proyecto/organización queda fuera del alcance.
- Crear/editar QA por proyecto y URL sin repo único; compatibilidad con QA antiguo y comandos fijados.
- Tanda 1 registra productos/clientes; tanda 2 prioriza ventas pendiente. Casos passed también influyen.
- Escenario bloqueado no se marca cubierto, no se reintenta en bucle y no crea bug de producto por defecto.
- Con áreas conocidas recorridas se proponen flujos combinados, límites y regresiones dentro del presupuesto.
- Checkpoint persistido sobrevive a timeout, reintento y limpieza de logs sin duplicar pruebas.
- Bug visible en UI pero localizado en backend se publica sólo en ese repo.
- Clasificación ambigua queda pendiente; bug duplicado no crea nueva issue; fix multi-repo crea vínculos.
- Reintento de entrega conserva repo/fingerprint originales aunque se edite el agente.
- Cambiar target/entorno no mezcla cobertura; listados/histórico no filtran secretos ni datos de otra organización.
- Wizard e histórico cubiertos por tests existentes ampliados, más tests backend de selección/persistencia/routing. Usar mocks para GitHub; ninguna prueba paga/producción automáticamente.

## Orden y operación

Implementar 1 → 2 → 3 → 4 → 5, sin habilitar agentes parcialmente migrados. Validar localmente con fixtures antes de ejecutar QA real.

Este plan requiere una migración para cobertura durable; el número/función se fija al implementar según la versión vigente, no se reserva ahora. La migración de producción la aplica el usuario; no ejecutar migration runner, desplegar ni reactivar agentes sin autorización.

## Resumen de sesión

- Goal: concretar el plan de QA multi-repo con histórico por agente.
- Accomplished: revisión de wizard, esquema/validación QA, prompt, publicación/reintentos, relación proyecto-repo, almacenamiento y vistas de resultados; sin cambios funcionales.
- Discoveries: vínculo uno-a-uno impuesto por función/test; repo exigido para publicar; contrato sin pruebas correctas; retención de runs insuficiente.
- Next Steps: aprobar alcance y comenzar entrega 1, preservando cambios locales existentes.
- Relevant Files: rutas indicadas por entrega; este documento es el único archivo añadido en esta sesión.

## Estado de implementación revisado tras Sol (sin despliegue)

Estado: **parcial; no apto para merge/activación**. Se corrigieron estas regresiones específicas: el resultado final QA exige escenarios ejecutados, hallazgos QA requieren al menos un escenario fallido con evidencia y los hallazgos no pueden acompañar una tanda exclusivamente bloqueada; cualquier QA con proyecto se conserva en triage independientemente de la lista de outputs. Hay pruebas unitarias añadidas para contrato vacío/bloqueado y para outputs NexusMind.

Siguen sin implementarse y no deben considerarse resueltos: redacción de credenciales conocidas dentro de escenarios/checkpoints/API; snapshot de repositorios autorizados por ejecución y clasificación/routing multi-repo; checkpoints estructurados en streaming con identidad de intento; inventario/plan durable y selección verificable; historial paginado del agente accesible tras podar runs; wizard y navegación de esa historia. No se declara corregida la autorización manual hasta que quede ligada al proyecto y destino original verificados en servidor. No ejecutar ni activar QA de proyecto mientras no exista snapshot autorizado.

## Nota de implementación local (sin despliegue)

- La migración local v92 añade `destination_repository` a las entregas; los nuevos reintentos usan el destino fijado y, para entregas antiguas sin destino, la configuración de la revisión del run de origen, no la revisión actual del agente.
- QA con `project_id` y salida `github_issue` se guarda en NexusMind, pero no crea issues automáticamente: la evidencia persistida incluye `routing.status = needs_triage` y el evento `finding.needs_triage`. No existe todavía un snapshot verificado de repos vinculados con permisos de lectura/escritura por proyecto. No inferir destino del prompt, del modelo ni de repos indexados. Entregas antiguas del proyecto sin destino fijado se detienen en `dead_letter` con `last_error_code = needs_triage`; no se reintentan contra una configuración actualizada.
- QA heredado sin `project_id` conserva el repositorio explícito de su configuración; el destino se valida y fija en la entrega antes de publicar. Este modo no equivale a autorización multi-repo por proyecto.
- La rotación usa el historial durable conocido para priorizar fallos y bloqueos; el inventario de escenarios sigue siendo exploratorio, no exhaustivo.

Herramientas NexusMind de consulta/guardado de memoria no expuestas en esta sesión; este documento deja el registro local, no implica memoria remota guardada.

## Decisión local de autoridad para QA por proyecto

- `code_projects.project_id` y la URL GitHub normalizada sólo identifican candidatos; no conceden acceso de lectura ni publicación.
- La autorización de QA usa grants explícitos por repositorio, default-deny, y un snapshot inmutable por run ligado a organización, proyecto, revisión, repositorio, permisos y la identidad no secreta de la credencial.
- Antes de checkout, publicación manual/automática o retry se comprueba el snapshot original junto con grants, membresía/proyecto vigentes y capacidad/identidad actual de credencial; revocaciones invalidan la operación. Tokens no se guardan en el snapshot.
- Si autoridad de gestión existente no identifica inequívocamente un actor apto para conceder acceso, sólo un administrador de organización puede crear el grant. Capacidad desconocida falla cerrada para esa operación.

## Estado local tras H4 checkpoint durable

H4 está **parcial**: existe almacenamiento idempotente validado por `(org, run, attempt, scenario)`, con redacción antes del INSERT y sin FK al run para sobrevivir a su limpieza. No está conectado a las corrientes local/sandbox, no se emite ni consume aún el protocolo de checkpoint y no conserva automáticamente checkpoints ante timeout/fallo. Por tanto, no considerar H4 completado ni activar QA de proyecto. La migración local es v97; no ejecutada en producción.

## Estado local tras revisión Sol 8

La publicación de findings legacy sin proyecto conserva su repo de origen. Su POST inicial ahora usa `create_github_issue_once`: no reintenta sin labels ante error de POST, porque una respuesta perdida puede ocultar una creación exitosa; la siguiente reejecución hace primero la búsqueda existente por marker. Esta corrección no resuelve paginación/búsqueda exhaustiva ni garantiza deduplicación fuera del conjunto consultado.

H2/H3 siguen **incompletos**: no se crean ni consumen snapshots autorizados por run, no hay verificación positiva de permisos GitHub por operación, no se revalida membresía/grant/capacidad y el QA por proyecto permanece en triage/fail-closed. Falta diagnóstico con scope autorizado, destinos por responsabilidad, selección manual/wizard multi-repo y casos positivos UI→backend/multi-repo. No activar QA por proyecto. Cargo/Rust ausentes en esta máquina limitan la validación, no justifican posponer la implementación.

## Estado local H7/H8 (sin despliegue)

Se añadió localmente migración v98 para inventario incremental por módulo/escenario y planes por tanda separados de resultados, con razón/estado pendiente/no ejecutado/ejecutado. La selección carga ese inventario al contexto real de la siguiente ejecución: conocidos nunca probados primero, luego fallos y pasados rotados; escenarios bloqueados sólo vuelven a entrar cuando cambia su fingerprint. El cierre de resultado actualiza los estados planificados. Resultados `passed` permanecen como cobertura ejecutada.

La API de historia QA es independiente de que sobreviva el run, usa cursor estable `(tested_at,id)`, filtros de proyecto/aplicación/entorno y permiso explícito del proyecto resuelto por nombre canónico. La UI de Runs ofrece navegación histórica por agente, resumen paginado y estados planificados frente a ejecutados con estados de error/loading. Se añadieron tests Rust fuente para progresión productos/clientes→ventas→combinaciones, bloqueos sin cambio y 125 resultados paginados. **No se considera H7/H8 totalmente verificado:** Rust/Cargo no están disponibles, checkpoints de inventario durante streaming/timeout todavía no están conectados y no se ejercitaron autorización/API contra fixtures reales ni limpieza de runs en la aplicación. Migración v98 sólo local; no aplicar en producción ni activar QA.

## Estado local tras follow-up Sol 1

Se corrigieron dos errores sintácticos (`;`) en la persistencia QA, el test de listado legacy ahora consulta explícitamente el scope de proyecto que exige la API y la aserción de versión usa v98. La paginación genera el cursor desde la última fila entregada en vez de la fila extra descartada; el test existente de 125 filas verifica que todas se recuperan. Los flujos automático y retry de issues ahora usan `create_github_issue_once`, eliminando el segundo POST inmediato ante error; el lookup por marker sigue precediendo a cada creación.

Estos arreglos son parciales y **no demuestran aceptación integral**: no hay `cargo`/`rustc` disponible para compilar/ejecutar tests. Siguen abiertos el flujo positivo y snapshot/grants/capacidades multi-repo, redacción segura/visibilidad completa de nuevos eventos/findings, checkpoints QA streaming por intento, persistencia del plan efectivo con actualización aislada por intento, y cobertura UI/autorización/dedup exhaustiva. Mantener QA de proyecto inactivo; ninguna migración productiva, despliegue o activación ejecutada.

## Estado local tras follow-up Sol 2

Se corrigió el parseo del cursor QA para propagar errores fuera de la closure, se alineó la expectativa del test del prompt y las aserciones de ausencia/rollback ahora consultan explícitamente el proyecto sembrado. La persistencia final recibe el `attempt_id`; el plan devuelto se valida y reemplaza los candidatos pendientes de ese intento, y los cierres `passed`/`failed`/`blocked`/`not_run` quedan limitados al mismo intento. Se añadieron regresiones fuente para plan efectivo y aislamiento entre intentos. No se cambió el esquema ni se ejecutó migración.

Esto reduce el defecto de cierre cruzado y descarte del plan efectivo, pero no completa H7: no hay checkpoints de plan durante streaming/timeout, ni prueba de progresión integrada con ejecución real; los outcomes finales todavía conservan la identidad única por run/escenario y pueden sobrescribir hechos entre intentos/orígenes (H4). H1/H2/H3/H4/H8 y los flujos positivos autorizados siguen abiertos. Rust/Cargo no está disponible, por lo que estos cambios no están compilados ni ejecutados; mantener QA de proyecto inactivo y no activar/desplegar.
