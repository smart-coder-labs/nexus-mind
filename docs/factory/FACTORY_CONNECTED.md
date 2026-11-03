# Factory: operación conectada

**Publicado y verificado en producción**, schema **97**, SHA `63125d281263a6c333495ffb02c836d4e3e5fcac`. [Recibo de publicación, backups y comprobaciones](PUBLICACION_2026-11-03.md). El workflow `Factory workflows release` pasó todos sus jobs y publicó el panel después del backend. No se activan nuevas cuentas, permisos, secretos, modelos pagados ni despliegues de aplicaciones Kasymir.

## Navegación

- **Resumen:** siguiente acción, workspaces y bloqueos; no equipara conexión del runner con QA aprobado.
- **Workflows:** configuración del workspace, roles, verificaciones, servicios, presupuesto y definición de fases.
- **Agentes:** catálogo único. Las plantillas completas siguen ejecutándose como agentes individuales; un workflow reutiliza perfiles compatibles, no ejecuta un resolver completo dentro de cada fase.
- **Entradas:** un acceso con destino explícito: workflow de workspace o agente individual. Se preservan las fuentes anteriores.
- **Ejecuciones:** un acceso a ambos historiales. Los detalles y controles siguen siendo específicos de cada motor y no se eliminan registros históricos.
- **Hallazgos:** observaciones actuales de workflows enlazadas a su evidencia, junto al triage de agentes. Software/QA se separa de contenido/leads. Los informes de workflows no se presentan como tickets archivables independientes.
- **Pendientes:** incluye bloqueos nativos además de decisiones del circuito anterior. Nunca afirma que la bandeja está vacía si la consulta de alguno de los motores falta o falla. Analítica y feedback no urgente quedan en una sección opcional.
- **Configuración:** infraestructura; agentes/harnesses/modelos; GitHub/conectores; permisos/aprobaciones. Laboratorio queda enlazado desde aquí, no en el menú diario.

Las rutas anteriores de políticas, modelos, evaluaciones, decisión y agentes conservan destinos equivalentes. Los perfiles `only-context` siguen ocultando y bloqueando toda Factory.

## Catálogo, harness y modelo son decisiones diferentes

Por cada fase LLM del workflow:

1. Elegir un **rol propio** o un perfil existente habilitado y validado.
2. Elegir **harness de ejecución**: Claude Code, Pi, Codex, Nexus u otro adaptador anunciado por el runner.
3. Heredar un modelo explícito del perfil o fijarlo para esa fase.
4. Usar infraestructura común o sobrescribir imagen, conexión al modelo y red interna por fase.

Los perfiles reutilizables actuales son las plantillas de código `github_issue_resolver` y `github_pr_reviewer`. Otras plantillas permanecen disponibles como ejecuciones individuales, pero no se anuncian como compatibles con las fases de código. Tests/build/servicios ejecutan comandos verificables; no se sustituyen por una opinión de un LLM.

La proyección del catálogo copia **identidad/versionado, modelo cuando se solicita e instrucciones**. El selector consulta una proyección mínima sin el resto del config. Pi hereda el modelo fijado por su runtime del worker; Nexus no inventa uno cuando su runtime no lo declara. No copia repositorios objetivo, permisos, herramientas, schedules, conectores, credenciales ni presupuestos. El alcance y presupuesto son los del workflow. Si un perfil no tiene modelo explícito, la fase conserva su modelo visible o pide uno al cambiar de harness; no inventa una herencia de preferencias globales.

Al guardar se valida la referencia dentro de la organización y se requiere permiso de lectura del catálogo. Al iniciar se resuelve la revisión habilitada y validada vigente y se fija en el snapshot: ID, revisión, nombre, hash de configuración, modelo e instrucciones. Cambios posteriores no alteran ejecuciones activas. Deshabilitar el perfil impide nuevos inicios que lo referencien; para detener ejecuciones ya iniciadas se usa Cancelar o la pausa de Factory.

