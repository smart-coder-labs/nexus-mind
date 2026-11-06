# Guía de uso de Nexus Harness

Esta guía describe el CLI `nexus` implementado en este repositorio. El CLI abre
una interfaz de terminal (TUI) con conversación, actividad y entrada de comandos.
Desde ella ejecuta los agentes y los
comandos de terminal dentro de NVIDIA OpenShell. OpenShell es obligatorio: si
no está instalado, `nexus init` y el inicio de `nexus chat`, `nexus claude` o
`nexus codex` intentan instalarlo
con el instalador oficial. No hay ejecución local de agentes como alternativa.

## 1. Preparación inicial

En macOS, deja Docker Desktop abierto antes de crear un sandbox. Para instalar
o actualizar el ejecutable desde este checkout necesitas Rust/Cargo:

```bash
cd /RUTA/A/TU/CHECKOUT/apps/nexus-cli
cargo install --path . --locked --force
nexus --version
```

Si la terminal no encuentra `nexus`, ejecuta
`/Users/cesar/.cargo/bin/nexus` o agrega `$HOME/.cargo/bin` a tu `PATH`.
Después, entra en el repositorio Git sobre el que quieres trabajar:

```bash
cd /RUTA/A/TU/PROYECTO
nexus init --project NOMBRE_DEL_PROYECTO_EN_NEXUSMIND --project-id ID_REAL_DEL_PROYECTO
nexus status
```

También puedes iniciar Nexus desde una carpeta que agrupa varios repositorios
(por ejemplo, `/Volumes/external/Documents/kasymir`):

```bash
cd /Volumes/external/Documents/kasymir
nexus codex
```

Nexus selecciona archivos de los repositorios Git hijos directos y respeta el
`.gitignore` de cada uno. Los archivos sueltos de la carpeta agrupadora quedan
fuera del sandbox; el agente debe crear y editar dentro de un repositorio hijo.
Si prefieres trabajar con uno solo, indícalo explícitamente:

```bash
nexus --repository /Volumes/external/Documents/kasymir/kasymir-app-ui codex
```

Nexus rechaza una carpeta que no sea ni repositorio Git ni agrupadora de
repositorios hijos. Nunca sube indiscriminadamente archivos sueltos de una
carpeta sin reglas Git; tampoco sincroniza archivos creados por el agente
fuera de los repositorios seleccionados.

`nexus init` crea `.nexus/` para sesiones y configuración local, e incluye un
`.gitignore` interno para que esos datos no entren en Git. También crea
`.nexusmind.yaml` (versión 1, perfil `essential`, rutas `**`) si no existe; es
configuración pública y versionable, sin credenciales. Nunca sobrescribe un YAML
existente. `--project-id` debe ser el ID real del proyecto en NexusMind. Si se
omite, el YAML usa temporalmente el valor de `--project` como `project_id`:
revísalo antes de migrar documentación o usar rutas dependientes del ID. En un proyecto Rust,
la primera ejecución que necesite sandbox construye una imagen OpenShell con
Cargo; puede tardar varios minutos. En otros proyectos usa la imagen base, o
puedes indicar una imagen compatible con `NEXUS_OPENSHELL_IMAGE`.

## 2. Iniciar con los ejecutables, sin API keys

Para el uso normal, basta con iniciar los ejecutables ya instalados:

```bash
nexus codex
nexus claude
```

Nexus ejecuta las versiones Linux de Codex y Claude Code que trae OpenShell;
un ejecutable macOS no puede correr dentro de ese sandbox. En Codex, Nexus
reutiliza la sesión de ChatGPT del `codex` instalado en este equipo mediante
un proveedor de credenciales de OpenShell. Los tokens no se escriben en el
repositorio ni se pasan como argumentos de línea de comandos. Nexus crea un
archivo de autenticación temporal (permisos 0600) dentro del sandbox y lo borra
al acabar el turno. El bootstrap usa los marcadores opacos específicos de
OpenShell y la imagen fija Codex CLI 0.154.0: esta combinación completó un
turno real sin API key de OpenAI el 27 de septiembre de 2026. Consulta
[el informe](INFORME_VALIDACION_2026-09-27.md) para límites de la validación.
Si Codex todavía
no tiene sesión, ejecuta `codex login` una sola vez (o `/login` desde la TUI).
Puedes comprobarlo sin lanzar una tarea con `nexus auth codex`.

