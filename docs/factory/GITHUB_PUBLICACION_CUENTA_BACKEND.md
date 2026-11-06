# Publicar con la cuenta autenticada del backend

**Publicado y verificado:** [recibo de release y estado del reviewer](PUBLICACION_GITHUB_2026-11-04.md).

La opción predeterminada en **Publicación y límites → Cuenta para publicar en GitHub** es la cuenta del CLI `gh` del backend. No es la sesión del navegador y no requiere pegar otro token en cada workflow.

## Qué hace

- Publica la rama del cambio verificado y abre un **PR draft**.
- Fija el login seleccionado en la definición/snapshot. Si cambia la cuenta, bloquea y pide revisar la identidad; no publica silenciosamente como otro usuario.
- Comprueba acceso de escritura efectivo en GitHub, repositorios confirmados como escribibles y permisos/aprobaciones del workflow.
- Conserva los gates de tests/build/QA para el SHA final, idempotencia, CI requerido, presupuesto y límites. No autoriza merges ni despliegues de aplicaciones.
- Las credenciales solo se utilizan en el control plane; no se guardan en snapshots, prompts, artefactos o configuración del runner.

Si necesitas otra cuenta o una GitHub App, usa **Otra identidad de GitHub · avanzado**. Los conectores existentes conservan su allowlist y su revocación; nunca se sustituye un conector inválido por la cuenta global como fallback.

## Autorización de la plataforma

La cuenta compartida del backend no es autoridad universal para todas las organizaciones. El operador autoriza expresamente el tenant con `FACTORY_GITHUB_PUBLISH_ORG_ID`. El release lo obtiene de la variable homónima de GitHub Actions y lo aplica al contenedor backend. En esta instalación corresponde a `bdf4b2aa-3f52-422e-8bee-a1babbb7d337`.

`GET /v1/factory/github/publication-identity` requiere `factory_policy:read` y devuelve solo habilitación, login y motivo de bloqueo. No devuelve tokens. Organizaciones no autorizadas deben usar un conector propio o solicitar autorización al operador.

Las definiciones nuevas usan `github_auth: server_gh_cli` y fijan `github_login`. Las anteriores que tenían un conector siguen con ese conector; no se migra automáticamente su autoridad.

## Reviewer: corrección incluida

Las revisiones de agentes son inmutables. Su columna inicial `validation_status=pending` no cambia al validar: el estado vigente vive en `autonomous_agent_validations`, un ledger separado. El filtro de webhooks/reconciliación estaba leyendo la columna inmutable en lugar de ese ledger. Esto aceptaba el evento pero descartaba agentes realmente validados.

El filtro ahora consulta la validación del ID de revisión y hash actuales. El test reproduce exactamente el caso: columna inicial pending, ledger valid y ejecución disparada correctamente. No se editan revisiones históricas para aparentar que fueron validadas.

El reviewer de AppUI se dejó en revisión 5 validada y habilitada, con `auto_merge=false` y schedule manual desactivado. El panel deja de atribuir `pull_request_number_missing` al runtime o prometerle reintento automático. También prioriza la necesidad de validación sobre una bandera enabled histórica.

## Trabajo separado pendiente

El alcance por proyecto de NexusMind con contexto transversal y los presets de sandbox componibles/multi-repo son solicitudes adicionales. Esta publicación no los presenta como implementados: quedan separados de la corrección de cuenta de publicación y del disparo del reviewer.
