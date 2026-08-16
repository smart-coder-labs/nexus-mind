# Proposal — Conector: memorias y harness de Claude Code

> **Change**: `knowledge-migration-claude-memories`
> **Status**: proposed
> **Owner**: backend + tooling
> **Date**: 2026-08-15
> **Depends on**: `knowledge-migration-core`

---

## 1. Intent

### Problema

Antes de NexusMind, cada desarrollador acumuló dos cosas distintas en su máquina, y las dos
siguen ahí:

**Conocimiento.** `~/.claude/projects/<slug>/memory/*.md` con frontmatter tipado, `MEMORY.md`
como índice, `CLAUDE.md` global y por repo, `AGENTS.md`, `.cursor/rules`. Preferencias,
convenciones, gotchas, decisiones. Nadie escribe una memoria de agente por obligación; se
escribe porque algo costó averiguarlo.

**Herramientas.** `.claude/skills/`, `.claude/agents/`, `.claude/commands/`, `.claude/hooks/`,
`.claude/output-styles/`, plugins, temas, y la configuración que los cablea
(`settings.json`, `.mcp.json`). Es el harness: el trabajo de ingeniería que hace que un agente
sea útil en *este* equipo y no en abstracto.

Las dos mitades son **privadas por accidente**: viven en un portátil, no se comparten, y se
pierden cuando esa persona cambia de proyecto. Y la segunda mitad es la que más duele perder,
porque cuesta más construirla y se re-construye peor.

NexusMind ya tiene el destino correcto para cada mitad —memories/conventions por un lado,
la librería de harness por otro— y **ninguna vía para llegar desde una máquina**.

### Success looks like

- El contenido de `~/.claude/projects/*/memory/*.md` queda propuesto como memories y
  conventions de equipo, atribuido a su cliente y proyecto.
- `CLAUDE.md` y `AGENTS.md` producen **conventions**, no memories: son reglas, no observaciones.
- **Cada skill, agent, command, hook, output-style, plugin y tema local queda propuesto como
  una harness version tipada**, con su formato correcto y su target correcto, lista para que
  otro del equipo la instale por el flujo de aprobación que ya existe.
- `settings.json` y `.mcp.json` llegan como **config review redactada**, nunca como harness
  version, y su reporte de redacción dice qué se quitó.
- Los `[[wikilinks]]` entre memorias no se pierden.
- Lo que es preferencia personal (`type: user`) **no** se convierte en convención de equipo sin
  que alguien lo diga explícitamente.
- Nada de terceros se republica como propio: los plugins descargados no se migran.
- Re-correrlo tras editar una memoria o una skill propone solo la editada.

---

## 2. Scope

### In scope

#### 2.1 Memorias locales → memories / conventions

Parseo del frontmatter (`name`, `description`, `metadata.type`) y del cuerpo, con
`MEMORY.md` como índice de descubrimiento. El tipo local es señal fuerte:

| Origen local | Destino propuesto |
|---|---|
| `metadata.type: feedback` | convention (candidata) o memory `feedback` |
| `metadata.type: project` | memory `project` |
| `metadata.type: reference` | memory `discovery` |
| `metadata.type: user` | **memory `preference`, `scope=personal`** — nunca convención de equipo por defecto |

#### 2.2 Reglas en prosa → conventions

`CLAUDE.md` (global y por repo), `AGENTS.md`, `.cursor/rules`, `.github/copilot-instructions.md`.
Son reglas que un agente debe respetar, y ese es exactamente el contrato de una convention.

#### 2.3 Harness → harness versions tipadas

**Este es el bloque que el proposal anterior despachó en un párrafo y aquí se especifica.**

El modelo de harness ya existe y admite **siete formatos** (`HarnessFormat`,
`models/types.rs:1884`). El conector mapea rutas locales a formatos, sin inventar ninguno:

| Origen local | `format` | `components[].kind` | `targets` |
|---|---|---|---|
| `.claude/agents/*.md` | `agent` | `file` (`text/markdown`) | `claude` |
| `.claude/skills/<name>/` | `skill` | `folder` con `entries` | `claude` |
| `.claude/commands/*.md` | `command` | `file` (`text/markdown`) | `claude` |
| `.claude/hooks/*.sh` y scripts referenciados desde `settings.json` | `hook` | `file` (`text/x-shellscript`), `security.executable: true` | `claude` |
| `.claude/output-styles/*.md` | `output_style` | `file` (`text/markdown`) | `claude` |
| Plugin propio (`.claude-plugin/plugin.json` + su árbol) | `claude_code_plugin` | `folder` | `claude` |
| Tema (`themes/*.json`) | `theme` | `theme_json` (`application/json`) | `claude` |
| `.cursor/rules/*`, agentes de Cursor | `agent` | `file` | `cursor` |
| `AGENTS.md` y config de Codex tratada como agente | `agent` | `file` | `codex` |

