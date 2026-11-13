# Entorno candidato automático

Los workflows pueden preparar la aplicación a probar sin pedir imágenes ni digests por proyecto. El SDK/harness sigue siendo una imagen reutilizable; **la aplicación se construye desde los commits de la revisión candidata**, no desde el checkout personal ni una imagen de producción.

## Admin

En **Workflows → Configuración → Servicios** deja activado **Detectar y construir el entorno desde el SHA candidato** y no añadas servicios manuales. Los servicios explícitos existentes conservan precedencia (alternativa avanzada). Guarda la configuración y espera a que el runner aplique la revisión.

Normalmente no hay que indicar archivos. Si hay varios Compose equivalentes, nombres duplicados o varios entrypoints posibles, usa el campo opcional, por ejemplo `api/compose.yaml, web/package.json`. Solo admite archivos dentro de repositorios autorizados del workspace.

## Qué se automatiza

- Descubre Compose, Dockerfile o `package.json` en cada repositorio del snapshot; los entrypoints explícitos también permiten subdirectorios.
- Compose se **interpreta**, no se ejecuta con `compose up`: ordena `depends_on`, construye las imágenes declaradas y crea una red interna por ejecución. No publica puertos del host.
- Los Dockerfiles se construyen sin red, sin secretos/build args/SSH del runner, con bases oficiales permitidas o imágenes ya autorizadas. Se fijan sus IDs antes de construir. No se permiten frontends arbitrarios, ADD remoto, ONBUILD, montajes o redes privilegiadas ni COPY desde imágenes externas. Máximo cuatro etapas.
- Para Node sin Dockerfile, o seleccionando explícitamente `repositorio/package.json`, usa el SDK administrado. Reutiliza la caché SHA-512 del lockfile y el `.npmrc` autorizado del repositorio **solo en el host** para tarballs privados; no copia credenciales al build. Instala con `npm ci --offline --ignore-scripts`, ejecuta `build` explícitamente si existe y arranca `start`/`dev`. Requiere lockfile npm v2/v3 sin paquetes workspace enlazados. Infiere puertos de Vite/Next, `--port`, `-p`, `package.config.port` o `PORT`; los flags del comando tienen precedencia, como en la aplicación. Si no es inequívoco, pide el puerto de prueba. Los builds Node mantienen límites de 8192 archivos abiertos y 1 GiB por archivo; los core dumps están desactivados.
- Crea identidades PostgreSQL desechables y deriva `DATABASE_URL`/`REDIS_URL` cuando hay una sola dependencia identificable. Los volúmenes de datos son temporales. Scripts SQL de inicialización declarados en Compose solo se montan desde el candidato, de solo lectura.
- En Node, ejecuta únicamente el script explícito `test:seed`, si está declarado, **antes de iniciar la aplicación**, con las dependencias listas y credenciales de prueba. No inventa datos de negocio ni ejecuta automáticamente un `seed`/migración genérico.
- Espera healthchecks declarados o un puerto inequívoco; comprueba que los contenedores sigan vivos durante QA. Cambiar la revisión destruye el entorno anterior.
- Conserva logs redactados de build/inicio/fixtures, decisiones pendientes, IDs de imagen y evidencia vinculada a los SHA. Al terminar/cancelar elimina contenedores, red, imágenes de aplicación y archivos privados temporales.

## Decisiones que no se adivinan

No hereda variables del shell, `.env` personal, credenciales de Docker/proxies, OAuth ni tokens del runner. Si hacen falta valores de prueba, el operador puede registrar la referencia privada **`application`** del workspace en `environments[WORKSPACE_ID]`, con un archivo `KEY=value` externo a los repositorios, propietario correcto y modo `0600`. El archivo no entra al contexto de construcción, ni al backend/modelo ni a los artefactos.

Las referencias externas, registros privados sin autorización, secretos de Compose, `env_file`, topologías especiales, YAML con aliases, rutas externas y cualquier autoridad que no pueda deducirse con seguridad quedan bloqueados con un motivo explícito. Los builds que necesitan descargas durante un Dockerfile tampoco reciben salida de red por defecto: usa el constructor Node offline o un perfil revisado para esas dependencias. No hay fallback silencioso a producción, otro harness o una configuración más permisiva.

Esto es un subconjunto deliberado de Compose, no compatibilidad universal. El modo automático se verifica con Docker; otros runtimes conservan perfiles explícitos.

## Instalación del runner (una vez)

