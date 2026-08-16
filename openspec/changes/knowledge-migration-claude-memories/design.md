# Design — Claude Code Connector

> **Change**: `knowledge-migration-claude-memories`
> **Status**: proposed
> **Owner**: backend + tooling
> **Date**: 2026-08-15
> **Depends on**: `knowledge-migration-core`, `knowledge-migration-repo-docs` (ambos en esta rama)

Asume leídos `proposal.md` y el delta spec.

---

## 1. Lo que el validador de manifiestos obliga

Este conector es el primero que emite harnesses, y `validate_typed_harness_manifest`
(`models/types.rs`) es estricto de formas que condicionan el diseño entero. Verificado leyendo
el validador, no asumido:

| Regla | Consecuencia para el conector |
|---|---|
| El contenido se rechaza si contiene `/users/` (minúsculas), `bearer `, `ghp_`, `nm_live`, `raw-secret` o una clave de OpenAI | **La redacción no es higiene, es precondición.** Sin ella, cualquier skill que mencione una ruta de home falla el manifiesto. |
| `size_bytes` y `sha256` deben cuadrar **exactamente** con `content` | Se calculan del contenido **ya redactado**, nunca del original. |
| Tope de **64 KB** por componente | Un asset más grande se salta con su razón, no se trunca: media skill no se instala. |
| Rutas absolutas y `..` rechazadas | Todas relativas a la raíz del asset. |
| `format` ↔ `kind` del componente debe casar | Tabla de §2. Un `agent` exige `file` acabado en `.md`; un `theme` exige `theme_json`. |
| `hook` y `claude_code_plugin` exigen `security.executable: true` | Se marca por formato, no por heurística. |
| `requires_approval` debe ser `true` | Siempre. |

**Corrección respecto al proposal**: `claude_code_plugin` usa `kind: "plugin_marketplace"` con
un `.json` cuyo contenido es un objeto — el proposal decía `folder`. Se descubrió leyendo el
validador antes de escribir código, que es donde había que descubrirlo.

---

## 2. El mapeo completo

```
~/.claude/ y el repo
   │
   ├─ projects/*/memory/*.md ─────────► memory (tipo del frontmatter)
   ├─ CLAUDE.md, AGENTS.md, .cursor/rules ─► convention
   │
   ├─ agents/*.md ────► harness format=agent          kind=file   (.md)
   ├─ skills/<n>/ ────► harness format=skill          kind=folder (entries=file)
   ├─ commands/*.md ──► harness format=command        kind=file   (.md)
   ├─ hooks/*.sh ─────► harness format=hook           kind=file   (.sh) executable
   ├─ output-styles/*.md ► harness format=output_style kind=file  (.md)
   ├─ themes/*.json ──► harness format=theme          kind=theme_json
   │
   ├─ settings.json, .mcp.json ────► harness_config_review (redactado + reporte)
   │
   ├─ plugins/cache/** ────────────► EXCLUIDO, no negociable
   └─ *.jsonl (transcripciones) ───► fuera de alcance
```

### 2.1 El tipo local es la señal, no el LLM

Estas memorias ya vienen clasificadas por quien las escribió. Pedirle al modelo que reclasifique
desde cero descarta información humana y gasta tokens en algo que ya está resuelto.

| `metadata.type` local | Destino |
|---|---|
| `feedback` | memory `feedback` |
| `project` | memory `project` |
| `reference` | memory `discovery` |
| `user` | **memory `preference`, `scope=personal`** |
| ausente | memory `discovery` |

**`user` nunca asciende solo.** Una preferencia personal convertida en convención de equipo es
cómo la manía de una persona se vuelve regla para doce. El spec lo exige como escenario.

---

## 3. La redacción, que es la pieza crítica

`redact()` corre **antes de construir el manifiesto** y devuelve `(texto, reporte)`.

| Patrón | Reemplazo | Por qué |
|---|---|---|
| `/Users/<n>/…`, `/home/<n>/…`, `C:\Users\<n>\…` | `~/…` | El validador rechaza `/users/`; y una ruta de home identifica a una persona. |
| `ghp_…`, `github_pat_…`, `nm_live_…`, `sk-…` | `<redacted:token>` | Credenciales. |
| `Bearer …` | `Bearer <redacted>` | Idem. |
| `postgres://user:pass@…`, `mysql://…` | esquema + `<redacted>` | Connection strings. |
| Correos | `<redacted:email>` | PII que no aporta al conocimiento. |

Dos decisiones:

