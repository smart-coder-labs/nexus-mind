# Configuración guiada del admin

**Publicado y verificado en producción**, schema **97**, backend/admin y programa del runner actualizados conservando su configuración privada. [Recibo y comprobaciones](PUBLICACION_2026-11-03.md), release `41666357163`, SHA `63125d2`. La vista previa sigue siendo ilustrativa, no una conexión productiva.

Vista previa: <http://127.0.0.1:3016/factory/workflows>. Usa datos ilustrativos, no credenciales reales. Permite recorrer formularios; el proxy rechaza todos los cambios y no conecta con producción. Los ejemplos de ejecuciones no prueban una ejecución nueva en Oracle.

Para reconstruir la vista previa, abre dos terminales en `apps/admin`:

```sh
# Terminal 1: API relativa, assets solo en loopback
VITE_API_URL='' npm run dev -- --host 127.0.0.1 --port 3015 --strictPort
# Terminal 2: fixtures y bloqueo de escrituras, puerto 3016
node dev/workflows-preview-proxy.mjs
```

La política del proxy impide conexiones fetch/WebSocket a APIs externas. No uses este modo como servidor público ni como una prueba de aceptación real.

La navegación completa se ha consolidado después de esta primera guía: consulta [Factory conectada](FACTORY_CONNECTED.md) para catálogo de perfiles, harnesses por fase, pausa compartida y compatibilidad de las rutas anteriores.

## Recorrido del panel

1. **Resumen:** muestra alcance, conexión y requisitos pendientes. Un runner conectado no implica que haya modelos, tests o QA operativos.
2. **Configuración → Repositorios:** busca entre los clones, selecciona su alcance y confirma lectura/escritura. La lista se pagina; los permisos no autorizan merges ni despliegues.
3. **Agentes y modelos:** configura imagen inmutable, conexión al proxy y red interna. Puedes reutilizar perfiles del catálogo y elegir **harness por fase**. Claude mantiene su adaptador compatible; Pi/Codex/Nexus requieren un descriptor local revisado y la autenticación/proxy del runner. Seleccionar un nombre no instala el adaptador. Los perfiles locales anteriores siguen siendo otra vía; no se convierten automáticamente a este formato.
4. **Tests y QA:** para npm elige **Node.js 22 · preparación automática**: el workflow descarga el SDK registrado y prepara dependencias del lockfile. No necesitas pegar una imagen por comprobación. [Alcance, paquetes privados y logs](ENTORNO_NODE.md). Añade comandos por repositorio para tests, build, lint/tipos y QA funcional, diseño/accesibilidad y seguridad. Indica imágenes SDK instaladas y referencias privadas de entorno. Las suites requieren tests y build para **cada repositorio modificado**; no basta una comprobación de otro repositorio.
5. **Servicios y entornos:** declara los servicios desechables, comando de inicio y healthcheck real. La red de servicios es interna. Los checks de QA verifican los servicios antes y después de ejecutarse.
6. **Publicación y límites:** selecciona un conector cifrado de publicación, presupuesto y hasta dos ciclos de reparación. No es el conector de webhooks. Crea una definición y revisa sus fases desplegables; los intentos y tiempos se editan en sus opciones técnicas.
7. **Ejecuciones:** selecciona una definición, una clave de origen única, tarea y criterios de aceptación. Puedes compartir el enlace `?run=ID`; el detalle aparece arriba, con carga, error, reintento y artefactos. La prueba de preparación no consume modelos. Una configuración guardada pero incompleta no habilita el flujo completo.
8. **GitHub:** conecta reviewers del backend, con explicación de permisos y administración temporal del hook. Un bot no puede aprobar formalmente su propio PR; la revisión interna independiente no satisface una protección de rama que exige otra identidad.
9. **Entradas:** configura filtros de Linear, canal/usuarios de Slack o instancia/proyecto de Sentry mediante conectores dedicados. Las entradas conservan las políticas y aprobaciones del workflow.
10. **Conexiones:** registra runner/carpeta, consulta su conexión y rota o revoca credenciales. La ruta es de la máquina del runner, no del navegador.

