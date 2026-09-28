# service / codex / iteración 1

## Pedido exacto de esta iteración

> Implementa un microservicio HTTP de pedidos sin dependencias externas: GET /health, POST /orders y GET /orders/:id, JSON consistente, validación estricta de sku y cantidad entera positiva, idempotencia por cabecera con 409 si la misma clave trae otro payload, 415 para content-type inválido, 413 para cuerpos mayores de 64 KiB y 404 JSON para rutas desconocidas. Mantén createServer y pasa node --test; no modifiques las pruebas.

## Eventos de ejecución

| Hora UTC | Etapa/herramienta | Evidencia abreviada |
| --- | --- | --- |
| orden 2 | command_execution (item.started) | /bin/zsh -lc "git rev-parse --show-toplevel && if [ -d .codegraph ]; then codegraph status; else pwd; fi && rg --files -g '"'!*node_modules*'"' -g '"'!*.lock'"'" |
| orden 3 | command_execution (item.completed) | /bin/zsh -lc "git rev-parse --show-toplevel && if [ -d .codegraph ]; then codegraph status; else pwd; fi && rg --files -g '"'!*node_modules*'"' -g '"'!*.lock'"'" |
| orden 4 | command_execution (item.started) | /bin/zsh -lc "sed -n '1,260p' src/server.js && sed -n '1,320p' test/server.test.js && sed -n '1,120p' package.json && git status --short" |
| orden 5 | command_execution (item.completed) | /bin/zsh -lc "sed -n '1,260p' src/server.js && sed -n '1,320p' test/server.test.js && sed -n '1,120p' package.json && git status --short" |
| orden 6 | file_change (item.started) | /private/tmp/nexus-bench-realworld-20260927/service-codex-1/src/server.js |
| orden 7 | file_change (item.completed) | /private/tmp/nexus-bench-realworld-20260927/service-codex-1/src/server.js |
| orden 8 | command_execution (item.started) | /bin/zsh -lc "npm test && node --input-type=module - <<'EOF' import { createServer } from './src/server.js'; const server = createServer(); await new Promise((resolve) => server.li |
| orden 9 | command_execution (item.completed) | /bin/zsh -lc "npm test && node --input-type=module - <<'EOF' import { createServer } from './src/server.js'; const server = createServer(); await new Promise((resolve) => server.li |
| orden 10 | command_execution (item.started) | /bin/zsh -lc 'node --check src/server.js && git diff --check && git status --short' |
| orden 11 | command_execution (item.completed) | /bin/zsh -lc 'node --check src/server.js && git diff --check && git status --short' |

La transcripción JSONL conserva mensajes operativos, llamadas a herramientas y sus resultados. Se omiten instrucciones internas, razonamiento privado y credenciales. El cuerpo HTTP completo de la petición/respuesta de JEV no fue persistido por esta versión del harness; `nexus-session.json` conserva la decisión y su marca temporal.