Un harness detectado produce un candidato cuyo `destination_kind='harness'` y cuyo
`destination_hint` lleva el manifiesto tipado completo (`schema_version: "1.1"`, `format`,
`targets`, `components`, `provenance`, `security`). Al commitear, el pipeline llama a
`create_harness` + `publish_harness_version`; **no** escribe SQL de harness por su cuenta.

**Cuatro reglas de validación que el conector debe cumplir antes de subir nada**, porque el
backend rechaza el manifiesto si no (`validate_typed_harness_manifest`, `types.rs:1924`):

1. **Rutas relativas siempre.** `/Users/cesar/.claude/agents/reviewer.md` → `agents/reviewer.md`.
   Las absolutas —POSIX y Windows— se rechazan, y con razón: llevan el nombre de usuario dentro.
2. **`secret_scan_status` debe ser `passed`.** El escaneo de secretos es precondición del
   staging, no un extra.
3. **El componente tiene que casar con el formato.** Un `format: theme` con un `hooks/run.sh`
   dentro se rechaza. El formato sale del directorio de origen, no de una inferencia del LLM.
4. **`targets` solo admite `claude`, `codex`, `cursor`.** `opencode` fue retirado
   (`types.rs:1933`); emitirlo invalida el manifiesto entero.

#### 2.4 Configuración con secretos → config review, no harness

`settings.json`, `settings.local.json`, `.mcp.json`, `keybindings.json` **no pueden ser una
harness version**: contienen API keys, rutas privadas y variables de entorno. Van por
`harness_config_reviews` (v50, `migrations.rs:1009`), que es la tabla construida justo para
esto y guarda `redacted_config_json` + `redaction_report_json` + `content_hash`, con
`source_tool` y estado de revisión.

De ahí se extraen dos cosas útiles sin exponer nada:
- **Hooks configurados** (`hooks` en `settings.json`) → sus scripts se proponen como harness
  `hook`, con la entrada de configuración que los invoca en el `destination_hint`.
- **Servidores MCP** (`.mcp.json`) → memory `config` que documenta qué MCPs usa el equipo,
  con los valores de credencial sustituidos por sus nombres de variable.

#### 2.5 Preservación de enlaces

Cada `[[name]]` se resuelve contra los demás candidatos del run y se registra en
`destination_hint`, para materializar aristas cuando el destino `graph` exista.

#### 2.6 Redacción

Rutas de home con nombre de usuario, tokens, connection strings, correos. Ocurre **en local,
antes del staging**: el material sensible no debe llegar siquiera a la cola de revisión.

### Out of scope

- **Transcripciones de sesión** (`*.jsonl`). Volumen enorme, señal baja, y el material más
  sensible de la máquina. Si se quiere, es su propio change con su propia discusión.
- **Plugins y skills de terceros** — todo lo que cuelgue de `.claude/plugins/cache/` o venga
  de un marketplace. No son del equipo y republicarlos como propios es un problema de licencia,
  no una feature. Ver §4.
- **Instalación automática** del harness migrado en las máquinas del equipo. El flujo de
  aprobación e instalación ya existe y es deliberadamente manual.
- **Escaneo remoto** de las máquinas del equipo. Cada quien corre el conector sobre la suya.
- Memorias y configuración de agentes que no sean Claude Code, Cursor o Codex.

---

## 3. Approach

```
migrate-knowledge --source claude-memories --client acme --project acme-billing \
                  [--home ~/.claude] [--repo .] \
                  [--skip-harness | --only-harness] [--dry-run]
```

`source_identity`:

| Artefacto | Identidad |
|---|---|
| Memoria / prosa | `claude:{host_scope}:{relpath}:{content_sha}` |
| Harness de archivo único | `claude-harness:{format}:{relpath}:{content_sha}` |
| Harness de carpeta (skill, plugin) | `claude-harness:{format}:{dir}:{tree_sha}` |
| Config review | `claude-config:{source_tool}:{relpath}:{content_hash}` |

`host_scope` distingue el origen (`global` vs. el slug del proyecto) sin meter el nombre de la
máquina ni del usuario en la identidad — sería PII innecesaria en una clave primaria.

El `tree_sha` para carpetas hace que editar un archivo dentro de una skill vuelva a proponer
**esa** skill completa, que es la unidad correcta: media skill no se instala.

### Cuatro caminos, deliberadamente separados

```
~/.claude/ y el repo
   │
   ├─ memory/*.md, CLAUDE.md ──────────► memories / conventions
   │
   ├─ agents|skills|commands|hooks| ───► HARNESS VERSION tipada
   │  output-styles|plugins|themes       (create_harness + publish_harness_version)
   │
   ├─ settings.json, .mcp.json ────────► CONFIG REVIEW redactada
   │                                      (harness_config_reviews)
   │
   └─ plugins/cache/** ────────────────► IGNORADO (de terceros)
```

