# Configuración de repositorio de NexusMind

Un repositorio puede declarar uno o varios proyectos en `.nexusmind.yaml`. El archivo es configuración
pública y versionable: no debe contener tokens, credenciales, comandos ni rutas absolutas.

```yaml
version: 1
repository:
  id: commerce-monorepo
defaults:
  project: platform
  agent_profile: essential
projects:
  platform:
    project_id: prj_platform
    paths: ["docs/**", "packages/**"]
  payments:
    project_id: prj_payments
    client_id: client_acme
    paths: ["services/payments/**"]
    exclude: ["services/payments/vendor/**"]
    agent_profile: readonly
agents:
  profiles:
    essential:
      capabilities: [context.read, memory.read, task.read]
    readonly:
      extends: essential
      disable_capabilities: [task.write]
```

## Resolución

La búsqueda comienza en la ruta fuente y asciende únicamente hasta la raíz Git. `--config` permite
seleccionar otro archivo dentro del mismo repositorio y `--require-config` convierte la ausencia en
error. Los patrones son relativos a la raíz y admiten segmentos literales, `*`, `?` y `**`; no se
admiten rutas absolutas, `..`, barras invertidas, clases ni llaves.

Cuando coinciden varios proyectos gana el patrón más específico. Un empate entre proyectos es un
error; si no hay coincidencia se usa `defaults.project`, si existe. El migrador resuelve el inventario
completo antes de clasificar o escribir, agrupa por destino y crea una ejecución independiente por
proyecto. Antes del primer POST vuelve a verificar el hash exacto del archivo para evitar publicar con
una configuración modificada durante la ejecución.

```sh
migrate-knowledge --source repo-docs --path . --config .nexusmind.yaml --dry-run
migrate-knowledge --source repo-docs --path . --require-config --no-llm
```

El modo `--dry-run` muestra los grupos y bloqueos sin llamar al clasificador ni al backend. Los eventos
JSON incluyen la carga de configuración, cada grupo, los problemas de routing y los IDs creados.

## Capabilities de agentes

Los perfiles pueden heredar de otro perfil, añadir capabilities y retirar capabilities heredadas con
`disable_capabilities`. Los ciclos, referencias desconocidas y capabilities fuera del vocabulario v1
se rechazan. En el MCP los perfiles `essential` y `reduced_readonly` filtran el registro de tools antes
de exponerlo; el perfil legacy conserva su catálogo para compatibilidad.

La especificación canónica está en `schemas/nexusmind-config-v1.schema.json` y los casos compartidos en
`schemas/fixtures/nexusmind-config/v1`. Cambiar la semántica incompatible requiere una nueva versión.
