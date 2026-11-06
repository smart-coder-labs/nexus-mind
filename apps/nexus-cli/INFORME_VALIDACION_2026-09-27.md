# Informe de validación del harness — 27 de septiembre de 2026

## Resultado

**El HTTP 401 quedó resuelto.** Codex terminó un turno dentro de OpenShell sin API key de OpenAI. Después, el flujo Codex + NexusMind + verificación + JEV también llegó hasta una decisión de JEV. NexusMind respondió HTTP 200; ese proyecto no devolvió memorias para el caso de prueba. JEV seleccionó `human_review` porque este repositorio ya tenía el directorio `apps/nexus-cli/` sin seguimiento antes del turno, no por un fallo del runtime. La comparación de eficiencia sigue siendo exploratoria: hay una sola muestra por condición y los binarios Codex del host y sandbox tienen versiones distintas.

## Caso y mediciones

Prompt idéntico: «Sin editar archivos, lee apps/nexus-cli/Cargo.toml e indica el nombre exacto del paquete Rust en una sola línea». El resultado esperado es `nexus`. Codex puro usó la configuración del host (`gpt-5.6-terra`, CLI 0.157.1); la prueba Nexus forzó el mismo modelo dentro de OpenShell (CLI 0.154.0).

| Condición | Resultado | Codex total | Entrada Codex | Caché Codex | Salida Codex | JEV entrada/salida |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Codex puro, `codex exec --json --sandbox read-only --ephemeral` | Correcto: `nexus` | 40 932 | 40 800 | 31 232 | 132 | 0/0 |
| Nexus + Codex + JEV + NexusMind real, OpenShell obligatorio | Correcto: `nexus`; verificaciones pasaron; JEV pidió revisión humana por cambios preexistentes | 40 033 | No persistido por separado | No persistido | No persistido | 663/78 |
| JEV aislado, estado sintético de prueba (no comparable con el prompt) | Respuesta válida `run_tests` | 0 | 0 | 0 | 0 | 626/78 |

El uso de Codex puro procede del evento `turn.completed.usage` de su traza JSONL; el harness conserva el total de tokens Codex en la sesión, pero todavía no conserva el desglose entrada/caché/salida. OpenAI documenta esos eventos para evaluaciones: <https://developers.openai.com/blog/eval-skills>. Los tokens JEV proceden del campo `usage` de su respuesta. La muestra Nexus suma 40 774 tokens Codex+JEV frente a 40 932 de Codex puro, pero **no permite afirmar ahorro**: hay variación entre ejecuciones, versiones diferentes y trabajo adicional de verificación. La prueba aislada de JEV evalúa otro estado y no se suma.

## Validaciones realizadas

- 28 pruebas automáticas pasaron, incluida una regresión que exige copiar los marcadores específicos de la sesión OpenShell al `auth.json`; una prueba de JEV en vivo está marcada `ignored` para que no consuma cuota ni requiera secretos en la suite ordinaria.
- `cargo clippy --all-targets -- -D warnings` terminó sin avisos.
- OpenShell 0.0.116 quedó conectado y autenticado después de iniciar Docker Desktop. Sin Docker activo, el gateway devuelve `Connection refused` y no se inicia ningún agente local alternativo.
- La prueba JEV en vivo confirmó credencial, esquema y lectura de tokens, sin guardar la clave en el repositorio.
- NexusMind respondió HTTP 200 a `/v1/memory/search`; el proyecto usado en esa consulta no tenía memorias coincidentes. Con `NEXUSMIND_REQUIRED=1`, el harness pasó la recuperación sin error. La respuesta temporal se eliminó.
- El perfil MCP `essential` se aplica explícitamente a los procesos Claude Code y Codex dentro de OpenShell. La recuperación previa por REST no usa perfiles MCP.
- La imagen Rust de OpenShell usa Codex CLI 0.154.0. La base traía 0.117.0; 0.157.1 fue probado, pero chocó con `selected workspace missing from routing discovery`, una regresión también reportada por otros usuarios de Codex 0.156+. Se sincroniza el proveedor de sesión antes de cada turno y se crea un archivo temporal con permisos 0600 dentro del sandbox; se elimina tras el turno.
- Causa del 401 original: el archivo `auth.json` del sandbox tenía cadenas genéricas `openshell:resolve:env:...`, pero OpenShell entrega marcadores opacos **específicos de la sesión**, más largos. El proxy registraba `credential_unavailable`. El bootstrap ahora copia esos marcadores del entorno para `access_token`, `refresh_token` y `account_id`, y mantiene un `id_token` sintético con forma JWT para el procesamiento local de Codex. Con esta combinación, Codex completó un turno y no hubo 401. El flujo sigue usando la política inspeccionada de OpenShell; no se añadió una API key de OpenAI.

## Pendientes para poder comparar

1. Repetir la búsqueda NexusMind sobre un proyecto con memorias relevantes para medir el efecto del contexto. La conexión y la credencial ya se validaron; no se persistió la clave.
2. Ejecutar la misma tarea sobre instantáneas idénticas con tres condiciones: Codex puro; Nexus + Codex + JEV sin NexusMind; Nexus + Codex + JEV con NexusMind. Registrar `turn.completed.usage`, `JEV usage`, salida correcta, comandos, duración, fallos y contexto recuperado. Repetir al menos tres veces por condición y separar tokens de entrada en caché.
3. Añadir persistencia del desglose de tokens Codex en las sesiones Nexus y revisar la versión fijada del CLI cuando la regresión de routing se resuelva.

No se almacenaron las claves JEV o NexusMind proporcionadas ni se incluyeron en este informe.
