# Entorno Node preparado por el workflow

En **Tests y QA → Entorno de las comprobaciones**, elige **Node.js 22 · preparación automática**. No hay que instalar Docker images o copiar digests por cada comprobación.

1. El runner anuncia un SDK inmutable aprobado localmente (`managed_images.node`).
2. Al guardar una comprobación que lo use, el workflow descarga esa imagen si falta. Solo puede descargar ese digest registrado y autorizado, nunca una imagen arbitraria indicada por un issue o un Dockerfile del repositorio.
3. Antes de ejecutar cada comprobación, lee el `package-lock.json` del commit candidato. Verifica SHA-512, descarga tarballs del registro público autorizado y usa una caché de contenido privada.
4. Instala mediante `npm ci --offline --ignore-scripts`, en un contenedor sin red. No carga el `.npmrc` del repositorio, ni credenciales, ni `NODE_OPTIONS` del entorno de aplicación durante la instalación. Las dependencias quedan en el árbol desechable, no en el clon original.
5. Ejecuta el comando real y conserva logs de preparación y resultado. El fallo de instalación es un fallo de la comprobación; nunca cuenta como test pasado.

El entorno común también actualiza comprobaciones existentes que heredaban la selección anterior o estaban vacías. Una imagen sobrescrita expresamente en una fila se conserva. El guardado identifica imagen, repositorio, comando o duplicado inválido, en lugar del error genérico de cuatro campos.

## Alcance y límites

- Node 22; npm; `package-lock.json` v2/v3. El SDK está fijado por digest, no por un tag mutable.
- Otros gestores, workspaces complejos y paquetes sin integridad necesitan un perfil explícito. No se improvisa una instalación con acceso general a Internet.
- Las dependencias privadas o externas **bloquean con un motivo concreto**. Hace falta una conexión de lectura de paquetes autorizada; no se copian tokens personales, de GitHub administrativo ni de producción.
- El lockfile de `kasymir-app-ui` incluye una dependencia en `npm.pkg.github.com`. Seleccionar Node soluciona el entorno, pero no concede acceso a ese paquete. No se ha configurado una credencial de paquetes en esta corrección.
- Los scripts de instalación se desactivan. Si el proyecto requiere generación adicional o binarios de navegador, configúralos como pasos explícitos del entorno revisado.
- Solo se siguen URLs HTTPS de `registry.npmjs.org`, sin credenciales, querystrings ni redirecciones. Máximos: 5000 entradas, 128 MiB por paquete y 2 GiB por preparación; se reserva espacio en el host. La caché instalada en contenedor tiene 1 GiB temporal y conserva el límite de memoria/CPU. Proyectos mayores necesitan un SDK específico.
- No cambia la exigencia de modelos, servicios o QA reales del workflow completo. Guardar una comprobación Node no habilita por sí solo el resto de fases.

## Política local y programa

El operador registra una vez `managed_images.node` y lo añade a `allowed_images`, preservando secretos y estado. El usuario del panel no necesita hacerlo a mano. Se necesita configuración remota habilitada y actualizar también `node_environment.py` junto con los cinco módulos existentes del runner. El reporte agrega `managed_images`; es compatible con perfiles históricos y no modifica el schema SQLite.

SDK registrado para esta corrección: `node@sha256:18a6d90fd60542269850c80ca03b29b1d9b14d6b221ac7ee8e35b0d33ef904bd`.

Prueba reproducible: `test_node_environment.py` ejecuta un contenedor real sin red, instala un paquete fixture desde tarball verificado, comprueba su importación y demuestra que no ejecutó el `preinstall` del proyecto ni modificó el clon original. El release repite esta prueba con `FACTORY_NODE_IMAGE` inmutable. No se llaman modelos pagados.