1. Instala coordinadamente `runner.py`, `control_plane.py`, `log_stream.py`, `infra_incidents.py`, `execution.py`, `delivery.py`, `runtime_settings.py`, `application_environment.py`, `application_node.py`, `node_environment.py`, `managed_agent.py`, `model_relay.py`, `container_entry.py` y la receta SDK instalada por el operador.
2. Instala `scripts/factory/requirements.txt` en el entorno Python del runner (PyYAML fijado, SafeLoader; nunca evalúa YAML como código).
3. Autoriza `allow_remote_configuration: true` y `allow_application_builds: true` en la configuración privada del runner. Esta última permite provisionar automáticamente el SDK si no se desactivó expresamente. Conserva credenciales, allowlists y referencias existentes.
4. Ejecuta el runner en un servicio/scope systemd de cgroup v2 con **MemoryMax finito (64 MiB–8 GiB)** y **TasksMax finito (32–4096)**. Dimensiona estos límites según el proyecto; se recomienda un runner dedicado con cuotas de disco y CPU.
5. El runner anuncia `auto_environment` solo cuando su parser, SDK y límites están disponibles. Sin autorización/límites el admin muestra el bloqueo, no exige que el usuario escriba un digest.

Cada `RUN` se coloca con `--cgroup-parent` debajo del servicio/scope del runner: RAM y procesos quedan limitados en conjunto, además del timeout del paso y límites de archivo/log. **No confiamos en `--resource` de Buildx:** algunos Docker embebidos aceptan esos flags pero los ignoran. La ubicación efectiva del proceso se verificó en Oracle. Las restricciones normales de ejecución (usuario no root, FS de solo lectura, capacidades eliminadas, RAM/CPU/PIDs de contenedor) siguen aplicándose a los servicios. Los límites de filesystem/cache del daemon siguen requiriendo cuotas/retención del operador, no se presenta Docker como una VM hermética.

## Perfil de prueba de Kasymir App UI

Selecciona `kasymir-app-ui/package.json` como entrypoint explícito del workspace para probar el candidato con la receta Node aislada, no con el Dockerfile de producción que declara credenciales y configuración de despliegue. No inventes defaults para `NPM_TOKEN`, `APOLLO_KEY` ni otros secretos.

El candidato real `fc5d201696efb7b865804419c20460ec8bed5c68` compiló y sirvió `/login` con HTTP 200 en una red interna, usando el SDK existente y dependencias offline. El límite agregado de 4 GiB produjo un kill durante page-data; 6 GiB y 2 CPUs funcionaron. Para aplicar este perfil en producción hay que aprobar el despliegue del runner corregido, `MemoryMax=6G`/`CPUQuota=200%` para el runner Kasymir y el entrypoint anterior. No cambia el runner de migraciones ni las protecciones de GitHub. Este smoke de entorno no demuestra QA funcional autenticado ni un workflow completo.

## Otras correcciones de esta entrega

- **Agentes → Cuenta para publicar en GitHub** permite reutilizar el conector cifrado de publicación de Workflows en resolvers y QA. Se comprueban organización, estado, scope `github:write`, repositorio, trigger y repos de contexto. Una conexión seleccionada revocada/incorrecta falla sin recurrir a otra cuenta. El `gh` logueado sigue siendo la identidad del reviewer y de los webhooks; nunca se cambia su login ni se envía el PAT al sandbox/modelo. El modo de ejecución local inseguro conserva su advertencia y privilegios originales.
- **Pendientes** enlaza resultados y bloqueos al run exacto, incluidos históricos/archivados. **Ver detalle y evidencia** no aprueba nada. `Mark accepted/rejected` sigue siendo feedback de evaluación, no autorización de merge ni repetición del agente.

## Comprobación local

```sh
python3 -m pip install -r scripts/factory/requirements.txt
python3 -m unittest discover -s scripts/factory -p 'test_*.py'
# Pruebas reales, con imágenes SDK disponibles localmente:
systemd-run --user --scope -p MemoryMax=6G -p TasksMax=512 env \
  FACTORY_TEST_IMAGE=sha256:ID_DEL_SDK_PYTHON \
  FACTORY_NODE_TEST_IMAGE=sha256:ID_DEL_SDK_NODE \
  python3 -m unittest discover -s scripts/factory -p test_application_environment.py -v
```

Las pruebas reales construyen y levantan candidatos Dockerfile/Node, consultan su código por la red interna, ejecutan el fixture declarado y verifican que no se hereda la clave NexusMind. No llaman modelos ni despliegan aplicaciones de producción.
