# API autenticada sin cuota HTTP por plan

## Qué y por qué

Por petición explícita del usuario se eliminó el middleware que devolvía
`{"error":"Rate limit exceeded","code":"rate_limited"}`. No se elevó de nuevo
la cuota: se retiró por completo el token bucket de solicitudes autenticadas.
Panel, MCP y runners podían compartir el bucket del mismo usuario.

Se quitaron la capa/estado del router, el módulo público y el archivo obsoleto
`apps/backend/src/api/rate_limit.rs`. No se modificaron los planes almacenados.
Autenticación, permisos, auditoría, límites de cuerpos, presupuestos de agentes,
leases y protecciones/concurrencia del puente de modelos siguen vigentes. Los
límites del proveedor externo no se pueden eliminar desde NexusMind.

## Reconciliación de entregas

La búsqueda semántica encontró que `3e43ae6`, entregado por el release
`42061705067` desde `feat/factory-auto-environment`, ya había aumentado la cuota
a 2000. El posterior release del reviewer (`1a9b460`) había usado la rama de
despliegue anterior y omitió esa entrega y la automatización `8ad34fe`.

Esta entrega integra de nuevo `8ad34fe` con los cambios de comprobaciones
`dee35b1`/`a4806e5` y el arreglo de hijos de proyecto `1a9b460`. El conflicto se
resolvió conservando **256 checks** y **16 entrypoints**; ambas regresiones pasan.
La nueva petición de quitar la cuota sustituye al aumento anterior a 2000.
El runner existente `runner-8ad34fe` permanece activo sin modificarlo.

**Aprendido:** la última entrega de producción puede venir de otra rama. Antes
de publicar, revisar los últimos releases exitosos y sus SHA, no asumir que
`deploy/factory-workflows-v95` representa por sí sola todo lo ya publicado.

## Evidencia

- Desplegado: `721db9c651f3aac4e62782ceba5c3fd09c1bb5fc`.
- Eliminación de la cuota: `01e0a02`; reconciliación: `38523d6`.
- Release: https://github.com/smart-coder-labs/nexus-mind/actions/runs/42073472791
- Siete jobs correctos: 1861 tests de biblioteca aprobados/4 ignorados,
  27 de integración, 83 Python y 137 UI; check all-targets y builds aprobados.
- La regresión del router real falló antes del cambio y ahora pasa: 120 peticiones
  autenticadas, sin cabeceras de cuota, y acceso anónimo rechazado con 401.
- Comprobación real en producción: **120 HTTP200**, sin `X-RateLimit-*`, y
  petición sin autenticación **HTTP401**. Clave temporal revocada en `finally`.
- Backend/worker: `sha256:e0379e94ad64d60584afa3e7bbff3ab688f3e47d0ff361bffb9ba295d0ed7c17`.
- Backup SQLite verificado: `/data/pre-workflows-20261105T173131Z.db`.
- Backend/egress desplegados; health público `status=ok`, `db=ok`; sin DiskPressure.

## Siguientes pasos

Recargar el panel para reintentar las peticiones previamente rechazadas. No se
editaron agentes, permisos, modelos, schedules ni credenciales existentes; sin
migración nueva y sin ejecuciones de modelos de pago. La integración normal de
las ramas/PR sigue pendiente; no se fusionó main ni se borraron ramas. Esta
prueba HTTP no equivale a QA de los 49 repositorios ni a una revisión live.

Archivos relevantes: `apps/backend/src/api/{router,mod}.rs`, eliminación de
`rate_limit.rs`, `apps/backend/tests/factory_workflows_api.rs` y la reconciliación
en `apps/backend/src/db/workflow_runtime.rs`.