La sesión del reviewer siempre es nueva. No recibe la conversación del implementador ni entornos de aplicación. El árbol de trabajo de la revisión es de solo lectura. La revisión interna no equivale a una aprobación formal de GitHub del autor de su propio PR.

## Registro local de harnesses

El worker del backend y el runner remoto no comparten automáticamente sesiones OAuth. Elegir Pi/Codex en el admin no copia el login del backend.

El runner conserva la autoridad local: opt-in de configuración remota, imágenes instaladas por digest, redes internas y conexiones privadas. Además del adaptador Claude compatible con configuraciones anteriores, puede registrar wrappers de harness:

```json
{
  "harnesses": {
    "pi": {
      "driver": "container",
      "image": "IMAGEN_REVISADA@sha256:DIGEST_REAL_DE_64_HEX",
      "format": "json",
      "command": ["/usr/local/bin/pi-workflow-adapter", "--model", "{model}"]
    }
  }
}
```

Es una **plantilla**, no un wrapper instalado ni una prueba de Pi autenticado. El programa revisado dentro de la imagen debe aceptar el prompt por stdin y devolver el contrato JSON del adapter, incluyendo resultados y coste extraído del runtime, no inventado por el modelo. Debe aplicar el presupuesto recibido antes de llamadas pagadas (preferiblemente mediante la cuota del proxy/broker); informar el coste después no constituye un límite de gasto. La aceptación del wrapper es responsabilidad del registro local revisado, no una garantía derivada del nombre Pi/Codex. `format: claude` admite el protocolo Claude ya existente. Los otros CLIs necesitan adaptación a ese contrato; el texto JSONL bruto de cualquier CLI no se acepta como prueba por el simple hecho de seleccionar su nombre.

Los comandos se resuelven exclusivamente del registro local, no de instrucciones del issue o comandos remitidos por el admin. `{model}` se sustituye como argumento sin shell del host. No se admiten flags de reanudación de sesión en el descriptor. Un harness sin descriptor compatible, imagen instalada, conexión y red autorizadas bloquea; no cambia a otro engine como fallback.

Las conexiones de modelo admiten los campos autorizados del proxy Anthropic u OpenAI-compatible. Las URLs de proveedor público y los prefijos conocidos de claves maestras se rechazan; los archivos deben referenciar un proxy autorizado y permanecer privados fuera del código. No se montan auth stores personales dentro de contenedores. Los drivers móviles/VM siguen necesitando infraestructura y aceptación específicas.

## Agentes individuales y preferencias históricas

Los modelos globales por rol se conservan como datos históricos, pero su pasarela no es operativa. Se retira su formulario del recorrido principal.

**No se reactiva automáticamente un agente anterior que estaba bloqueado por esa pasarela.** El editor conserva su comportamiento anterior hasta que el operador autoriza explícitamente «usar el modelo y la identidad del harness de este agente». Los nuevos perfiles pueden usar esa fuente explícita. La revisión de configuración guarda `model_source: agent`; una petición de `factory_roles` no conectada sigue bloqueada. No se cambia de proveedor o cuenta como fallback.

El editor permite fijar un modelo para Claude/Codex, con validación de compatibilidad/precio cuando corresponde. Pi/Nexus del worker conservan su modelo de runtime; un campo de Claude/Codex no pretende modificarlo. Las opciones avanzadas no modeladas por el formulario —por ejemplo, planes de verificación, tiers o integraciones— se conservan al editar.

## Pausa y conservación

La pausa de organización es común a agentes y workflows:

- Requiere `autonomous_agent:enable` **y** `factory_policy:write`: controlar el worker no autoriza por sí solo cancelar workflows. La retención mantiene su permiso anterior.
- Impide nuevos inicios y claims nativos.
- Revoca las fases activas y sus leases; la autoridad antigua no vuelve al reactivar.
- Conserva cola, decisiones pendientes y evidencia; no relanza automáticamente runs cancelados.
- No deshace un efecto externo ya completado. Si había una publicación en tránsito, corresponde reconciliarla.