Los borradores de perfiles se conservan **solo en memoria de la página** al cambiar de sección; recargar la página los descarta. Guardar es explícito. Una revisión concurrente bloquea sobrescrituras; recarga antes de volver a editar.

## Preparación local obligatoria

El admin no introduce credenciales en Oracle ni autoriza imágenes arbitrarias. El workflow sí descarga automáticamente el SDK Node registrado por el operador y prepara dependencias verificadas; el usuario no tiene que instalarlo a mano. Al actualizar el programa del runner, incluye `runtime_settings.py` y `node_environment.py` junto a `runner.py`, `execution.py`, `delivery.py` y `container_entry.py`; conserva su archivo de configuración y estado privados. No basta con actualizar el backend.

El operador debe preparar una vez los siguientes campos en el archivo privado del runner, conservando su token, raíces y estado existentes:

```json
{
  "allow_remote_configuration": true,
  "allowed_images": ["IMAGEN_INSTALADA@sha256:DIGEST_REAL_DE_64_HEX"],
  "allowed_egress_networks": ["nexusmind-model-proxy"],
  "model_connections": {
    "proxy-kasymir": {
      "env_file": "/home/ubuntu/.config/nexusmind/model-proxy.env",
      "network": "nexusmind-model-proxy"
    }
  },
  "environments": {
    "ID_DEL_WORKSPACE": {
      "backend-dev": "/home/ubuntu/.config/nexusmind/backend-dev.env"
    }
  }
}
```

Este fragmento es una **plantilla, no una configuración ejecutable**. Instala imágenes revisadas y fijadas por digest; la sincronización no las descarga. La red del proxy debe existir y ser interna, y el proxy debe proporcionar exclusivamente acceso autorizado al modelo. El archivo del modelo solo admite `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN` para el proxy y, si el cliente lo requiere, `ANTHROPIC_API_KEY=sandbox-placeholder-not-a-credential`. No pongas una clave maestra del proveedor en ninguno de esos campos.

Los archivos privados deben ser regulares, no symlinks, tener permisos 0600 y estar fuera de las raíces de repositorios. Los entornos de aplicación contienen valores literales `KEY=value`, nunca se ejecutan con `source`. El reviewer recibe únicamente la conexión del modelo, no entornos de aplicación. Mantén los secretos de producción fuera del runner.

Los comandos se guardan como **argv**, no como un shell del host. Usa un comando por comprobación; para pasos complejos, invoca un script revisado. No incrustes tokens o contraseñas en argumentos: utiliza referencias privadas. El operador es responsable de que los comandos comprueben realmente el comportamiento esperado y que las imágenes incluyan las herramientas y dependencias necesarias.

## Versiones, evidencia y límites

- La configuración tiene control de revisión optimista. Cada ejecución fija una copia del perfil; cambios posteriores no alteran su snapshot.
- El runner informa imágenes instaladas, redes internas y nombres de conexiones/entornos; no publica archivos ni valores secretos.
- «Configurado» significa que el runner pudo aplicar ese perfil, **no que los tests pasaron** ni que se probaron modelos pagados o dispositivos móviles. La evidencia se genera en cada ejecución.
- La falta de imágenes, proxy, permisos, comandos o servicios bloquea; no se convierte en un pass ni en una omisión implícita.
- Los informes de aplicación se borran al restaurar un backup, para exigir una nueva comprobación local.
- No se han cambiado los permisos productivos ni iniciado tareas Kasymir con este rediseño. La preparación real de Oracle y la aceptación end-to-end siguen pendientes de perfiles e infraestructura revisados.

## API añadida

- `PUT /v1/factory/workspaces/:id/runtime`: requiere `factory_policy:write`; cuerpo `{ "revision": 0, "config": { "agents": [], "checks": [], "services": [] } }`. No acepta campos de secretos.
- `POST /v1/factory/runner/workspaces/:id/runtime-report`: solo admite la credencial del runner propietario. No concede acceso al control plane.
- El catálogo de workspaces incluye `runtime`, su revisión, reporte y bloqueos. Una revisión 0 mantiene la compatibilidad de perfiles locales existentes.
