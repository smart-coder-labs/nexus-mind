# service / codex / iteración 3

## Pedido exacto de esta iteración

> Implementa un microservicio HTTP de pedidos sin dependencias externas: GET /health, POST /orders y GET /orders/:id, JSON consistente, validación estricta de sku y cantidad entera positiva, idempotencia por cabecera con 409 si la misma clave trae otro payload, 415 para content-type inválido, 413 para cuerpos mayores de 64 KiB y 404 JSON para rutas desconocidas. Mantén createServer y pasa node --test; no modifiques las pruebas.

## Eventos de ejecución

| Hora UTC | Etapa/herramienta | Evidencia abreviada |
| --- | --- | --- |
| orden 2 | command_execution (item.started) | /bin/zsh -lc 'git rev-parse --show-toplevel && if [ -d .codegraph ]; then echo codegraph-present; else echo codegraph-missing; fi && git status --short' |
| orden 3 | command_execution (item.completed) | /bin/zsh -lc 'git rev-parse --show-toplevel && if [ -d .codegraph ]; then echo codegraph-present; else echo codegraph-missing; fi && git status --short' |
| orden 4 | command_execution (item.started) | /bin/zsh -lc 'gentle-ai codegraph init --cwd /private/tmp/nexus-bench-realworld-20260927/service-codex-3' |
| orden 5 | command_execution (item.completed) | /bin/zsh -lc 'gentle-ai codegraph init --cwd /private/tmp/nexus-bench-realworld-20260927/service-codex-3' |
| orden 6 | command_execution (item.started) | /bin/zsh -lc "rg --files -g '"'!node_modules'"' -g '"'!package-lock.json'"' && printf '\\n--- package ---\\n' && sed -n '1,200p' package.json" |
| orden 7 | command_execution (item.completed) | /bin/zsh -lc "rg --files -g '"'!node_modules'"' -g '"'!package-lock.json'"' && printf '\\n--- package ---\\n' && sed -n '1,200p' package.json" |
| orden 8 | command_execution (item.started) | /bin/zsh -lc "sed -n '1,260p' src/server.js && printf '\\n--- tests ---\\n' && sed -n '1,360p' test/server.test.js" |
| orden 9 | command_execution (item.completed) | /bin/zsh -lc "sed -n '1,260p' src/server.js && printf '\\n--- tests ---\\n' && sed -n '1,360p' test/server.test.js" |
| orden 10 | file_change (item.started) | /private/tmp/nexus-bench-realworld-20260927/service-codex-3/src/server.js |
| orden 11 | file_change (item.completed) | /private/tmp/nexus-bench-realworld-20260927/service-codex-3/src/server.js |
| orden 12 | file_change (item.started) | /private/tmp/nexus-bench-realworld-20260927/service-codex-3/src/server.js |
| orden 13 | file_change (item.completed) | /private/tmp/nexus-bench-realworld-20260927/service-codex-3/src/server.js |
| orden 14 | command_execution (item.started) | /bin/zsh -lc 'npm test' |
| orden 15 | command_execution (item.completed) | /bin/zsh -lc 'npm test' |
| orden 16 | command_execution (item.started) | /bin/zsh -lc 'git diff --check && git diff -- src/server.js && git status --short' |
| orden 17 | command_execution (item.completed) | /bin/zsh -lc 'git diff --check && git diff -- src/server.js && git status --short' |

La transcripción JSONL conserva mensajes operativos, llamadas a herramientas y sus resultados. Se omiten instrucciones internas, razonamiento privado y credenciales. El cuerpo HTTP completo de la petición/respuesta de JEV no fue persistido por esta versión del harness; `nexus-session.json` conserva la decisión y su marca temporal.