La retención configurada conserva su alcance anterior: runs/conversaciones/findings de agentes individuales. **No se activa una nueva eliminación de workflows o artefactos.** El formulario lo indica explícitamente para evitar pérdida de datos o promesas de retención global falsas.

## GitHub Issues y conectores

En Entradas → Workflow, GitHub Issues elige un repositorio del inventario autorizado y labels adicionales. Siempre requiere `factory`, solo toma issues abiertos y excluye pull requests. Por defecto omite issues asignados para no apropiarse de trabajo ajeno ni competir con resolvers; incluirlos requiere un opt-in visible. El contenido entra como no confiable, incluso si el autor pertenece a la organización.

El conector dedicado requiere `github:read`, `target:use`, propósito `factory_workflow_intake`, `source_kind: github` y allowlist del repositorio. El backend vuelve a comprobar alcance y credencial antes de encolar. La lectura pagina con un límite explícito; si se excede, bloquea con error, no trunca silenciosamente.

Se pueden crear conectores cifrados desde el mismo formulario. La publicación usa otro propósito y `github:write`; la selección de repositorios es explícita. Los tokens se limpian del formulario tras cada intento y no se envían a agentes. Un token creado no demuestra por sí mismo que la API remota lo haya aceptado: eso se comprueba al consultar o publicar.

La deduplicación del workflow conserva la identidad de origen dentro del workspace. No configures simultáneamente un resolver independiente y una entrada de workflow para procesar el mismo trabajo externo: esta entrega unifica interfaces y permisos, no migra ambos motores a una única cola distribuida ni reescribe sus historiales.

## Datos y despliegue

Schema 96 incorpora perfiles versionados; schema **97** amplía el CHECK de entradas para GitHub conservando filas e índice. También actualiza bases locales que ya habían aplicado la primera revisión guiada 96, sin depender de reejecutar una migración histórica. Los reportes locales y el estado de aplicación se vuelven a comprobar tras restaurar backups. Los perfiles de agentes fijados en snapshots no contienen credenciales.

El runner debe actualizarse junto al backend/admin: `runner.py`, `execution.py`, `runtime_settings.py`, `delivery.py` y `container_entry.py`, conservando configuración privada y estado. Actualizar solo la interfaz no instala un harness ni autoriza una red.

Vista previa: <http://127.0.0.1:3016/factory>. Datos ilustrativos, sin acceso productivo ni guardado; todas las operaciones mutables reciben 405. Instrucciones de arranque en [CONFIGURACION_ADMIN.md](CONFIGURACION_ADMIN.md).

Verificación local de esta entrega:

- Backend: **1861** tests de biblioteca pasados, **4 ignorados** explícitamente; **26** tests de workflows/API/backups pasados. Incluye actualización real 96 → 97 con conservación de fila, índice y claves foráneas.
- Runner: **55** tests Python pasados, con imagen Docker local fijada para los casos de contenedor.
- Admin: **125** tests pasados, TypeScript y build productivo correctos. Con Node 26 se desactiva el WebStorage experimental del proceso de tests, no el almacenamiento del navegador.
- Vista previa: **20** estados desktop/móvil, sin errores JS, peticiones externas ni overflow; enlace Pendientes → detalle de ejecución y rutas antiguas de modelos/políticas comprobados en navegador. GET 200 y PATCH 405.
- `git diff --check` sin incidencias. Estos resultados son locales; el CI y despliegue correspondientes se consultan en el workflow de publicación del SHA liberado.

Logs y capturas locales: `/tmp/nexusmind-build-tools/factory-connected-*` (`schema97.log`, `python-final.log`, `ui-confirm.log`, `visual-confirm.log`, `smoke.log` y carpeta `factory-connected-confirm/`).

La validación local comprueba código, UI, contratos, revisiones, permisos, pausa y contenedores de prueba. No constituye aceptación pagada de modelos, QA de Kasymir en vivo, autenticación de todos los harnesses o validación de dispositivos móviles.