Claude Code está instalado, pero actualmente no tiene una sesión iniciada.
En `nexus claude`, escribe `/login`: se abre el inicio de sesión de Claude
dentro de OpenShell y eliges tu cuenta de Claude. No necesitas una API key.
Al terminar, vuelve a la TUI y escribe tu tarea. La sesión queda en ese
sandbox para los siguientes turnos. `nexus auth claude` comprueba si está lista.

OpenShell sigue siendo obligatorio para los agentes y sus comandos. Si no
está instalado, Nexus intenta instalarlo automáticamente.

## 3. JEV y NexusMind (opcionales)

Por defecto, Nexus usa JEV si ya está configurado; si no, utiliza decisiones
locales identificadas como tales. No bloquea `nexus claude` ni `nexus codex`
por falta de una API key de JEV. Si quieres activar JEV expresamente en el
futuro:

Obtén tu clave de la cuenta de TypeSafe/Jev. En la misma terminal desde la que
vas a iniciar `nexus`, configura:

```bash
export TYPESAFE_API_KEY='TU_API_KEY_DE_TYPESAFE'
nexus status
```

`JEV_API_KEY` también se acepta como alias, pero recomendamos
`TYPESAFE_API_KEY`. No pegues la clave en el chat de Nexus, en este documento,
en un archivo del repositorio ni en un commit. El comando `export` solo la deja
en el entorno de esa sesión de terminal; si abres una terminal nueva, tendrás
que configurarla otra vez o cargarla desde tu gestor de secretos. Ten en cuenta
que escribir una clave literal en un comando puede dejarla en el historial de
tu shell; puedes desactivar temporalmente el historial o usar tu gestor de
secretos para inyectar la variable.

`nexus --decision-engine jev claude` exige JEV y fallará si no está
configurado; `--decision-engine local` fuerza las reglas locales. `nexus status`
solo confirma la configuración, no valida la credencial con el servicio.

La conexión al conocimiento de NexusMind es opcional para la primera prueba.
Para activarla, indica la URL de tu servidor y una API key de NexusMind:

```bash
export NEXUSMIND_BASE_URL='https://TU_SERVIDOR_NEXUSMIND'
export NEXUSMIND_API_KEY='TU_API_KEY_DE_NEXUSMIND'
nexus status
```

Si necesitas que una falla de esa conexión detenga la tarea, usa
`export NEXUSMIND_REQUIRED=1`. Sin esa opción, el agente puede continuar sin
contexto de NexusMind y muestra el error de recuperación.

Los procesos de Claude Code y Codex reciben `NEXUSMIND_MCP_TOOL_PROFILE=essential`
para limitar las herramientas MCP al perfil esencial. La recuperación compacta
que hace Nexus antes de cada turno usa la API REST y no cambia con ese perfil.

Los runtimes directos `claude-api` y `openai-api` son opcionales y sí usan
API keys; no son necesarios para `nexus claude` ni `nexus codex`. También requieren `NEXUS_CLAUDE_MODEL` o
`NEXUS_OPENAI_MODEL` con un identificador de modelo válido para tu cuenta.
Esos runtimes llaman a la API desde el host, pero todas sus herramientas de
terminal se ejecutan en OpenShell.

## 4. Uso diario

```bash
nexus chat
nexus claude
nexus codex
nexus claude "Corrige el fallo de autenticación"
nexus codex "Corrige el fallo de autenticación"
nexus shell
nexus run "Corrige el fallo de autenticación"
nexus plan "Diseña la migración sin editar archivos"
nexus verify
nexus status
```