**El reporte va con el candidato, no solo en un log.** El revisor tiene que poder ver que se
quitaron tres cosas y de qué tipo antes de aprobar. Un candidato redactado en silencio es un
candidato en el que no se puede confiar.

**Si tras redactar el escaneo de secretos del validador sigue fallando, el candidato falla.**
No se fuerza. Que el validador rechace algo que creímos limpio significa que la redacción tiene
un hueco, y taparlo con un bypass sería exactamente el error.

---

## 4. Qué se excluye y por qué no es negociable

`~/.claude/plugins/cache/**` está lleno de skills y agents descargados de marketplaces — el
propio NexusMind está ahí. Republicarlos como harnesses de u2s es un problema de licencia, no
una feature.

La exclusión **no acepta override**: `ScanOptions.includes` no puede reintroducirla, y hay un
test que lo comprueba. Un flag que permita republicar trabajo ajeno es un flag que alguien va a
usar sin darse cuenta.

Un plugin **propio** se reconoce por tener `.claude-plugin/plugin.json` **fuera** del cache.

---

## 5. La configuración va por otra puerta

`settings.json`, `settings.local.json`, `.mcp.json` y `keybindings.json` **no pueden ser harness
versions**: llevan API keys. Van a `harness_config_review`, que existe justo para eso y guarda
`redacted_config` + `redaction_report` + `content_hash`.

De ahí se extrae algo útil sin exponer nada: los servidores MCP declarados en `.mcp.json` se
proponen además como memory `config` con los valores de credencial sustituidos por el **nombre**
de su variable de entorno. Saber que el equipo usa tres MCPs es conocimiento; saber la clave no.

---

## 6. Identidad

```
claude:{host_scope}:{relpath}:{content_sha16}          # memorias y prosa
claude-harness:{format}:{relpath}:{content_sha16}      # asset de un solo archivo
claude-harness:{format}:{dir}:{tree_sha16}             # skill o plugin (carpeta)
claude-config:{source_tool}:{relpath}:{content_sha16}  # config review
```

`host_scope` es `global` o el slug del proyecto — **nunca el nombre de la máquina ni del
usuario**, que sería PII dentro de una clave primaria.

El `tree_sha` de una carpeta es el hash de los `(path, sha)` de sus archivos ordenados: editar
un archivo dentro de una skill vuelve a proponer **la skill entera**, que es la unidad correcta
porque media skill no se instala.

---

## 7. Dónde vive

`src/migration/claude_memories.rs`, junto a `repo_docs.rs`. El trait ya está en la librería
desde el change anterior, así que aquí no hay movimiento estructural: solo un `impl Connector`
más y su registro en `connector_for`.

---

## 8. Tests — orden TDD

1. `frontmatter_type_drives_the_destination` — los cinco tipos.
2. `a_memory_without_frontmatter_still_scans`.
3. `user_type_stays_personal_and_never_becomes_a_convention`.
4. `agent_instruction_files_propose_conventions`.
5. `each_asset_kind_maps_to_its_own_harness_format` — los seis.
6. `hook_manifests_are_marked_executable`.
7. **`every_emitted_manifest_passes_the_real_validator`** — el test que importa: construye un
   manifiesto por formato y lo pasa por `validate_typed_harness_manifest`. Sin él, el conector
   produce candidatos que solo fallan en commit-time.
8. `manifest_paths_are_relative_and_never_contain_a_home_directory`.
9. `redaction_removes_home_paths_tokens_and_connection_strings`.
10. `redaction_report_travels_with_the_candidate`.
11. `content_that_still_fails_the_scanner_after_redaction_fails_its_candidate`.
12. `settings_files_propose_a_config_review_not_a_harness`.
13. `mcp_config_proposes_a_memory_with_variable_names_not_values`.
14. `plugin_cache_assets_are_excluded` / `the_cache_exclusion_cannot_be_overridden`.
15. `wikilinks_are_recorded_on_the_candidate`.
16. `transcripts_are_never_scanned`.
17. `oversized_assets_are_skipped_with_a_reason` — el tope de 64 KB.

---

## 9. Riesgo que no se puede cerrar con código

**Material de un cliente A en la memoria local de alguien que también trabaja para B.** No hay
forma automática de detectarlo: el conector no sabe de qué cliente habla un párrafo.

La mitigación es el `client_id` obligatorio del run, el acotado por proyecto, y la ruta de origen
visible en cada candidato. **Y decirlo en la UI en vez de fingir que el sistema lo cubre** —
esa frase está en el proposal y sigue siendo la respuesta honesta.
