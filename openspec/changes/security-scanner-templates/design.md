# Design — Security Scanner Templates (SAST/SCA + DAST activo)

**Change:** `security-scanner-templates`
**Project:** `nexus-mind`
**Estado:** diseño (no se ha escrito código de producción)
**Decisiones del owner:** (1) solo diseño primero · (2) target por allowlist (`web_application`) · (3) el owner controla el servidor de runtime y puede aprovisionar CLIs.

---

## D0 — Contexto y hallazgo central

Un *template* de autonomous agents **no es una tarjeta de configuración**: es un `key` que el worker
usa para elegir prompt, perfil de ejecución, evaluación y publicación. Agregar el struct a
`managed_templates()` (`apps/backend/src/api/autonomous_agents.rs:102`) solo dibuja la tarjeta de la UI;
si no hay rama de ejecución para ese `key` en `apps/backend/src/automation/worker.rs`, el run no hace nada
o falla.

Por eso esto es una **feature de backend multi-archivo**, no un template. El alcance real por fase se
detalla en D3/D4.

### Modelo de seguridad de la plataforma (invariante a respetar)

Todos los agentes actuales son **evidence-only y acotados**: leen, redactan borradores, registran findings
y **nunca** hacen merge/deploy/publish sin humano (ver el prompt del Issue Resolver: *"Do not merge,
deploy, or publish"*). Los perfiles de política (`read-only`, `implementation`, `qa-deploy`) no contemplan
"lanzar tráfico de ataque intrusivo".

- **SAST/SCA** (Fase 1) encaja dentro del invariante: es lectura de código + ejecución de escáneres sin
  efectos secundarios. No lo rompe.
- **DAST activo** (Fase 2) **rompe el invariante**: inyecta payloads reales contra un target vivo, y el
  runtime se convierte en el **origen del ataque**. Requiere perfil y capability nuevos + gating estructural
  de target. Se construye *encima* de la Fase 1, no antes.

---

## D1 — Esquema de un template (verificado contra el código)

`AutonomousAgentTemplate` (`apps/backend/src/models/types.rs:1830`):

| Campo | Tipo | Nota |
|---|---|---|
| `key` | `String` | snake_case, único; es lo que ramifica el worker |
| `version` | `i64` | |
| `name`, `description` | `String` | copy de la tarjeta |
| `capabilities` | `Vec<String>` | tags tipo `repository:read`, `finding:write`; validadas por política |
| `default_budgets` | `json` | `wall_time_seconds`, `max_attempts`, `max_cost_usd`, concurrencias |
| `config_schema` | `json` | lo que el owner llena por agente |
| `workflow` | `Vec<String>` | fases mostradas (documentales; la lógica vive en el worker) |

**Findings** (`types.rs:1919`) — a esto mapea la salida del agente:
`fingerprint`, `title`, `severity`, `status`, `summary`, `evidence(json)`, `occurrence_count`.
La evidencia técnica (request/response/payload/archivo+línea) va en `evidence`.

**Targets** (`types.rs:1964`, validación en `queries.rs:18049`): `kind ∈ {repository, web_application, project}`,
`name`, `config(json)`, `credential_connector_id?`, `enabled`. Son **por definición** y se upsert por nombre.
`web_application` es la allowlist de DAST **que ya existe** — no hay que crear tipo nuevo.

**Perfiles de ejecución** (`profiles.rs:24`): solo `read-only`, `implementation`, `qa-deploy`.
La validación de capabilities por perfil está en `policy.rs:80` (`validate_capabilities`).

**Runner de comandos allowlisted** (`worker.rs:261`, `run_allowlisted_commands`):
programa hardcodeado a `npm|npx|pnpm|yarn|bun|cargo` (`worker.rs:286`), máx 8 comandos, argv sin shell,
env restringido por `restrict_test_environment`. **Los CLIs de seguridad no están permitidos hoy.**

**Evaluación** (`worker.rs:2364`, `evaluate_structured_result`): el agente debe devolver un objeto con
`findings[]`; `>100` findings es error duro; hay un *secret canary scan* sobre la salida; exige
`context_manifest`.

---

## D2 — Estrategia: dos fases

| | Fase 1 · `security_scan` (SAST/SCA) | Fase 2 · `security_dast` (DAST activo) |
|---|---|---|
| Riesgo | Ninguno (read-only) | Alto (intrusivo, tráfico de ataque) |
| Target kind | `repository` (existente) | `web_application` (existente, allowlist) |
| Perfil | `read-only` (existente) | `active-scan` (**nuevo**) |
| Capability nueva | — | `scan:active` (**nueva**) |
| CLIs | `semgrep`, `osv-scanner`/`npm audit` | `nuclei`, `sqlmap`, ZAP daemon |
| Cambia el invariante | No | **Sí** |
| Shippable | Ya | Después de Fase 1 + barandillas |

Racional del orden: la Fase 1 valida de punta a punta el circuito `checkout → escáner → finding → evidencia
→ delivery` con blast radius cero, y deja probado el mecanismo de invocar CLIs. La Fase 2 añade solo lo
peligroso encima de barandillas ya funcionando.

---

## D3 — Fase 1: `security_scan` (SAST + SCA)

### Definición del template (para `managed_templates()`)

```
key: "security_scan"
version: 1
name: "Security Scan"
description: "Runs SAST (Semgrep) and dependency audit (SCA) over a repository
  checkout and records canonical findings. Read-only; never modifies code."
capabilities: ["repository:read", "finding:write", "delivery:write"]
default_budgets: { wall_time_seconds: 1800, max_attempts: 1, max_cost_usd: 8,
  max_definition_concurrency: 1, max_repository_concurrency: 1, max_organization_concurrency: 4 }
config_schema:
  repository:  { type: "owner/repo", required: true }
  github_auth: { const: "server_gh_cli" }
  sast:        { engine: "semgrep", ruleset: enum["auto","p/ci","p/owasp-top-ten", custom_path] }
  sca:         { enabled: bool default true, ecosystem: enum["npm","pnpm","cargo","pip","auto"] }
  outputs:     { type: array, items: ["nexusmind","slack","github_issue"] }
  custom_instructions: { type: string }   # no puede expandir alcance
workflow: ["checkout","sast_scan","sca_audit","evaluate","record_findings","deliver"]
```

### Cableado en el worker (rama por `template_key`)

1. **Prompt fijo** — nueva arma en `fixed_prompt` (`worker.rs` ~2228). El agente: clona (ya lo hace el
   host), corre los escáneres vía el runner allowlisted, parsea el JSON y emite el contrato de findings.
   Estilo idéntico al de QA/Lead: *"Only ever use the scan tools; never modify the repo."*
2. **Ejecución de escáneres** — reutilizar el patrón de `run_allowlisted_commands`, pero **extender la
   allowlist de programas** (ver D5) para permitir `semgrep` y `osv-scanner`. Alternativa más limpia: un
   runner separado `run_security_scanners()` con su propia allowlist estricta y plantillas de argv fijas,
   para no aflojar el runner de tests.
3. **Evaluación** — agregar `"security_scan"` al `matches!(template, "qa" | ...)` de
   `evaluate_structured_result` (`worker.rs:2372`) para heredar el *salvage-over-reject* y el tope de 100.
4. **Delivery/budgets/límites** — arms análogas a QA en las zonas ~3662 y ~3706-3760.

### Mapeo de salida → Finding

| Fuente | `title` | `severity` | `evidence` | `fingerprint` |
|---|---|---|---|---|
| Semgrep JSON (`results[]`) | `check_id` / mensaje | de `extra.severity` | `path`, `start.line`, snippet, `check_id` | `sha(check_id + path + line)` |
| SCA (osv/npm audit) | `CVE`/advisory | del advisory | paquete, versión, rango vulnerable, fixed_in | `sha(package + advisory)` |

Contrato de salida del agente (un solo objeto JSON, como los otros templates):
`{"summary": "...", "findings": [{"title","severity","summary","fingerprint","evidence":{...}}]}`.

### Aprovisionamiento en el runtime

`semgrep` (pip/binario) y `osv-scanner` (binario Go) en el servidor de runtime, o vía imagen Docker.
`npm audit` ya existe si hay Node. Documentar en `docs/autonomous-agents-operations.md`.

---

## D4 — Fase 2: `security_dast` (DAST activo)

### Definición del template

```
key: "security_dast"
version: 1
name: "Security DAST"
description: "Runs an authorized active security scan (Nuclei / OWASP ZAP) against a
  pre-registered web_application target and records findings with request/response evidence.
  Only scans allowlisted, owner-authorized environments."
capabilities: ["scan:active", "finding:write", "delivery:write"]
default_budgets: { wall_time_seconds: 2700, max_attempts: 1, max_cost_usd: 15,
  max_requests: 20000, requests_per_second: 20,
  max_definition_concurrency: 1, max_organization_concurrency: 2 }
config_schema:
  target_name:  { type: string, required: true }   # DEBE referenciar un web_application target enabled
  scanners:     { type: array, items: ["nuclei","zap_active"], default: ["nuclei"] }
  nuclei:       { severity: enum["info".."critical"], templates: enum["default","cves","owasp"] }
  scope:        { paths_allow: [string], paths_deny: [string] }
  require_human_confirmation: { type: bool, default: false }
  outputs:      { type: array, items: ["nexusmind","slack"] }
  custom_instructions: { type: string }
workflow: ["select_target","authorize_scope","discover","active_scan","collect_evidence","record_findings","deliver"]
```

### Barandillas estructurales (lo que hace esto defendible)

1. **Target obligatorio desde la allowlist.** El run DEBE enlazar a un `AutonomousAgentTarget`
   `kind = "web_application"`, `enabled = true`, de **esta** definición. La URL base sale de
   `target.config` — **nunca** de input libre del run. Si no hay target válido → el run falla antes de
   emitir un solo paquete. (Se apoya en `list_autonomous_agent_targets` / validación existente.)
2. **Scope guard por request.** Antes de cada petición, el host/scheme/base-path debe coincidir con el
   authorized host del target (+ `paths_allow`/`paths_deny`). Cualquier host fuera de scope se bloquea.
   Análogo funcional al `authority_recheck` del Issue Resolver.
3. **Egress restringido.** El runtime solo debe poder alcanzar los hosts autorizados (firewall/allowlist
   de egress a nivel de red). Documentado como requisito de operación.
4. **Rate-limit y budgets duros.** `requests_per_second`, `max_requests`, `wall_time_seconds` para no
   tumbar el target. Se sanea contra `default_budgets`.
5. **Registro de autorización.** Cada `web_application` target debe llevar en `config` una marca de
   autorización escrita (owner + fecha + ambiente = staging por defecto). Prod solo con flag explícito.
6. **Confirmación humana opcional** (`require_human_confirmation`) por run antes de disparar el active scan.

### Perfil y capability nuevos

- `profiles.rs` — agregar `managed_profile("active-scan", 1)` a `managed_profiles()`.
- `policy.rs:80` (`validate_capabilities`) — nueva arma:
  - `scan:active` **solo** es válido bajo el perfil `active-scan`; cualquier otro perfil que lo pida → denegado.
  - `active-scan` deniega `repository_write`, `merge`, `production_deploy`, `pr_publish` (no toca código ni
    despliega; solo escanea y registra evidencia).
- La definición debe requerir el perfil `active-scan` y que esté en la allowlist org/proyecto
  (`resolve_execution`).

### Escáneres

- **v1 de Fase 2: solo `nuclei`** — es one-shot CLI con salida JSON (`-jsonl`), encaja en el runner de
  argv. Menor complejidad operativa.
- **ZAP** es un daemon (proceso/contenedor + API REST), no un CLI one-shot → decisión aparte: levantar
  ZAP como sidecar y hablarle por API, o posponerlo. `sqlmap` queda para una iteración posterior por su
  perfil de riesgo.
- Evidencia DAST: request HTTP exacto + response que prueba la vuln + payload → `finding.evidence`.

---

## D5 — El obstáculo del allowlist de comandos (aplica a ambas fases)

`run_allowlisted_commands` (`worker.rs:286`) rechaza cualquier programa que no sea el gestor de paquetes.
Dos opciones:

| Opción | Descripción | Trade-off |
|---|---|---|
| **A. Extender la allowlist existente** | Añadir `semgrep`, `osv-scanner`, `nuclei` al `matches!` | Menos código, pero afloja el runner que también usan los tests de QA |
| **B. Runner dedicado `run_security_scanners()`** (recomendado) | Nueva función con allowlist propia y **plantillas de argv fijas** (flags cerrados; solo se inyecta target/ruleset validados) | Aísla el blast radius; el target de DAST se inyecta de forma controlada, no como argv libre |

Recomendación: **Opción B**. Mantiene el runner de tests intacto y permite que las plantillas de argv de
DAST cierren los flags peligrosos y garanticen que el `-u/-target` provenga solo del target autorizado.

---

## D6 — Superficie de cambios por archivo

| Archivo | Fase 1 | Fase 2 |
|---|---|---|
| `api/autonomous_agents.rs` (`managed_templates`) | +template `security_scan` | +template `security_dast` |
| `automation/worker.rs` | +prompt, +runner escáneres, +arm de evaluate, +delivery/budgets | +prompt DAST, +scope guard, +binding de target, +rate-limit |
| `automation/profiles.rs` | — | +perfil `active-scan` |
| `automation/policy.rs` | — | +validación de `scan:active` |
| `db/queries.rs` | (posible) mapping de findings de seguridad | validación de que el target es `web_application` enabled |
| Frontend (Templates tab) | la tarjeta sale sola del endpoint | ídem + UI de target/autorización |
| `docs/autonomous-agents-operations.md` | aprovisionar `semgrep`/`osv-scanner` | aprovisionar `nuclei`, egress firewall, ZAP sidecar |
| Tests (`tests/automation_policy.rs`, worker tests) | evaluate + escáneres | perfil/capability + scope guard |

---

## D7 — Riesgos y decisiones abiertas

1. **Aflojar el runner de comandos** (D5) es un cambio de superficie de seguridad → requiere revisión
   adversarial (arch-review/judgment-day) antes de merge, incluso en Fase 1.
2. **ZAP como daemon** no encaja en el modelo one-shot → decidir sidecar vs. posponer (propuesta: Fase 2
   arranca solo con `nuclei`).
3. **Egress del runtime**: sin firewall de egress, el scope guard es la única barrera. Recomiendo ambas.
4. **Prod vs staging**: `web_application` targets deben marcar ambiente; prod exige flag explícito +
   `require_human_confirmation`. Confirmar política.
5. **Falsos positivos SAST**: definir `status` inicial (`open`) y flujo de triage (ya existe
   `PatchAutonomousAgentFindingRequest`).
6. **Autorización legal**: el registro de autorización por target (D4.5) es requisito, no opcional.

---

## D8 — Qué NO hace este diseño

- No programa exploits desde cero (orquesta escáneres reconocidos).
- No añade CrewAI/AutoGen/LangGraph: el orquestador es el worker existente + Claude Code headless.
- No conecta MCP servers ofensivos de terceros (superficie de confianza no auditada); usa CLIs bajo
  allowlist controlada.
- No escanea nada fuera de un target autorizado y enabled de la propia definición.
