# service / nexus / iteración 2

## Pedido exacto de esta iteración

> Implementa un microservicio HTTP de pedidos sin dependencias externas: GET /health, POST /orders y GET /orders/:id, JSON consistente, validación estricta de sku y cantidad entera positiva, idempotencia por cabecera con 409 si la misma clave trae otro payload, 415 para content-type inválido, 413 para cuerpos mayores de 64 KiB y 404 JSON para rutas desconocidas. Mantén createServer y pasa node --test; no modifiques las pruebas.

## Eventos de ejecución

| Hora UTC | Etapa/herramienta | Evidencia abreviada |
| --- | --- | --- |
| — | JEV no invocado | La ejecución terminó antes de la fase de decisión. |

La transcripción JSONL conserva mensajes operativos, llamadas a herramientas y sus resultados. Se omiten instrucciones internas, razonamiento privado y credenciales. El cuerpo HTTP completo de la petición/respuesta de JEV no fue persistido por esta versión del harness; `nexus-session.json` conserva la decisión y su marca temporal.