Mezclarlos sería el error fácil: una skill guardada como memory es un registro inútil que
nadie puede instalar; un `settings.json` guardado como harness es una fuga de credenciales.

### Rationale

- **El tipo local es señal fuerte, no dato ignorable.** Estas memorias ya vienen clasificadas
  por quien las escribió. Pedirle al LLM que reclasifique desde cero descarta información
  humana y gasta tokens.
- **`type: user` nunca asciende solo.** Una preferencia personal convertida en convención de
  equipo es cómo la manía de una persona se vuelve regla para doce.
- **El formato del harness sale de la ruta, no del LLM.** `.claude/hooks/x.sh` es un `hook`;
  no hay ambigüedad que resolver y la validación del backend rechaza el error. El LLM se ocupa
  de lo que sí requiere criterio: nombre, descripción y si vale la pena compartirlo.
- **Reusar `create_harness`/`publish_harness_version` en vez de escribir harnesses por SQL.**
  Ahí viven el hash del manifiesto, la validación tipada, el `owner_user_id` y el gate de
  aprobación. Un camino paralelo se los saltaría todos.

---

## 4. Risks & open questions

| Riesgo | Mitigación |
|---|---|
| **Republicar trabajo de terceros como propio.** `~/.claude/plugins/cache/` está lleno de skills y agents descargados de marketplaces. | Exclusión dura de `plugins/cache/**` y de cualquier árbol con marcador de marketplace, **no configurable**. Un plugin propio se reconoce por tener `.claude-plugin/plugin.json` fuera del cache. Si hay duda, no se propone. |
| **Hooks ejecutables.** Es el formato de mayor confianza: código que corre en la máquina de otro. | `security.executable: true` + `requires_approval` (ya obligatorio en el modelo); escaneo de secretos previo; el candidato muestra el script íntegro en la revisión, no un resumen. La instalación sigue siendo manual y aprobada por quien la recibe. |
| **Credenciales en `settings.json` / `.mcp.json`.** Es donde viven las API keys. | Nunca como harness version. Solo config review con `redaction_report_json`, que deja constancia de qué se quitó. Si la redacción falla, el candidato se bloquea en vez de subir. |
| **Doble gate: aprobación de migración + aprobación de instalación.** Puede leerse como redundante. | No lo es, y conviene decirlo en la UI: el gate de migración responde *"¿esto debe ser una herramienta del equipo?"*; el de instalación responde *"¿dejo que esto corra en mi máquina?"*. Son dos preguntas distintas con dos responsables distintos. |
| **Rutas absolutas y datos de máquina** en el contenido migrado. El formato local los tiene por diseño. | Reescritura determinista a rutas relativas antes de subir; el candidato muestra el antes/después. La validación del backend es la red de seguridad, no la primera línea. |
| **Memorias obsoletas.** El protocolo de memoria advierte que reflejan lo que era cierto al escribirlas. | El candidato lleva la fecha de modificación; la UI marca lo antiguo. Verificar que un archivo o flag citado aún existe queda en manos del revisor. |
| **Colisión con memorias ya existentes** guardadas por el protocolo normal. | `find_duplicate_memories` ya existe; se corre sobre los candidatos antes de la revisión y muestra los cercanos junto al candidato. |
| **Material de cliente A en la memoria local de un dev que también trabaja para B.** | `client_id` obligatorio en el run, escaneo acotado por proyecto, y ruta de origen visible en cada candidato. **No hay forma automática de detectarlo** — es responsabilidad del revisor, y así hay que decirlo en la UI en vez de fingir que el sistema lo cubre. |

### Preguntas abiertas

1. **`MEMORY.md` es derivado, no fuente.** Recomendación: ignorarlo como candidato y usarlo
   solo para descubrir archivos.
2. **Propiedad del harness migrado.** `harnesses.owner_user_id` (v49) es un usuario concreto.
   ¿El dueño es quien corrió la migración, o quien escribió originalmente la skill? Cuando la
   migración la corre un lead sobre la máquina de otro, la respuesta obvia es la equivocada.
   Recomendación: el operador del run, con el autor original en `provenance`.
3. **Versionado inicial.** `publish_harness_version` exige una `version`. ¿`0.1.0` para todo lo
   migrado, o se infiere del contenido si el archivo la declara? Recomendación: `0.1.0`
   uniforme — inventar historial de versiones que nunca existió es peor que empezar en cero.
4. **Skills que dependen de plugins de terceros.** Una skill propia que invoca herramientas de
   un plugin del marketplace se migra rota si el receptor no tiene ese plugin. Detectarlo
   automáticamente queda fuera de alcance en v1; se registra como advertencia en el candidato.
