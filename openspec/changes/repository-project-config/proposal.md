# Proposal — Configuración local de repositorio y proyectos

> **Change**: `repository-project-config`
> **Status**: proposed
> **Owner**: backend + tooling + MCP
> **Date**: 2026-08-17
> **Depends on**: `knowledge-migration-core`, project/client hierarchy

---

## 1. Intent

### Problema

NexusMind conoce proyectos y clientes en el backend, pero un checkout local no declara a cuál
de ellos corresponde. Los agentes y las herramientas reciben hoy el proyecto por argumentos,
variables de entorno o por decisión manual del operador. Esa duplicación produce tres problemas:

1. un agente puede leer o escribir contexto en el proyecto equivocado;
2. `migrate-knowledge` exige `--client` y `--project` aunque la relación ya debería pertenecer al
   repositorio;
3. un monorepo no puede expresar de forma estable que distintas carpetas pertenecen a proyectos
   NexusMind diferentes.

La configuración de capabilities también vive separada de la identidad del proyecto. El perfil
MCP se selecciona globalmente, pero no existe una declaración versionada que permita a un repo o
a uno de sus proyectos habilitar sólo los dominios funcionales que sus agentes necesitan.

### Propuesta

Introducir un archivo versionado `.nexusmind.yaml` en la raíz del repositorio. El archivo declara:

- la identidad estable del repositorio;
- uno o más proyectos NexusMind y su cliente propietario;
- reglas repo-relativas que asignan carpetas a proyectos;
- un proyecto por defecto opcional;
- perfiles de capabilities para agentes;
- defaults seguros para consumidores como el migrator.

El archivo es metadata operativa, no un almacén de credenciales. API keys, DSNs, tokens y secretos
quedan expresamente prohibidos.

### Success looks like

- Desde cualquier archivo cubierto por el config se puede explicar de forma determinista qué
  proyecto y cliente NexusMind le corresponden y qué regla produjo el resultado.
- Un solo repo puede contener varios proyectos sin que sus documentos se migren o su contexto se
  guarde bajo un destino común accidental.
- `migrate-knowledge --path <repo>` puede resolver destinos desde el config, mostrar el routing en
  `--dry-run` y crear una corrida independiente por proyecto.
- Flags explícitos siguen permitiendo un override intencional y auditable.
- Un agente puede recibir un catálogo de herramientas reducido por capability sin convertir el
  config local en una frontera de autorización.
- Configs inválidos, rutas ambiguas y contenido sin proyecto fallan cerrados con un diagnóstico
  accionable.

---

## 2. Scope

### In scope

1. **Formato versionado** — esquema v1 de `.nexusmind.yaml`, parser compartible, validación estricta
   y errores con ubicación del campo.
2. **Descubrimiento** — búsqueda desde el path de trabajo hacia la raíz Git, sin escapar del repo;
   `--config <path>` permite una selección explícita.
3. **Routing multi-proyecto** — patrones repo-relativos, normalización de paths, regla de
   especificidad documentada y rechazo de empates ambiguos.
4. **Precedencia** — flags explícitos > proyecto resuelto por path > default del repo. La detección
   por nombre de carpeta nunca decide silenciosamente un destino.
5. **Migrator CLI** — resolución antes de publicar, agrupación de candidatos por proyecto, una
   `MigrationRun` por destino y reporte completo en dry-run.
6. **Migrator TUI** — carga del config, visualización de proyectos/rutas resueltos, warnings y
   confirmación del destino antes de ejecutar.
7. **Provenance** — hash, versión y regla aplicada del config en la attestation de cada corrida;
   nunca rutas absolutas ni contenido secreto.
8. **Capabilities de agentes** — vocabulario por dominio y operación (`memory.read`, `sdd.write`,
   `migration.run`, etc.) y selección opcional de perfil MCP. El config filtra exposición local de
   herramientas; RBAC del backend conserva toda la autoridad.
9. **Validación operativa** — comando o modo read-only que explica `path → project → client → rule`
   y permite usar el esquema en CI.
10. **Compatibilidad** — repos sin config conservan el flujo actual; los flags existentes no se
    eliminan.

### Out of scope

- Guardar secretos o sustituir `NEXUSMIND_API_KEY`, `NEXUSMIND_SOURCE_DSN` u otros secret stores.
- Crear, renombrar o archivar automáticamente proyectos/clientes del backend desde el config.
- Usar el config como mecanismo de autorización o permitir que amplíe permisos de una API key.
- Ejecutar comandos, hooks o interpolación arbitraria de variables desde YAML.
- Una sola `MigrationRun` con candidatos de proyectos diferentes. El modelo actual hace inmutable
  el proyecto de la corrida; v1 preserva esa propiedad creando una corrida por proyecto.
- Overrides locales que cambien silenciosamente el routing compartido. Si se añade un archivo
  `.nexusmind.local.yaml`, sólo podrá contener preferencias no autoritativas como URL local o
  presentación.
- Integrar en v1 todos los consumidores posibles (SDD, tasks, usage, code indexing y context). El
  contrato se diseña para ellos, pero el primer apply conecta migrator, TUI y perfil MCP.

---

## 3. Shape propuesta

