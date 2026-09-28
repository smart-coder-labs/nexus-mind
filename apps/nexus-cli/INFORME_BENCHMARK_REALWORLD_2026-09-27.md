# Benchmark realista y auditable: Codex solo frente a Nexus

Fecha local: 27 de septiembre de 2026. Se evaluaron una página web, un módulo de aprobaciones empresarial y un microservicio HTTP. Hubo **dos corridas válidas por brazo y categoría** (12 entregas) y dos intentos del microservicio censurados por agotamiento de cuota, posteriormente reemplazados por el par válido de iteración 3. No se atribuyen resultados de los intentos censurados al código de los agentes.

## Resumen ejecutivo

| Categoría | Calidad funcional Codex solo | Calidad funcional Nexus | Promedio de tiempo Codex / Nexus | Promedio de tokens totales Codex / Nexus | Promedio de tokens no cacheados Codex / Nexus |
| --- | --- | --- | ---: | ---: | ---: |
| Página web | 22/22 pruebas | 21/22 pruebas | 148,1 / 117,4 s | 226 917 / 93 705 | 34 917 / 17 801 |
| Módulo empresarial | 20/20 | 20/20 | 107,7 / 104,3 s | 238 438 / 114 240 | 26 598 / 16 192 |
| Microservicio | 18/18 | 18/18 | 111,9 / 124,3 s | 204 625 / 138 747 | 24 657 / 20 475 |
| **Total de 6 entregas** | **60/60** | **59/60** | **735,1 / 691,9 s** | **1 339 959 / 693 381** | **172 343 / 108 933** |

En estas muestras, Nexus consumió 48,3 % menos tokens totales y 36,8 % menos tokens no cacheados, y terminó 5,9 % antes en tiempo acumulado. **Codex solo entregó mejor completitud funcional**: Nexus omitió una modalidad de ordenación en una de las dos páginas web. Además, las seis sesiones Nexus válidas quedaron en `human_review` por el umbral de confianza de JEV, mientras las seis de Codex solo cerraron normalmente. Por tanto, Nexus fue más eficiente en tokens en esta batería, pero **no tuvo mejor calidad de entrega autónoma**. Son seis pares, no una estimación estadística general.

## Qué se pidió en cada iteración

Cada fila de la tabla de corridas enlaza una cronología con el prompt exacto y los pasos realizados. La iteración 1 y la 2 de cada categoría recibieron el **mismo** pedido; la iteración 3 del microservicio repitió exactamente el pedido de la iteración 2 después de que la cuota se restableció.

| ID | Pedido literal |
| --- | --- |
| WEB | “Implementa una página de catálogo responsive en español en index.html, styles.css y src/app.js. Debe tener búsqueda sin distinguir acentos, filtro de categoría, orden por nombre/precio ascendente/descendente, estado vacío, controles etiquetados, HTML seguro frente a XSS y un layout adaptable. Mantén las exportaciones y pasa node --test; no modifiques las pruebas.” |
| CORP | “Implementa el módulo ApprovalService para solicitudes de compra: validación de importe finito positivo e ID único, aprobación/rechazo solo por manager o admin ajeno al solicitante, transición solo desde pending, auditoría con actor/acción/fecha, reintentos idempotentes sin eventos duplicados y lecturas que no permitan mutar el estado interno. Mantén la API y pasa node --test; no modifiques las pruebas.” |
| SVC | “Implementa un microservicio HTTP de pedidos sin dependencias externas: GET /health, POST /orders y GET /orders/:id, JSON consistente, validación estricta de sku y cantidad entera positiva, idempotencia por cabecera con 409 si la misma clave trae otro payload, 415 para content-type inválido, 413 para cuerpos mayores de 64 KiB y 404 JSON para rutas desconocidas. Mantén createServer y pasa node --test; no modifiques las pruebas.” |

Los prompts están asimismo fijados en [`run-realworld.mjs`](benchmarks/run-realworld.mjs). Las plantillas y pruebas visibles son [`web`](benchmarks/fixtures/web/test/web.test.js), [`corporate`](benchmarks/fixtures/corporate/test/approvals.test.js) y [`service`](benchmarks/fixtures/service/test/server.test.js). Las pruebas reservadas son [`web`](benchmarks/heldout/web.test.mjs), [`corporate`](benchmarks/heldout/corporate.test.mjs) y [`service`](benchmarks/heldout/service.test.mjs).

