# Diagnóstico SSH: cola de Issue Resolver

Inspección de solo lectura: **2026-11-04 05:56–05:58 UTC**. Estado puntual; las cantidades cambian mientras trabaja el sistema. No se cancelaron, reintentaron o lanzaron ejecuciones, ni se modificaron autenticación/schedules.

## Kasymir Issues Resolver

- **9 queued + 1 running**. El activo: `9cc3f93d-390d-4c8d-b030-ba216ab406fe`, creado 03:00:27, arrancó 05:55:26 UTC. Emitió `verification.started` a las 05:56:02.
- Lease con heartbeat reciente a las **05:57:08**, vencimiento 05:59:08. El worker está vivo: no es un proceso abandonado según esta observación.
- Los nueve queued vienen del **schedule cada 15 minutos**, no son nueve issues diferentes. Son ejecuciones genéricas del resolver sin work item/issue fijo y arrancarán el fanout correspondiente.
- El scheduler añade cada ocurrencia debida aunque ese mismo agente ya tenga otra queued/running. `misfire_policy=run_once` no coalesce por agente la cola existente.
- El worker local atiende **un claim y espera execute_claim antes del siguiente**. Los máximos de concurrencia SQL son techos, no un pool de ejecución paralelo. Los trabajos de QA y otros agentes comparten ese worker.
- Resultados recientes: tres ejecuciones anteriores finalizaron `blocked_policy / resolver_verification_failed`; no publicaron un arreglo exitoso. Las dos últimas inspeccionadas reportan un issue intentado, cero resueltos y una verificación fallida. Sus tiempos observados fueron aproximadamente 2–12 minutos, aparte del resto de trabajos de la cola.

Código: `queries::enqueue_due_autonomous_agent_runs`, `queries::claim_next_autonomous_agent_run`, `worker::spawn_local_worker`.

## NexusMind Issue Resolver

- **8 queued**, encolados a las 01:47:50 por reconciliación/webhook, dirigidos a issues diferentes: #215, #244, #339, #249, #245, #242, #255 y #243 de `smart-coder-labs/nexus-mind`.
- Usa Claude. La salud persistida es **reauth_required / claude_auth_required**. El selector de claims excluye agentes Claude mientras no esté ready, por lo que esos ocho no pueden iniciar aunque sean más antiguos.
- Además conserva la fuente histórica de modelos (`model_source` sin opt-in). La reautenticación no demuestra por sí sola que todas las demás comprobaciones/pasarela estén operativas. No se cambió silenciosamente de cuenta/modelo.

No se encontraron work items duplicados activos para una misma definición/repositorio/issue.

## Acciones propuestas, no aplicadas

1. Coalescer schedules de sondeo: si hay una ejecución genérica del mismo agente queued/running, avanzar la próxima fecha sin crear otra redundante.
2. Conservar el activo y decidir explícitamente cómo consolidar los queued redundantes sin perder trabajo dirigido a un issue concreto.
3. Mostrar en el admin el motivo real de espera: ejecución activa, autenticación, límites o verificación fallida; no solo «queued».
4. Atender la verificación fallida con su evidencia, no saltarse el gate ni lanzar más runs iguales.
5. Reparar Claude mediante consentimiento/autenticación del operador y revisar su fuente de modelo antes de reactivar los ocho trabajos.
6. Si se incorpora un pool, respetar exclusión de escritores por repositorio y presupuestos; aumentar un número de concurrencia por sí solo no convierte el worker actual en paralelo.

Otras solicitudes se conservan en [PENDIENTES_OPERADOR.md](PENDIENTES_OPERADOR.md): revisión 7 guardada/aplicada 0, presets de sandbox multi-repo/integrados y agentes por proyecto.