La forma definitiva pertenece a `design`, pero el contrato necesita demostrar que soporta el caso
central sin mezclar identidad, routing y permisos:

```yaml
version: 1

repository:
  id: ecommerce-platform

defaults:
  project: platform
  agent_profile: essential

projects:
  platform:
    project_id: "prj-platform"
    client_id: "client-acme"
    paths: ["/"]
    exclude: ["services/payments/**", "apps/storefront/**"]

  payments:
    project_id: "prj-payments"
    client_id: "client-acme"
    paths: ["services/payments/**"]

  storefront:
    project_id: "prj-storefront"
    client_id: "client-retail"
    paths: ["apps/storefront/**"]

agents:
  profiles:
    essential:
      capabilities:
        memory.read: true
        memory.write: true
        tasks: true
        sdd: true
        migration.run: false
```

Los nombres bajo `projects` son aliases humanos. `project_id` y `client_id` son las identidades
estables de backend. El parser no debe confundir aliases con IDs ni resolver por coincidencias
aproximadas.

---

## 4. Resolución y seguridad

Para cada path, el resolver devuelve un resultado explicable:

```text
apps/storefront/README.md
  → project_id: prj-storefront
  → client_id: client-retail
  → rule: projects.storefront.paths[0]
  → source: /repo/.nexusmind.yaml (sha256: …)
```

Principios obligatorios:

- Las rutas se evalúan normalizadas y relativas al directorio del config.
- Ningún patrón puede escapar mediante `..` ni seguir una resolución fuera de la raíz Git.
- La regla más específica gana sólo cuando el orden es inequívoco; un empate es error.
- Un path sin match usa `defaults.project` únicamente si fue declarado explícitamente.
- Sin match ni default, el resultado es `unmapped`; escribir o migrar requiere decisión explícita.
- El dry-run enumera conteos por proyecto y elementos `unmapped` antes de clasificación o red.
- Un override por flags queda registrado en provenance como override, junto con el valor resuelto
  que sustituyó.
- Capabilities sólo pueden reducir la superficie local. El servidor vuelve a comprobar permisos
  en cada operación.

---

## 5. Entregas previstas

1. **Contrato y resolver** en el repo principal: tipos, parser, validación, discovery y explicación.
2. **Migrator CLI/TUI**: routing por unidad, agrupación y corridas separadas por proyecto.
3. **MCP** en `nexusmind-mcp`: lectura del mismo contrato y filtrado del catálogo/profile de
   herramientas sin alterar RBAC.
4. **Integraciones posteriores**: context bootstrap, memories, SDD/OpenSpec, tasks, code indexing y
   usage reporting consumen el resolver en lugar de pedir el proyecto repetidamente.

La tercera entrega cruza repositorios. Este change conserva el contrato canónico en `nexusmind` y
deberá definir fixtures de compatibilidad que `nexusmind-mcp` pueda ejecutar para evitar que ambos
parsers diverjan.

---

## 6. Risks & open questions

| Riesgo | Mitigación propuesta |
|---|---|
| Un config versionado apunta a IDs de otra organización o entorno. | El backend valida pertenencia al crear la corrida; el resolver local no afirma que el ID exista. Estudiar aliases por environment en design sin introducir secretos. |
| Patrones solapados enrutan conocimiento al proyecto equivocado. | Validador de ambigüedad, explicación por path y fail-closed. |
| Config y RBAC se perciben como equivalentes. | Terminología separada (`capabilities` vs `permissions`) y autorización exclusiva en backend. |
| Dos implementaciones YAML divergen entre Rust y TypeScript. | Esquema/fixtures canónicos y suite de compatibilidad cross-repo. Evaluar generar tipos desde JSON Schema. |
| El default `/` absorbe carpetas que debían declararse. | Exclusiones obligatorias cuando existe un proyecto más específico y warning CI para paths relevantes cubiertos sólo por default. |
| Una migración multi-proyecto clasifica todo antes de descubrir un error de routing. | Resolver y validar el inventario completo antes de invocar el LLM o abrir corridas. |
| Cambiar el config entre scan y publish altera el destino. | Snapshot/hash al inicio; abortar si cambia antes de publicar. |

### Preguntas para `spec` y `design`

1. ¿El nombre canónico debe ser `.nexusmind.yaml`, `nexusmind.config.yaml` o soportar ambos con uno
   recomendado? Recomendación: sólo `.nexusmind.yaml` en v1.
2. ¿Los IDs deben variar por environment (local/staging/prod)? Recomendación inicial: permitir
   aliases por environment sólo si el environment se elige explícitamente; nunca inferir producción.
3. ¿El migrator agrupa por proyecto antes o después de producir `SourceItem`? Recomendación: cada
   unidad escaneada conserva su path de origen y se enruta antes de clasificación.
4. ¿Capabilities usan booleanos jerárquicos o listas allow/deny? Recomendación: perfiles con
   `extends`, allow explícito y deny con precedencia, validados contra un vocabulario conocido.
5. ¿El contrato canónico se distribuye como JSON Schema o como paquete compartido? La frontera
   Rust/TypeScript favorece JSON Schema + fixtures, pero debe decidirse por mantenibilidad.
