# Pendientes solicitados por el operador

## Prioridad interrumpida: guardado y aplicación de workspaces

El usuario completó formularios, pero la interfaz no comunica con claridad qué persiste y qué se aplicó. Inspección SSH del workspace Kasymir: **revisión guardada 7, aplicada 0**; el runner está activo, sin reinicios por fallo.

Estado realmente guardado:
- Cuatro roles con modelos/harnesses elegidos; imágenes, conexiones de proxy y redes vacías. Pi no está registrado como harness remoto en ese runner (su autenticación del backend es otra ruta).
- Dos comprobaciones: build de `kasymir-app-ui` y typecheck de `kasymir-accounting-service`, ambas con el SDK Node aprobado.
- Ningún servicio guardado.
- Rechazo del runner: `runtime_image_not_allowed`. Faltan además tests, QA funcional/diseño/seguridad, servicios y capacidades de los agentes.

Pendiente: investigar por qué el usuario pierde o cree guardar otros campos, proteger borradores y revisiones concurrentes, separar visualmente borrador/guardado/aplicado, y mostrar el rechazo concreto sin indicadores verdes que impliquen operatividad. No borrar ni sustituir la revisión 7 para aparentar que está aplicada.

## Agentes por proyecto de NexusMind

Permitir alcance repositorio **o proyecto**, resolver repositorios autorizados y contexto transversal del proyecto (convenios, decisiones y contexto), y revisar sus PRs sin crear un agente por repositorio. No inferir pertenencia por parecido de nombres. Existen dos proyectos llamados kasymir/Kasymir; el vínculo y el contexto deben ser inequívocos y respetar organización/ACL.

## Presets y composición de sandbox

Seleccionar tecnologías en lugar de configurar infraestructura manual en cada comprobación: Node+Playwright, PostgreSQL, MongoDB y composición compatible. Una receta se asigna a varios repositorios y puede levantar sus servicios en un entorno integrado. Reutilizar configuración/caché, no datos mutables entre ejecuciones. Runner orquesta Docker; no dar su socket o privilegios a agentes. Imágenes fijadas, redes y healthchecks reales; secretos privados requieren autorización explícita.

Node/npm automático ya está publicado, pero no equivale a esos presets completos ni a QA integrado aceptado.

## Prioridad nueva del operador

Antes de continuar lo anterior, **diagnosticar por SSH los 10–12 runs pendientes de Issue Resolver**: contar cola y activos, identificar antigüedad/objetivos, workers, leases, concurrencia y bloqueos. Solo lectura; no cancelar, reintentar ni lanzar modelos sin motivo/autorización.
