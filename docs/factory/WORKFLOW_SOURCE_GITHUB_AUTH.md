# Reutilizar la autenticación GitHub existente para fuentes

## Qué / Por qué

La ejecución `6cb6c240-d717-4ea2-91a7-4283fa84fe28` quedó bloqueada en `prepare`:
la versión 80e96d8 actualizaba fuentes, pero su runner no tenía credenciales locales
para repos privados. La cuenta `gh` del backend ya estaba autenticada. Pedir otro
PAT no era necesario: faltaba integrar ese acceso con el transporte de fuentes.

Se verificaron **las 49 ramas seleccionadas con `git ls-remote`**, desde el backend,
con el mismo entorno restringido que usa su lector de credenciales. Todas legibles;
Accounting master apuntaba a `55845405ef63b11ca6e42bd14e764c3497647b32`.
No se creó ni copió ningún token, no se alteró la identidad de publicación y no se
inició una ejecución de agentes para estas comprobaciones.

## Cómo

- El runner hace fetch Git smart-HTTP contra NexusMind, no contra GitHub directamente.
- Su token de registro autentica exclusivamente al API de NexusMind; un header
  separado proporciona el lease de esa preparación.
- El backend valida runner, organización, run, fase `prepare-workspace`, lease,
  estado de fuentes pendiente y repositorio elegido del snapshot por índice.
- La cuenta existente del backend solo se comparte con la organización que ya
  tiene el permiso operativo `FACTORY_GITHUB_PUBLISH_ORG_ID`.
- Solo se sirven anuncios de `git-upload-pack` y `git-upload-pack`. No se ofrecen
  push, `git-receive-pack`, rutas arbitrarias ni acceso a repos fuera del snapshot.
- El backend usa Basic auth GitHub con su propio token. No reenvía la autorización,
  lease, cookies ni headers arbitrarios del cliente hacia GitHub. Solo conserva
  `Git-Protocol: version=2` y los encodings HTTP aprobados: Git puede comprimir las
  negociaciones grandes con gzip; el payload se transmite sin descomprimir en el backend.
- Streaming acotado: cuatro transferencias concurrentes, 256 MiB por respuesta,
  1 MiB por request y 16 requests por repo/intento; tiempo limitado y lease revisado
  durante transferencias. Sin bundles temporales que llenen el disco del servidor.
- Las credenciales GitHub no salen del backend, no están en URLs, manifiestos,
  `.git/config`, prompts ni logs. Los headers de NexusMind son efímeros en el proceso
  Git del runner y están limitados al origen/ruta de su proxy.
- El perfil local explícito sigue siendo una alternativa avanzada. Si está
  seleccionado y falla, no se cambia de identidad como fallback.

La conexión de publicación guardada sigue usándose para publicar cuando así se
seleccionó. Leer fuentes con `gh` no cambia al publicador ni al reviewer.

## Compatibilidad y reintentos

Las preparaciones nuevas requieren `workspace-git-read:v1`, además de
`workspace-sources:v1`. Desplegar backend antes del runner. La confirmación de ramas
ya realizada no necesita repetirse: no cambia el formato del inventario.

Una preparación fresca **todavía no sellada**, como la del run citado, puede
reintentarse tras actualizar el transporte. No se mueve la base de una ejecución
que ya haya sellado sus fuentes. No se reinician intentos automáticamente ni se
resetean contadores para ocultar fallos.

Se corrigió otro caso de recuperación: el journal de una preparación inicialmente
fallida crea el directorio del run; tras confirmar el resultado puede quedar vacío.
Solo ese directorio vacío se puede recrear. Si contiene archivos sin manifiesto,
se conserva todo y se exige reconciliación, sin borrar evidencias desconocidas.
El directorio vacío fue confirmado en el run real; no se modificó durante el diagnóstico.

## Comprobaciones

- Suite Python: 100 tests, 12 omitidos; prueba nueva con Git real y servidor HTTP
  local usando `git http-backend`: fetch de un commit nuevo sin PAT en el runner,
  bearer y lease correctos, original intacto, recuperación de directorio vacío y
  ausencia de secretos en manifiesto/config Git.
- Negativas existentes de URL/host/perfil, no fallback al commit viejo y conservación
  de datos desconocidos; un perfil explícito sigue funcionando.
- Backend: scope/fence/expiración/cuota/sellado y peticiones upstream sin credenciales
  NexusMind; rechazo de receive-pack y cuerpos no Git.
- `cargo check --tests` pasó en contenedor limitado a 5 GiB, sin nuevo lockfile o
  dependencia. Clippy no estaba instalado en la imagen local; CI debe validarlo.

## Estado de sesión / Próximos pasos

Código en `/tmp/nexusmind-workflow-source-auth`, rama
`fix/workflow-source-github-auth`. **Todavía no desplegado** al redactar este registro.
Completar CI y revisión, desplegar coordinadamente y verificar lectura sin copiar
credenciales. No se requiere migración de esquema ni nuevo token. No reintentar
el run real hasta que esté desplegada la corrección.

Archivos: `api/workflow_sources.rs`, `api/mod.rs`, `api/router.rs`,
`db/workflows.rs`, tests de workflow, `scripts/factory/runner.py`, `test_runner.py`
y la documentación `WORKFLOW_SOURCE_REFRESH.md`.