`nexus chat`, `nexus claude`, `nexus codex` y `nexus resume UUID` abren la TUI.
La columna izquierda muestra la conversación; la derecha, las fases de
OpenShell, los comandos que ejecuta el agente, sus salidas y la verificación.
Escribe abajo y pulsa Enter. Usa ↑/↓ para desplazar la actividad y `/exit`
para salir. Mientras un turno está activo, Nexus no permite iniciar otro ni
salir accidentalmente. `nexus chat --plain` y `nexus resume UUID --plain`
mantienen el modo de texto para terminales sin TUI.

Dentro de la TUI o del modo de texto:

| Entrada | Acción |
| --- | --- |
| texto libre | Ejecuta una tarea con el runtime actual. |
| `/plan TAREA` | Pide un plan sin edición. |
| `/runtime codex-headless` | Cambia de runtime para la sesión. |
| `/login` | Inicia sesión en Claude dentro de OpenShell o en Codex en el host. |
| `/shell` | Abre Bash con TTY real dentro de OpenShell. `exit` vuelve a Nexus. |
| `!pwd` | Ejecuta un comando puntual dentro de OpenShell. |
| `/verify` | Repite la verificación de la sesión. |
| `/status` y `/logs` | Muestra el estado y el historial de la sesión. |
| `/finish` | Registra tu aceptación manual del resultado. |
| `/exit` | Cierra Nexus. |

Para retomar una sesión, usa el UUID mostrado por Nexus:

```bash
nexus resume UUID_DE_SESION
nexus logs UUID_DE_SESION
```

El CLI ejecuta pruebas de forma automática si detecta `Cargo.toml` o
`package.json`. `NEXUS_VERIFY_COMMAND` reemplaza ese comando. Para demostrar
un requisito específico de tu tarea puedes definir `NEXUS_ACCEPTANCE_COMMAND`;
pasar pruebas genéricas no implica que Jev marque automáticamente la tarea
como completa.

En este checkout de NexusMind, la verificación predeterminada del backend usa
`cargo test` con un solo trabajo de compilación para evitar agotar la memoria
del sandbox. Omite únicamente el test que exige el historial Git real del host,
porque `.git` no se copia al sandbox por seguridad. Los demás tests se
ejecutan normalmente. Si necesitas validar también ese test, ejecútalo en un
entorno de confianza con el historial disponible.

## 5. Si algo falla

- `Jev requires TYPESAFE_API_KEY`: seleccionaste explícitamente
  `--decision-engine jev`; usa el modo automático si no deseas configurar JEV.
- `Codex has no reusable local login`: ejecuta `codex login` y vuelve a intentar.
- Claude pide autenticación: usa `/login` en `nexus claude` y completa el inicio
  de sesión de tu cuenta, no una API key.
- `OpenShell gateway is unavailable`: comprueba `openshell status`, que Docker
  Desktop esté abierto y que el gateway local esté conectado.
- Error al construir la imagen Rust: comprueba que Docker funcione y que el
  equipo pueda acceder a las fuentes de Rust. Puedes seleccionar una imagen ya
  preparada con `NEXUS_OPENSHELL_IMAGE`.
- `NexusMind context: offline` en `nexus status`: configura tanto
  `NEXUSMIND_BASE_URL` como `NEXUSMIND_API_KEY`; esa conexión no es la de Jev.

Las sesiones se guardan en `.nexus/sessions/` con permisos privados. Las claves
no deben guardarse allí. Antes de una tarea real, prueba `nexus shell` y `!pwd`
para confirmar que el shell interactivo funciona dentro de OpenShell.

Los informes y las transcripciones de benchmarks se publican en un PR
separado para que puedan revisarse sin mezclar evidencia extensa con el
ejecutable.
