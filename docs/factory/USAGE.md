# Guía de uso del Software Factory

Esta guía cubre el flujo operativo de Factory en producción: configurar agentes, entregar trabajo, revisar ejecuciones, aplicar controles, y evaluar calidad y aprendizaje.

## Acceso

Abre [NexusMind Admin](https://admin.nexusmind.smartcoderlabs.com) con una cuenta que tenga los permisos necesarios. El menú **Factory** contiene estas páginas:

| Página | Ruta | Para qué sirve |
|---|---|---|
| Agents | `/factory` | Crear, validar, habilitar, pausar, ejecutar y archivar agentes |
| Runs | `/factory/runs` | Seguir ejecuciones, eventos, cambios y resultados |
| Findings | `/factory/findings` | Revisar hallazgos producidos por agentes de QA o seguridad |
| Templates | `/factory/templates` | Consultar los tipos de agente y sus capacidades |
| Needs a human | `/factory/inbox` | Revisar runs detenidos, resultados pendientes y tareas sin iniciar |
| Intake | `/factory/intake` | Administrar fuentes de trabajo y revisar lo recibido |
| Autonomy policies | `/factory/policies` | Leer y administrar las reglas de ejecución automática |
| Evaluations | `/factory/evaluations` | Consultar evaluaciones y sus informes |
| Models and roles | `/factory/models` | Revisar proveedores, credenciales y asignación de roles |
| Decision model | `/factory/decision-model` | Revisar decisiones shadow, etiquetas y preparación por clase |
| Factory settings | `/factory/settings` | Comprobar runtimes, pausar toda la flota y administrar retención |

Si un control no aparece, pide a un administrador el permiso correspondiente (`autonomous_agent:read`, `autonomous_agent:create`, `autonomous_agent:run`, `autonomous_agent:enable` o `factory_policy:read/write`).

## Primer agente

1. En **Agents**, selecciona **New agent** y elige una plantilla en **Templates**. Las plantillas habituales son QA, GitHub Issue Resolver, GitHub PR Reviewer, Judge, Security Scan, Security DAST, Lead Generation y AI Content Manager.
2. En **Target**, define el repositorio o aplicación que el agente puede tocar. Usa solo repositorios autorizados, una rama base conocida y una lista acotada de repositorios de contexto.
3. En **Configuration**, completa las opciones de la plantilla: instrucciones, etiquetas de issues, rutas excluidas, comandos de prueba, repositorios para revisión, destinos o límites diarios. No pongas contraseñas ni tokens en las instrucciones o en la configuración libre.
4. Elige el executor conscientemente:

   | Executor | Uso | Requisitos y límites |
   |---|---|---|
   | Claude pure | Ejecutar con Claude Code en el worker | Necesita un login Claude utilizable en el entorno de ejecución |
   | Nexus harness (OpenShell) | Ejecutar dentro del sandbox Nexus OpenShell | Requiere gateway privado conectado por mTLS, imagen y executor provisionados, y un login Claude válido dentro del sandbox |
   | Pi | Usar la cuenta OpenAI/Codex del worker | Executor local. La disponibilidad depende de la plantilla; QA, resolvers y reviewers tienen soporte según [PI_EXECUTOR.md](PI_EXECUTOR.md) |
   | Codex | Usar OpenAI API en sandbox | Requiere su API key en el proxy y solo está habilitado para plantillas compatibles |

5. En **Schedule**, deja `manual` durante la primera validación. Añade horario solo después de probar el alcance, el presupuesto y la entrega de resultados.
6. Guarda el agente, usa **Validate** y revisa todos los errores. Habilítalo solo cuando la revisión sea válida. Para desactivar uno, usa **Disable**; para parar toda la organización, **Pause all agents** en Settings.
7. Inicia una ejecución manual desde la tarjeta del agente. Los jueces y reviewers pueden pedir un PR o issue objetivo al iniciar; verifica que cada objetivo esté dentro de los repositorios configurados.
8. Sigue el run en **Runs**. Inspecciona sus eventos, transcript, diff, verificación y hallazgos antes de aceptar o publicar algo. Cancela una ejecución que exceda el alcance o dejó de ser necesaria.

Los agentes no se deben usar como aprobación automática de sus propios cambios. El GitHub Issue Resolver abre PRs draft; el GitHub PR Reviewer no aprueba ni mezcla. Un humano debe revisar los cambios y la evidencia. Auto-merge requiere la política y verificaciones configuradas para ese repositorio.

## Entregar trabajo

Hay tres entradas habituales:

- **GitHub Issue Resolver:** configura repositorio, rama, etiquetas y rutas excluidas. El agente toma issues elegibles y propone PRs draft con cambios y pruebas.
- **Factory tasks:** crea o entrega una tarea en el proyecto conectado y clasifícala (`docs`, `tests`, `ui`, `backend`, `bugfix`, `migration`, `infra`, `security` o `refactor`). Usa un título que describa el resultado y criterios de aceptación comprobables. Las tareas se revisan en la bandeja antes de ejecutarse según la política de la clase.
- **Intake:** conecta una fuente permitida y revisa cada elemento recibido. Gmail, documentos, transcripciones y contenido subido se tratan como no confiables y requieren `fix: manual`; no inician cambios automáticamente.

Para QA, registra la aplicación objetivo y su alcance permitido; el agente recoge evidencia y crea findings. Para una revisión Judge, configura los repositorios permitidos y entrega explícitamente el PR o issue que debe evaluar. No uses un URL proporcionado al iniciar como autorización para explorar un sitio: la URL y credenciales deben provenir del target registrado.

## Runs y resultados

En **Runs** revisa:

- estado, duración, presupuesto y executor;
- eventos de inicio, validación, sandbox, verificación y cierre;
- transcript y artefactos adjuntos, si existen;
- archivos cambiados y diff antes de aceptar un PR;
- findings creados y su evidencia.

Un estado `blocked_runtime` indica que el executor, gateway, imagen o login no está disponible. `blocked_policy` indica que una regla de seguridad detuvo el run. `cancelled` significa que se interrumpió antes de completar; `failed` indica un error no recuperable. No trates un run bloqueado o cancelado como un resultado aceptado.

Si el run queda en **Needs a human**, resuelve la acción solicitada desde **Inbox**. Si la ejecución no está usando el repositorio o alcance esperado, cancélala y corrige la definición antes de intentarlo otra vez.

## Modelos, gateway y credenciales

En **Models and roles** y **Factory settings**, comprueba el estado del runtime antes de activar agentes. El indicador `ready` confirma preflight, no garantiza que el proveedor vaya a aceptar su token; el primer run sigue siendo la prueba de autenticación real.

Para Nexus OpenShell deben estar listos el gateway privado, la autenticación mTLS, la imagen y el executor. La sesión Claude se provisiona en el worker con un flujo operativo temporal; nunca pegues el token en un issue, PR, prompt o archivo del repositorio. Si aparece `claude_auth_required` o `OAuth access token has been revoked`, renueva el login en el perfil local correcto y vuelve a provisionarlo por el canal restringido. No ejecutes una evaluación de Claude para probar el login.

Para Pi, Settings ofrece un chequeo de OAuth que no envía un prompt. El executor de producción usa la cuenta ChatGPT/Codex local del worker. La evaluación F4 descrita abajo usa el modelo `openai-codex/gpt-6-luna` con thinking `medium`; ese modelo/configuración de evaluación es independiente del selector normal de Pi.

## Policies y Decision Model

Usa **Autonomy policies** para revisar qué clases pueden comenzar una acción automáticamente, cuáles requieren aprobación y cuáles permanecen manuales. En **Decision model**, revisa las decisiones shadow, sus etiquetas humanas y los resultados asentados.

La regla OD-5 exige al menos 50 decisiones `allow` asentadas por clase y una tasa de false-low de como máximo 2% antes de activar routing automático. Si una clase no alcanza su muestra, mantenla en revisión humana. No inventes etiquetas: una decisión se marca solo después de que un revisor haya considerado su evidencia y resultado.

## Evaluaciones

### F1: replay determinista

El golden replay F1 ejecuta verificación reproducible y no llama a un modelo. Se lanza desde GitHub Actions → **Factory F1 golden replay**. `start` recibe un manifest JSONL sanitizado con `id`, `repository`, `merge_sha` y `changed_files`; `status` recibe el ID de Actions de un replay previo. El workflow permite entre 1 y 50 tareas de los repositorios allowlisted. No agregues prompts, descripciones con secretos ni datos personales al manifest.

### F4: comparación de specialists

F4 compara baseline y specialist sobre el mismo commit de una tarea congelada. La corrida modelada se hace por Pi/OpenAI, no por Claude. El exit medido requiere que docs, tests y UI superen el baseline en sets congelados. No edites una entrada ya evaluada: para probar una hipótesis nueva, crea otro set independiente.

El workflow **Queue factory specialist eval rerun** tiene estos modos:

```bash
# Inspección de progreso; no invoca modelos.
gh workflow run factory-eval-rerun.yml -R smart-coder-labs/nexus-mind \
  --ref main -f action=status -f sets=docs

# Crea un rerun nuevo del set docs y despierta el runner Pi/OpenAI.
gh workflow run factory-eval-rerun.yml -R smart-coder-labs/nexus-mind \
  --ref main -f action=queue -f sets=docs
```

`sets` puede ser `docs`, `tests`, `ui` o `all`. `status` no consume cuota de modelo. `queue` y `resume` sí pueden iniciar llamadas a Pi/OpenAI y consumir la cuota de esa cuenta. `resume` continúa todos los sets pendientes que ya están en la cola; no es un filtro por `sets`. Revisa el status antes de reanudar.

F1 y F4 tienen costes distintos: F1 no llama a un modelo, pero consume CPU y tiempo de sandbox; F4 consume la cuota OpenAI/Codex del worker. Mantén las evaluaciones de Claude apagadas.

## Inbox, feedback y lookback (F5)

En **Inbox**, revisa resultados de runs que esperan una persona y agrega el juicio `accepted` o `rejected` solo después de inspeccionar el diff, la verificación y el resultado. La evaluación humana se guarda como evidencia; no reescribe la política del run.

**Lookback** agrupa recurrencias con memorias `bugfix` y entrega recomendaciones. Es consultivo: no corrige código, no crea PRs y no cambia políticas por sí mismo. Revisa cada recomendación y la memoria que la respalda antes de decidir una acción.

## Pausa, retención y recuperación

- Para una ejecución individual, usa **Cancel** en Runs y confirma que el estado final sea `cancelled`.
- Para detener todos los agentes de la organización, usa **Pause all agents** en Settings. Los runs en curso se cancelan y no empiezan runs nuevos hasta reanudar.
- Ajusta retención en Settings dentro del rango indicado por la interfaz. El historial eliminado no se puede usar para QA posterior.
- Ante fallos, comprueba en este orden: estado del agente, validación de definición, permiso del usuario, runtime/executor, gateway, autenticación del proveedor, límites/presupuesto, y por último transcript/eventos del run.

## Referencias operativas

- [Plan y estado por fase](PLAN.md)
- [Contrato y controles de evaluación specialist F4](specialist-eval.md)
- [Executor Pi y login ChatGPT del worker](PI_EXECUTOR.md)
- [Fixtures y screenshots de UI QA](ui-fixtures.md)
- [Lookback y recomendaciones F5](lookback.md)
- [Issue OpenShell #339](https://github.com/smart-coder-labs/nexus-mind/issues/339)
