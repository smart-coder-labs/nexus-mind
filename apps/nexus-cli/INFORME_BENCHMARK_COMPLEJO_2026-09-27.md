# Benchmark comparativo: tareas de programación con casos límite

Fecha: 27 de septiembre de 2026. Se comparó Codex CLI sin harness con Nexus (Codex CLI dentro de OpenShell, NexusMind obligatorio y decisión final de JEV). Se utilizó el modelo `gpt-5.6-terra` en ambas condiciones y el mismo prompt por tarea. Cada ejecución empezó desde una copia Git limpia e idéntica. No se modificaron los tests de las tareas.

## Tareas y criterio de éxito

| Caso | Trabajo solicitado | Tests visibles | Tests independientes, no mostrados al agente |
| --- | --- | ---: | ---: |
| CSV | Sustituir un `split` ingenuo por un parser con comillas escapadas, comas y saltos de línea dentro de campos, BOM, CRLF y errores de sintaxis. | 5 | 5 |
| LRU + TTL | Implementar expulsión por recencia, actualización, capacidad válida y caducidad con reloj inyectable, incluyendo el límite exacto. | 5 | 4 |

Las plantillas están en `benchmarks/fixtures/` y las pruebas independientes en `benchmarks/heldout/`. Se evaluaron con `node --test`. Los prompts exactos fueron:

- CSV: `Implementa parseCsv en src/csv.js para que pase todas las pruebas de node --test. Mantén la API pública y no modifiques las pruebas. Verifica ejecutando node --test.`
- LRU: `Implementa LruCache en src/cache.js para que pase todas las pruebas de node --test. Mantén la API pública y no modifiques las pruebas. Verifica ejecutando node --test.`

## Resultados observados

| Caso | Condición | Tests visibles | Tests independientes | Tokens Codex | Tokens JEV | Tokens totales medidos | Tiempo aproximado | Decisión Nexus/JEV |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| CSV | Codex solo | 5/5 | 5/5 | 121 273 | — | 121 273 | 87 s | — |
| CSV | Nexus | 5/5 | 5/5 | 78 632 | 787 | 79 419 | 92 s | `finish` (confianza 0,78) |
| LRU | Codex solo | 5/5 | 4/4 | 148 678 | — | 148 678 | 40 s | — |
| LRU | Nexus | 5/5 | 4/4 | 71 972 | 782 | 72 754 | 46 s | `human_review`: JEV propuso terminar, pero la confianza 0,64 no superó el umbral de seguridad |

En estas dos muestras, Nexus usó aproximadamente 34,5 % y 51,1 % menos tokens medidos, respectivamente, pero tardó unos 5 y 6 segundos más. Ambos brazos produjeron código correcto para todas las pruebas visibles e independientes. El resultado LRU de Nexus **no** se dio por terminado automáticamente: requiere revisión humana aunque sus pruebas pasen. Esto es una decisión conservadora del gate de JEV, no un fallo de ejecución.

## Integraciones y límites de la comparación

- OpenShell fue obligatorio en Nexus y ejecutó Codex en el contenedor local. La imagen usada llevaba Codex CLI 0.154.0; Codex solo se ejecutó en el host con CLI 0.157.1. Esta diferencia de versión limita una comparación causal estricta.
- `NEXUSMIND_REQUIRED=1` y las credenciales temporales permitieron que la recuperación de NexusMind se completara sin error, pero los dos casos registraron **cero fuentes de contexto**. Por tanto, esta prueba valida conectividad, no demuestra una mejora atribuible a recuerdos recuperados.
- JEV se invocó para ambos casos de Nexus: 710/77 tokens de entrada/salida para CSV y 705/77 para LRU. Los tokens de Codex provienen de `turn.completed.usage` en el brazo solo y de `state.tokens_used` en Nexus; los de JEV provienen del registro de decisión.
- Los tiempos se estimaron a partir de creación/última modificación de los logs. Las ejecuciones se solaparon parcialmente y compitieron por recursos; los tiempos **no** son una medición controlada de latencia.
- Los tokens de entrada incluyen tokens de caché (Codex solo informó 104 192 y 125 440 tokens cacheados). No equivalen a coste facturado. Nexus no guardó una descomposición comparable de caché.
- Dos tareas pequeñas de biblioteca y una ejecución por brazo no bastan para afirmar superioridad general. Para una conclusión robusta hacen falta repeticiones, versión idéntica de Codex, orden aleatorizado, tareas de varios archivos y una base de conocimiento de NexusMind con recuerdos pertinentes.

## Incidencias encontradas

La primera tentativa de CSV con Nexus se detuvo antes de invocar Codex porque el proceso aislado no heredó `NEXUSMIND_API_KEY`. No cambió ningún archivo de la tarea. Se repitió suministrando las claves por entrada temporal oculta al proceso y ambas ejecuciones concluyeron. No se guardaron claves en el repositorio ni en este informe.

Los cuatro repositorios temporales usados para la medición se encuentran en `/private/tmp/nexus-bench-complex.kBE1KY/` y contienen las salidas JSONL/logs y los cambios para auditoría local mientras no se limpie el directorio temporal.
