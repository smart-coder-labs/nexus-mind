# Fuentes actualizadas antes de ejecutar un workflow

## Qué cambia

El inventario ahora registra la rama de cada repositorio. Una ejecución nueva
normal requiere ramas confirmadas y su preparación obtiene la cabeza remota de
**cada rama confirmada**, dentro de clones privados. No hace pull/reset/stash en los checkouts
personales, no incluye archivos sin commit y no presupone `main` ni `master`.

El alcance, permisos y definición se congelan al crear el run. Los SHA están
marcados como pendientes hasta que la preparación inicial, con lease vigente,
termina y sella todos los commits de forma atómica. Ninguna otra fase puede
arrancar antes. `sources.pinned` y `checkout-manifest.json` dejan la trazabilidad.

Después de ese sellado, el snapshot es inmutable: un reintento/reparación no hace
fetch de ramas nuevas. Si se pierde la respuesta del API tras preparar, se reutiliza
el manifiesto local con la misma identidad de fuentes; no se vuelve a resolver
upstream. Otra ejecución nueva sí obtiene las nuevas cabezas.

El consentimiento inicial `fix` sigue siendo autorización de la tarea, no prueba
ni aprobación de publicación de un SHA. No se admiten aprobaciones de publicación
mientras los commits estén pendientes; no se heredan verificaciones ni aprobaciones
de publicación anteriores. Un fallo de fetch bloquea antes de llamar agentes,
sin recurrir silenciosamente al commit viejo.

## Activación y compatibilidad

1. Actualizar backend y admin primero; no requiere una migración SQL nueva.
2. Actualizar el runner coordinadamente. Anuncia `workspace-sources:v1`.
3. Redescubrir y confirmar **una vez** el inventario que incluye las ramas. La
   confirmación verifica también la rama; no puede introducir otra no descubierta.
4. Configurar acceso Git de lectura, si los repositorios son privados.
5. Crear una ejecución nueva y comprobar su manifiesto/evento de fuentes fijadas.

Los runs antiguos mantienen su contrato de commits fijados: no se reescriben ni
se promueven sus evidencias. Para iniciar otro run normal desde un inventario
antiguo hay que descubrir/confirmar las ramas: el backend devuelve
`source_branch_not_confirmed` en vez de crear silenciosamente otro snapshot viejo.
La interfaz distingue ramas con actualización por ejecución del SHA legacy.
Un runner antiguo no puede reclamar la preparación del nuevo protocolo.

Para una reproducción exacta/offline existe el opt-in explícito
`pinned_sources: true` en `POST /v1/factory/workflow-runs` y el helper interno
`start_pinned_run`. No es el valor predeterminado ni un fallback si falla fetch;
las entradas automáticas de Linear/GitHub/etc. siempre usan actualización. Un
campo igual dentro del texto de la tarea no cambia la política de fuentes.

La primera entrega admite HTTPS en hosts autorizados por el operador, por defecto
`github.com`. Rechaza URLs con credenciales, redirecciones, transportes no
admitidos y ramas inválidas. No procesa hooks de Git, submódulos ni código del
repositorio en el host. Un workflow de este modo necesita una única fase inicial
`prepare` con adapter `prepare-workspace`.

## Acceso a repositorios privados

El runner no hereda credenciales del usuario, del modelo, de producción ni el token
de registro del runner para hablar con GitHub. El operador puede proporcionar un
perfil de lectura explícito en su configuración privada:

```json
{
  "source_hosts": ["github.com"],
  "source_credentials": {
    "WORKSPACE_ID": "/ruta/privada/github-source-read.json"
  }
}
```

Contenido del archivo de perfil (propietario del runner, regular, no symlink,
modo `0600`, fuera de las raíces de repositorios):

```json
{
  "host": "github.com",
  "username": "x-access-token",
  "token": "<token-de-lectura-autorizado>"
}
```

Usar acceso `Contents: read` limitado a los repositorios necesarios. El perfil se
aplica solamente al host indicado, durante fetch, mediante configuración efímera
en el entorno del proceso. No queda en argumentos de proceso, `.git/config`,
manifiestos, snapshots, artefactos ni en el entorno del agente. La primera versión
usa un host de credenciales por workspace; ampliar a perfiles por host si llega
un workspace con múltiples proveedores privados.

No se copiaron credenciales ni se amplió ninguna allowlist en Oracle durante la
implementación. Sin acceso de lectura, la fase se bloqueará con el repositorio y
la indicación de revisar rama/conectividad/perfil, antes del trabajo de modelos.

## Comprobaciones

- Git real contra repositorio bare local como sustituto de HTTPS: upstream más
  reciente que el checkout original, rama distinta de main/master, trabajo sucio
  original intacto, dos ejecuciones con nuevos commits y reintentos estables.
- Recuperación del manifiesto después de una finalización ambigua; no refetch.
- Perfil privado de lectura usado solo por fetch y ausente de manifiesto/config Git.
- Rechazo de hosts/URLs no autorizados, credenciales embebidas, ramas y perfiles malos.
- Backend: lease y artefacto obligatorios, sellado inicial de cabezas, capacidad
  requerida, otras fases bloqueadas mientras hay fuentes pendientes y rechazo de
  cambios de base en un reintento ya fijado.
- UI: mostrar/conservar la rama al confirmar, sin conceder escritura implícitamente.

## Registro de sesión

### Goal
Corregir que las ejecuciones nuevas siguieran tomando el SHA viejo de Accounting
aunque el usuario ya había integrado el lockfile correcto en GitHub master.

### Accomplished
Cambios aislados en `/tmp/nexusmind-workflow-fresh-sources`, rama
`fix/workflow-fresh-sources`: descubrimiento de ramas, fetch explícito en clones
privados, protocolo/backend de sellado inicial y pruebas de regresión. No se
modificaron inventarios, aprobaciones o ejecuciones de producción.

### Discoveries
La copia original no necesita un pull: puede seguir desactualizada o sucia y la
nueva ejecución obtiene la rama remota en su propia copia. La actualización y el
sellado deben ocurrir una sola vez, antes de cualquier otra fase. Simplemente
hacer fetch en `prepare` sin cambiar el contrato backend produciría evidencia
contra un snapshot antiguo o `unauthorized_change_set`.

### Next Steps
Completar la validación y CI, después desplegar coordinadamente y confirmar el
inventario de ramas. El acceso privado Git requiere un perfil autorizado por el
operador; no se asume ni se toma prestado de otro componente. Este documento no
reclama un despliegue ni una ejecución de producción satisfactoria.

### Relevant Files
- `scripts/factory/runner.py`, `test_runner.py`.
- `apps/backend/src/factory/workflows.rs` — ramas confirmadas.
- `apps/backend/src/db/workflows.rs`, `workflow_execution.rs` — sellado y fencing.
- `apps/backend/tests/factory_workflows.rs` — regresión de fuente/lease/reintento.
- `apps/admin/src/pages/factory/workflows/RepositoryInventory*`, `types.ts`.