## Protocolo de medición

- Mismo modelo `gpt-5.6-terra`, misma versión de Codex CLI **0.154.0**, mismo prompt y árbol Git inicial por categoría. Se verificaron los hashes de árbol inicial: web `ea67e2d91afa2163e6c495c3ae435ce074de119b`, corporate `184ee0e6f99f1c85dd9066470caad9d9e984c580`, service `86c1eaecdca2666c92c4636f85a381e4b232ef9f`.
- Codex solo usó el CLI autenticado del host con `--ignore-user-config`, `--json` y sandbox `workspace-write`. Nexus usó Codex dentro de OpenShell, perfil NexusMind `essential`, `NEXUSMIND_REQUIRED=1`, verificación `node --test`, aceptación `node --test` y JEV para decidir el cierre. Ninguna clave está en el repositorio ni en las evidencias.
- Se registraron tiempo monotónico de pared, salida del proceso, diffs, pruebas visibles, pruebas reservadas, archivos modificados, tokens de entrada/cache/salida de Codex y tokens JEV. La fórmula “no cacheados” es `input_tokens - cached_input_tokens + output_tokens + tokens_JEV`; **no** es una factura ni una estimación de coste monetario. Los contadores de Nexus obtenidos de los JSONL internos coincidieron exactamente con `state.tokens_used` en las seis ejecuciones Nexus válidas.
- Las dos repeticiones alternaron el orden de brazos para web y corporate. En service, la repetición válida número 3 se hizo tras restablecerse la cuota, por lo que el tiempo de esa categoría queda más expuesto a variaciones entre ventanas. No se ejecutaron dos brazos simultáneamente.
- La metodología de trazas JSONL, comprobaciones deterministas y grader de calidad sigue la [guía oficial de OpenAI Docs para evaluaciones de Codex](https://developers.openai.com/blog/eval-skills). Las comprobaciones reservadas se aplicaron después de que el agente entregó el código.

## Resultados por corrida

`V/R` = pruebas visibles / reservadas aprobadas. Cada enlace “traza” lleva a la cronología de la iteración; en la misma carpeta están la transcripción JSONL, el código resultante, el diff y las métricas JSON. “Total / no caché” incluye Codex + JEV para Nexus, y solo Codex para el brazo base.

| Iteración | Brazo | Tiempo | Tokens total / no caché | V/R | Cierre | Evidencia |
| --- | --- | ---: | ---: | --- | --- | --- |
| WEB 1 | Codex solo | 169,6 s | 295 716 / 44 068 | 6/6 + 5/5 | completo | [traza](benchmarks/evidence/2026-09-27/web-codex-1/timeline.md) |
| WEB 1 | Nexus | 140,4 s | 105 080 / 19 576 | 6/6 + 4/5 | `human_review` | [traza](benchmarks/evidence/2026-09-27/web-nexus-1/timeline.md) |
| WEB 2 | Nexus | 94,4 s | 82 329 / 16 025 | 6/6 + 5/5 | `human_review` | [traza](benchmarks/evidence/2026-09-27/web-nexus-2/timeline.md) |
| WEB 2 | Codex solo | 126,5 s | 158 118 / 25 766 | 6/6 + 5/5 | completo | [traza](benchmarks/evidence/2026-09-27/web-codex-2/timeline.md) |
| CORP 1 | Nexus | 92,0 s | 96 372 / 13 940 | 6/6 + 4/4 | `human_review` | [traza](benchmarks/evidence/2026-09-27/corporate-nexus-1/timeline.md) |
| CORP 1 | Codex solo | 98,6 s | 227 945 / 27 753 | 6/6 + 4/4 | completo | [traza](benchmarks/evidence/2026-09-27/corporate-codex-1/timeline.md) |
| CORP 2 | Codex solo | 116,7 s | 248 931 / 25 443 | 6/6 + 4/4 | completo | [traza](benchmarks/evidence/2026-09-27/corporate-codex-2/timeline.md) |
| CORP 2 | Nexus | 116,5 s | 132 107 / 18 443 | 6/6 + 4/4 | `human_review` | [traza](benchmarks/evidence/2026-09-27/corporate-nexus-2/timeline.md) |
| SVC 1 | Nexus | 122,1 s | 137 290 / 21 578 | 5/5 + 4/4 | `human_review` | [traza](benchmarks/evidence/2026-09-27/service-nexus-1/timeline.md) |
| SVC 1 | Codex solo | 107,9 s | 160 471 / 20 951 | 5/5 + 4/4 | completo | [traza](benchmarks/evidence/2026-09-27/service-codex-1/timeline.md) |
| SVC 3 | Codex solo | 115,8 s | 248 778 / 28 362 | 5/5 + 4/4 | completo | [traza](benchmarks/evidence/2026-09-27/service-codex-3/timeline.md) |
| SVC 3 | Nexus | 126,5 s | 140 203 / 19 371 | 5/5 + 4/4 | `human_review` | [traza](benchmarks/evidence/2026-09-27/service-nexus-3/timeline.md) |

Intentos censurados, **excluidos** de todas las medias y del 60/59: [SVC Codex 2](benchmarks/evidence/2026-09-27/service-codex-2/timeline.md) recibió `You've hit your usage limit` tras 64,7 s, antes de completar la tarea; [SVC Nexus 2](benchmarks/evidence/2026-09-27/service-nexus-2/timeline.md) encontró el mismo límite tras 26,7 s, con 0 tokens de turno completo y sin llamada a JEV. Los logs originales se conservaron.

## Calidad de entrega y referencias al código

- **Web:** ambas entregas de Codex solo pasaron 11/11 comprobaciones. Nexus pasó 10/11 en WEB 1: [`selectProducts`](benchmarks/evidence/2026-09-27/web-nexus-1/src/app.js), líneas 18–29, trataba cualquier orden distinto de precio como nombre ascendente, y la interfaz no ofrecía `name-desc`, pese a pedirlo el prompt; lo demuestra el [test reservado](benchmarks/evidence/2026-09-27/web-nexus-1/heldout-tests.final.log). [Codex WEB 1](benchmarks/evidence/2026-09-27/web-codex-1/src/app.js), línea 20, sí lo implementó. La segunda entrega Nexus también lo hizo en [`web-nexus-2/src/app.js`](benchmarks/evidence/2026-09-27/web-nexus-2/src/app.js), líneas 28–35. En los cuatro resultados se comprobó escape de HTML; la revisión estática encontró controles etiquetados y CSS responsive. **No se verificó visualmente en un navegador**: la política del navegador bloqueó abrir el archivo local y no se eludió esa restricción.
- **Módulo empresarial:** los cuatro resultados pasaron 10/10 pruebas. El código entregado por [Codex](benchmarks/evidence/2026-09-27/corporate-codex-1/src/approvals.js) y por [Nexus](benchmarks/evidence/2026-09-27/corporate-nexus-1/src/approvals.js) valida roles, autoaprobación, importe, transición, idempotencia y auditoría. Queda un límite compartido para producción: ambos exponen `requests` y `audit` como propiedades públicas mutables aunque `get()` y `history()` devuelven copias; esta batería no certifica aislamiento frente a código que acceda directamente a esas propiedades.
- **Microservicio:** los cuatro resultados válidos pasaron 9/9 pruebas. El código de [Codex](benchmarks/evidence/2026-09-27/service-codex-1/src/server.js) y [Nexus](benchmarks/evidence/2026-09-27/service-nexus-1/src/server.js) cubrió validación, límites de cuerpo, JSON de error e idempotencia. Ambos usan mapas en memoria: reiniciar el proceso pierde pedidos y claves; no se pidió persistencia ni se puntuó como defecto, pero **no** son diseños listos para producción.
- **Alcance:** en las 12 entregas válidas las pruebas visibles permanecieron intactas, `git diff --check` pasó y solo cambiaron los archivos previstos. Ver [`summary.json`](benchmarks/evidence/2026-09-27/summary.json), campos `testsUnmodified`, `diffCheck` y `changedPaths`.
- **Cierre autónomo:** JEV respondió a las seis ejecuciones Nexus válidas, pero su [gate de confianza](src/jev.rs), líneas 334–354, las convirtió a `human_review`; el [flujo del harness](src/main.rs), líneas 478–509, verificó y luego llamó a JEV. Los seis [registros de sesión](benchmarks/evidence/2026-09-27/) muestran la selección, probabilidad, hora y tokens. Un usuario tendría que revisar esas entregas; no se deben presentar como seis tareas finalizadas automáticamente.

## Dónde ver cada llamada a herramienta y a JEV

Cada carpeta de [evidencia](benchmarks/evidence/2026-09-27/) contiene:

- `timeline.md`: prompt externo exacto, llamadas a herramientas en orden, verificación y momento en que quedó registrada la decisión JEV.
- `agent.stdout.log` / `agent.stderr.log`: salida del ejecutable. En Codex solo, stdout es el JSONL de `codex exec --json`; en Nexus es el log de progreso del harness (incluye “Consultando JEV…”).
- `codex-rollout-1.jsonl` en Nexus: transcripción operativa recuperada del sandbox con mensajes de usuario/asistente, llamadas a herramientas y resultados. Se omitieron instrucciones internas, razonamiento privado y credenciales. `codex-internal-prompts.txt` reúne los mensajes de usuario que recibió Codex dentro del harness.
- `nexus-session.json`: estado, comandos de verificación y decisión de JEV con marca temporal. `jev-request-reconstruction.json` reproduce los campos que habría formado [`request_body()`](src/jev.rs) a partir del estado guardado; está **etiquetado como reconstrucción**, no como captura HTTP. La respuesta HTTP bruta de JEV no se persistió en esta versión; solo se conserva su decisión resumida. En SVC Nexus 2 no hubo llamada a JEV.
- `codex-usage.json` en Nexus: desglose de entrada, caché y salida extraído del JSONL interno; `metrics.json`: resultado estructurado; `implementation.diff` y `src/`: código exacto entregado; `visible-tests.log` y `heldout-tests.final.log`: pruebas.

Como ejemplo directo: [cronología WEB Nexus 1](benchmarks/evidence/2026-09-27/web-nexus-1/timeline.md) enumera `exec_command`, `apply_patch`, `node --test` y JEV a las 19:51:18 UTC; [registro de sesión](benchmarks/evidence/2026-09-27/web-nexus-1/nexus-session.json) muestra que JEV propuso `finish` con confianza 0,35, pero el gate dejó `human_review`.

## Incidencias y límites metodológicos

1. La primera versión de una prueba reservada web exigía `<label>` en el HTML estático, aunque Nexus lo generaba correctamente al montar la página. Se corrigió el evaluador para aceptar HTML/JS y se añadió una prueba de `name-desc`, expresamente requerido. Los logs iniciales y finales se conservan; la nota de calidad WEB 1 proviene del **test final**, no del falso positivo inicial.
2. La reexportación de tests HTTP se ejecutó inicialmente en un sandbox que impedía escuchar en `127.0.0.1` y produjo falsos 0/4. Se repitió con permiso de loopback idéntico al de la medición original. Los logs del falso fallo se guardaron como `heldout-tests.export-sandbox-error.log` y no se incluyeron en los resultados.
3. La cuota de Codex interrumpió SVC 2; los intentos se censuraron y se repitieron en SVC 3 tras el restablecimiento. Docker Desktop se detuvo después; se recuperaron los JSONL desde los contenedores OpenShell detenidos sin copiar `auth.json` ni credenciales.
4. `NEXUSMIND_REQUIRED=1` completó la conexión, pero las seis sesiones Nexus válidas registraron **0 fuentes de contexto**. Esta batería no demuestra ningún beneficio causal de recuerdos NexusMind; mide el harness completo con la integración activa pero sin material recuperado.
5. Hay diferencias inevitables entre host macOS y contenedor Linux, entre cachés y entre ventanas temporales. El promedio de seis pares no establece significación estadística ni coste real. Tampoco hubo validación visual de la web, carga del microservicio, persistencia o pruebas de seguridad completas. Estos son benchmarks reproducibles de tareas acotadas, no certificación de producción.

## Reproducción

El ejecutor es [`benchmarks/run-realworld.mjs`](benchmarks/run-realworld.mjs); la extracción de JSONL de OpenShell está en [`collect-nexus-usage.mjs`](benchmarks/collect-nexus-usage.mjs) y el exportador en [`export-evidence.mjs`](benchmarks/export-evidence.mjs). Usa claves en variables de entorno **solo durante la ejecución**; nunca las pongas en argumentos, archivos de fixture o logs. Cada corrida crea una carpeta Git nueva y falla si la carpeta ya existe. Las evidencias exportadas de las 14 corridas están en [`benchmarks/evidence/2026-09-27/`](benchmarks/evidence/2026-09-27/).
