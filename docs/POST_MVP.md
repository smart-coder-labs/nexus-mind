# NexusMind — Post-MVP Roadmap

> **Documento**: POST_MVP.md  
> **Versión**: 1.0  
> **Fecha**: Mayo 2026  
> **Fuentes**: PRD.md, ROADMAP.md, AUTH_SPEC.md, ADR-001, ADR-002, API_SPEC.md, BUSINESS_MODEL.md, 03-SCOPE_CHANGELOG.md

Este documento consolida TODO lo que los docs del producto prometen y marca honestamente qué está hecho, qué no, y en qué orden atacarlo.

---

## Estado Actual — v0.2.0 "Enterprise Demo Ready"

### ✅ Hecho

| Componente | Detalle |
|---|---|
| Backend multi-tenant | Rust + Axum 0.7 + SQLite (WAL) — orgs, users, api_keys, memories, audit_logs |
| Auth middleware | API key por usuario, scoped a org_id — sin JWT, sin sesiones |
| Memory API | `POST /v1/memory/store`, `POST /v1/memory/search` (FTS5), `GET /v1/memory`, `DELETE /v1/memory/:id` |
| Users API | List, invite (genera key), revoke, rotate key |
| Admin API | Org stats, org settings |
| Audit trail | Append-only, scoped por org — store/search/delete/invite/revoke |
| Seed binary | 3 orgs × 5 users × 20 memorias — determinístico, reproducible |
| Admin panel | Login, Dashboard, Users, Memories, AuditLog, Settings — dark mode, Apple DS |
| MCP server | TypeScript — `store_memory`, `search_memory`, `list_memories` |
| `.mcp.json` | Claude Code se conecta automáticamente desde la raíz del repo |
| `CLAUDE.md` | Instrucciones explícitas para que Claude priorice NexusMind sobre otros plugins de memoria |
| Docker Compose | Backend + admin en 2 comandos |
| CI/CD | GitHub Actions — backend (build + test + clippy), admin (build), MCP (build), E2E smoke test |
| Scripts | `reset-demo.sh`, `test-mcp.sh`, `test-e2e.sh` (multi-tenant isolation) |
| Docs | `README.md` enterprise, `docs/RUNNING.md`, `demo/MCP_DEMO.md` |

### ❌ No hecho (prometido en los docs)

Ver las secciones siguientes.

---

## v0.3.0 — Search + Integrations (Semanas 1-4 post-MVP)

**Foco**: Mejorar la búsqueda y conectar la segunda herramienta AI.

### Búsqueda Semántica (vector search)

Documentado en: PRD.md §3.1, ADR-001 §1.1 R2, ADR-002 §2.1, 03-SCOPE_CHANGELOG.md

| Tarea | Detalle |
|---|---|
| ❌ Embeddings pipeline | Candle + ONNX — modelo open (e5-small o nomic-embed) corriendo local, sin llamadas externas |
| ❌ `sqlite-vec` extension | Almacén de vectores sobre SQLite — evita segunda DB |
| ❌ Hybrid search | FTS5 (texto exacto) + vector (semántico) fusionados con Reciprocal Rank Fusion |
| ❌ `POST /v1/memory/search` upgrade | Añadir `mode: "semantic" | "keyword" | "hybrid"` al request |
| ❌ Warm-up del modelo | Cargar el modelo ONNX en startup, no en cada request |

**Estimación**: ~2 semanas. Bloqueante: elegir modelo (e5-small corre en 512MB RAM).

### Plugin Cursor

Documentado en: ROADMAP.md M2, 03-SCOPE_CHANGELOG.md, PRD.md §3.5

| Tarea | Detalle |
|---|---|
| ✅ MCP server Cursor-compatible | Mismo servidor stdio, Cursor v0.45+ lo soporta nativamente |
| ✅ `cursor_rules` context injection | Tool `get_context` — fetches memorias agrupadas por tipo, formato listo para `.cursor/rules/` o notepad |
| ✅ `docs/CURSOR_PLUGIN.md` | Setup guide + `.cursor/mcp.json` de ejemplo (npx y local binary) |
| ❌ Demo: Cursor → admin panel | Equivalente al MCP_DEMO.md para Cursor |

**Estimación**: ~1 semana. El servidor MCP ya existe — es config + docs + testing con Cursor real.

### Store Abstraction Trait (ADR-002)

Documentado en: ADR-002 §6.3

| Tarea | Detalle |
|---|---|
| ✅ `trait MemoryStore` en Rust | `store/mod.rs` — `store()`, `search()`, `list()`, `get()`, `delete()`, `validate_session()` |
| ✅ `SqliteStore` impl | `store/sqlite.rs` — queries detrás del trait, audit logging interno, expone `conn()` para handlers no-memory |
| ❌ Feature flag `postgres` | Compilación opcional con `deadpool-postgres` |
| ❌ Migration guide | SQLite → Postgres para cuando el cliente supera ~50 usuarios activos |

**Estimación**: ~1 semana. Sin esto, el path a Postgres es una reescritura.

---

## v0.4.0 — Auth Hardening (Semanas 5-8 post-MVP)

**Foco**: Lo que los enterprise buyers piden en el primer call.

Documentado en: AUTH_SPEC.md (v1.1 completo), PRD.md, ROADMAP.md M4-M5

### Roles y permisos (RBAC)

| Tarea | Detalle |
|---|---|
| ❌ Roles granulares | `admin`, `member`, `viewer` — ya existe en DB pero no se enforce en API |
| ❌ Permissions catalog | `memory:write`, `memory:read`, `memory:delete`, `user:invite`, `user:revoke`, `audit:read`, `settings:write` |
| ❌ Middleware de roles | Cada endpoint verifica el rol del API key — no solo que pertenezca a la org |
| ❌ Custom roles (YAML) | Admin puede definir roles custom con subsets de permisos — AUTH_SPEC.md §4.3 |
| ❌ Per-project role overrides | Un usuario puede ser `viewer` en la org pero `admin` en un proyecto — AUTH_SPEC.md §4.5 |

### Memory isolation levels

Documentado en: AUTH_SPEC.md §6

| Tarea | Detalle |
|---|---|
| ❌ `visibility` field en memories | `public` / `internal` / `sensitive` / `critical` / `audit_only` |
| ❌ Automatic redaction | Memories `sensitive` o `critical` no se devuelven a `viewer` — se redactan en search results |
| ❌ Admin panel: isolation badges | Mostrar nivel de visibilidad en Memory Browser |

### SSO / OIDC

| Tarea | Detalle |
|---|---|
| ❌ OIDC provider integration | Google Workspace, Okta, Azure AD — AUTH_SPEC.md §3.1 |
| ❌ SAML 2.0 | Para clientes enterprise con IdP legacy — AUTH_SPEC.md §3.2 |
| ❌ SCIM provisioning | Auto-crear/desactivar usuarios desde el IdP — AUTH_SPEC.md §3.4 |
| ❌ JIT provisioning | Primer login via SSO crea el usuario automáticamente — AUTH_SPEC.md §3.5 |
| ❌ Admin panel: SSO config page | Settings → SSO: ingresar OIDC metadata URL o SAML cert |

**Estimación**: ~3 semanas. OIDC+Google primero, SAML después. SCIM es semana extra.  
**Nota**: Sin SSO, los enterprise con >50 empleados no pueden aprobar la compra — security review lo bloquea.

### Device fingerprinting + MFA

| Tarea | Detalle |
|---|---|
| ❌ Device auth flow | `POST /v1/auth/device` — genera device token + user confirmation — AUTH_SPEC.md §4 |
| ❌ TOTP support | Para orgs que quieren 2FA sin SSO — AUTH_SPEC.md §4.2 |
| ❌ Trusted devices list | Admin ve qué devices tienen tokens activos y puede revocarlos |

---

## v0.5.0 — Policy Engine (Semanas 9-12 post-MVP)

**Foco**: El diferencial de producto que los docs prometen como P0 en el PRD.

Documentado en: PRD.md §3.2, AUTH_SPEC.md §5 (ABAC), ADR-001 §1.1 R7, 03-SCOPE_CHANGELOG.md

### Policy Engine básico

| Tarea | Detalle |
|---|---|
| ❌ `policies` table | `id`, `org_id`, `name`, `conditions` (JSON), `effect` (allow/deny), `priority` |
| ❌ Policy evaluator en Rust | Lee reglas en startup, evalúa en <1ms por request — sin Rego, lógica custom |
| ❌ Condiciones soportadas | `user.role`, `memory.tags`, `memory.project`, `tool.name`, `time.hour`, `request.action` |
| ❌ Policy API | `GET/POST/PUT/DELETE /v1/policies` — solo admin |
| ❌ Policy admin panel | Settings → Policies: crear/editar reglas con UI, preview de efecto |
| ❌ `POST /v1/memory/search` policy check | Antes de devolver resultados, filtrar por policies que aplican al caller |

### ABAC overrides

| Tarea | Detalle |
|---|---|
| ❌ Attribute-based conditions | Memory tiene atributos (`tags`, `project`, `tool`), usuario tiene atributos (`role`, `department`) |
| ❌ Policy templates | Templates predefinidos: "No dev ve memorias de producción", "Solo admin puede borrar" |
| ❌ Audit de policy decisions | Cada deny genera un audit event con la regla que lo bloqueó |

**Estimación**: ~3 semanas para policy engine funcional. ABAC completo es semana extra.  
**Cuidado**: Sin benchmarks reales de latencia, la promesa de <50μs del PRD es aspiracional.

### Audit trail inmutable

Documentado en: PRD.md §3.3, ADR-001 §1.1 R6, ADR-001 §3.2, 03-SCOPE_CHANGELOG.md

| Tarea | Detalle |
|---|---|
| ❌ Hash chain en audit_logs | Cada entry incluye `prev_hash` — SHA-256 del entry anterior — cadena verificable |
| ❌ Ed25519 signatures | Cada entry firmado con clave privada del servidor — exportable para auditoría |
| ❌ `GET /v1/audit/verify` | Verifica integridad de la cadena — devuelve OK o señala qué entry fue alterado |
| ❌ Export firmado | `GET /v1/audit/export?format=jsonl` — JSONL con firmas, importable por auditores |
| ❌ Merkle tree (opcional) | Para pruebas de inclusión eficientes — ADR-001 §3.2, solo si hay demanda real |

**Estimación**: ~1.5 semanas (hash chain + Ed25519). Merkle es adicional si alguien lo pide.

---

## v0.6.0 — Multi-Agent + Más Integraciones (Semanas 13-18 post-MVP)

**Foco**: Expandir el ecosistema de tools conectadas.

### Plugin Copilot

Documentado en: ROADMAP.md M3, PRD.md §3.5

| Tarea | Detalle |
|---|---|
| ❌ GitHub Copilot Extensions | API en beta — mismo backend, adapter diferente |
| ❌ `copilot-ext/` scaffold | Extension manifest + skill handlers |
| ❌ Context injection | Al sugerir código, buscar en NexusMind memorias relevantes |
| ❌ Demo: Copilot → admin panel | Equivalente al MCP_DEMO.md |

**Bloqueante**: GitHub Copilot Extensions API sigue en beta cerrada a mayo 2026. Requiere acceso.

### Session Capture (inspirado en Lore/Tanagram)

**Contexto**: Lore (lore.tanagram.ai) resuelve el problema de compartir sesiones de Claude Code como artefactos post-sesión (URLs compartibles con el thread completo). NexusMind lo resuelve diferente: captura el contexto *durante* la sesión y lo hace disponible para toda la herramienta en tiempo real.

La oportunidad es ofrecer un comando `/nexus` dentro de Claude Code que capture el hilo razonado de la sesión — no solo memorias sueltas sino las decisiones tomadas durante la sesión — y las guarde como un bloque estructurado de memoria.

**Diferencial frente a Lore**: Lore guarda el replay de la sesión (artefacto pasivo). NexusMind guarda el *conocimiento destilado* de la sesión como memoria activa que el equipo puede consultar desde cualquier herramienta en la próxima sesión.

| Tarea | Detalle |
|---|---|
| ❌ MCP tool `capture_session` | Captura el contexto actual de la sesión y lo guarda como memoria estructurada |
| ❌ `/nexus` skill para Claude Code | Comando que el developer ejecuta al cerrar una sesión — guarda decisiones, convenciones y hallazgos en un solo paso |
| ❌ Session summary schema | Estructura fija: `goal`, `decisions`, `conventions`, `open_questions`, `files_touched` |
| ❌ Admin panel: Sessions view | Lista de sesiones capturadas por usuario, por herramienta, por proyecto — con búsqueda |
| ❌ Cross-tool session replay | La sesión capturada desde Claude Code es consultable desde Cursor en la siguiente sesión |

**Estimación**: ~1.5 semanas. El MCP server ya existe — es añadir el tool + schema + UI.

---

### Multi-agent Orchestration

Documentado en: PRD.md §3.4 (P1), ROADMAP.md M5

| Tarea | Detalle |
|---|---|
| ❌ Agent sessions | `POST /v1/sessions` — sesión con contexto acumulado para un agent |
| ❌ Context window API | `GET /v1/context` — devuelve las N memorias más relevantes para el contexto actual |
| ❌ Agent handoff protocol | Sesión A puede transferir contexto a Sesión B — cross-agent memory |
| ❌ `list_agents` MCP tool | Lista qué agents están activos en la org y qué sesiones tienen |
| ❌ Admin panel: Agents view | Ver sesiones activas, terminarlas, ver qué memorias usaron |

### SDKs

Documentado en: PRD.md, ROADMAP.md M4, API_SPEC.md

| Tarea | Detalle |
|---|---|
| ❌ TypeScript SDK | `npm install @nexusmind/sdk` — wrapper del REST API con types generados desde el spec |
| ❌ Python SDK | `pip install nexusmind` — idem, para data scientists y automation |
| ❌ Rust SDK | `nexusmind-client` crate — para integraciones nativas en herramientas Rust |
| ❌ SDK docs | Guías de quickstart + ejemplos para cada SDK |

**Estimación**: ~1 semana por SDK (TypeScript primero, Python segundo, Rust tercero).

### CLI completa

Documentado en: ADR-001 §3.2

| Tarea | Detalle |
|---|---|
| ❌ `nexusmind-cli` binario | `cargo install nexusmind-cli` |
| ❌ `nexusmind store` | CLI equivalente al `store_memory` MCP |
| ❌ `nexusmind search <query>` | Output colorizado en terminal |
| ❌ `nexusmind list` | Tabular output, paginado |
| ❌ `nexusmind audit` | Stream del audit log en tiempo real |
| ❌ `nexusmind keygen` | Generar API key para un usuario |
| ❌ Shell completions | Bash/Zsh/Fish |

---

## v1.0.0 — Producto Enterprise Completo (Semanas 19-28 post-MVP)

**Foco**: Lo que se necesita para cerrar contratos enterprise reales.

### Billing

Documentado en: BUSINESS_MODEL.md

| Tarea | Detalle |
|---|---|
| ❌ Stripe integration | Checkout, webhooks, invoice, cancelation |
| ❌ Team plan ($49-199/mo) | Hasta 25 usuarios, 10K memorias/mes, basic analytics |
| ❌ Enterprise plan ($499+/mo) | Usuarios ilimitados, audit trail inmutable, SSO, SLA |
| ❌ Usage metering | Contar memorias almacenadas, búsquedas, usuarios activos por mes |
| ❌ Admin panel: Billing page | Plan actual, uso del mes, invoices, upgrade/downgrade |
| ❌ Quota enforcement | Hard limits por plan — 429 con mensaje claro cuando se supera |

### Compliance

Documentado en: PRD.md, RISK_ANALYSIS.md, ROADMAP.md M8-M10

| Tarea | Detalle |
|---|---|
| ❌ SOC2 Type II prep | Controles técnicos documentados, evidencia de CI/audit trail para auditores |
| ❌ GDPR: right to erasure | `DELETE /v1/users/:id/data` — borra todas las memorias de un usuario |
| ❌ Data retention policies | Config por org: auto-delete memorias >N días |
| ❌ Encryption at rest | SQLite WAL + VACUUM → SQLCipher o file-level encryption |
| ❌ Encryption in transit | TLS 1.3 forzado — actualmente el Docker Compose no termina TLS |
| ❌ HIPAA readiness | BAA, audit trail signed, encryption at rest + transit |

### Postgres migration

Documentado en: ADR-002

| Tarea | Detalle |
|---|---|
| ❌ `PostgresStore` impl | Implementa `MemoryStore` trait con `deadpool-postgres` |
| ❌ Migration script | `sqlite → postgres` — dump + importación |
| ❌ `PgVector` extension | Reemplaza `sqlite-vec` para vector search en Postgres |
| ❌ Read replicas | Para orgs con >50 usuarios concurrentes |
| ❌ Connection pooling | `pgbouncer` en el Docker Compose para production |

### On-prem / Self-hosted

Documentado en: ADR-001 §1.1 R11, ROADMAP.md M8

| Tarea | Detalle |
|---|---|
| ❌ Single binary release | `cargo build --release` → binario estático para Linux/macOS/Windows |
| ❌ ARM cross-compile | `aarch64-unknown-linux-musl` — para AWS Graviton, Apple M* servers |
| ❌ Helm chart | Para clients que usan Kubernetes |
| ❌ Air-gapped install | Sin acceso a Internet — embeddings model bundled en el binario |
| ❌ `nexusmind-server` systemd unit | Para instalar como servicio en Linux sin Docker |

---

## v2.0.0 — The Platform (Año 2)

**Foco**: Lo que ROADMAP.md llama "Phase 3 — The Platform".

Documentado en: ROADMAP.md M13-M24

| Feature | Detalle |
|---|---|
| ❌ Plugin marketplace | Developers publican plugins (tools, adapters, transformers) — monetizable |
| ❌ Analytics dashboard | Métricas de uso por tool, por usuario, por proyecto — heat maps de actividad |
| ❌ Custom Agent Builder | UI no-code para crear agents que usan NexusMind como memoria — PRD.md §3.7 |
| ❌ Non-dev agents | Agents para PM, diseñadores, compliance — no solo developers — PRD.md §3.6 |
| ❌ API v2 | Breaking changes del schema, versioned endpoints, deprecation policy |
| ❌ ISO 27001 | Certificación de seguridad para enterprise Europa |
| ❌ Private cloud | Deploy en cloud del cliente (AWS/Azure/GCP) gestionado por NexusMind |
| ❌ TUI (Ratatui) | Terminal UI completa — ADR-001 §3.2, para power users |
| ❌ Sync engine | CRDTs para sync offline → online — ADR-001 §3.2 `nexusmind-sync` |
| ❌ Multi-crate workspace | 8 crates separados (core/store/auth/audit/server/mcp/cli/tui) — ADR-001 §3.3 |

---

## Resumen de prioridades

```
v0.3 (Sem 1-4)   Vector search + Cursor plugin + Store trait
v0.4 (Sem 5-8)   RBAC + memory isolation + OIDC/SSO + MFA
v0.5 (Sem 9-12)  Policy engine + audit trail inmutable
v0.6 (Sem 13-18) Copilot + multi-agent + SDKs + CLI
v1.0 (Sem 19-28) Billing + SOC2 + Postgres + on-prem
v2.0 (Año 2)     Platform + marketplace + analytics + ISO 27001
```

---

## Decisiones técnicas pendientes

Estos no son features sino decisiones de arquitectura que bloquean múltiples features upstream:

| Decisión | Opciones | Urgencia |
|---|---|---|
| Modelo de embeddings | e5-small (local) vs OpenAI API vs Voyage AI | Antes de v0.3 |
| Auth provider library | Rust `oxide-auth` vs custom OIDC vs `axum-login` | Antes de v0.4 |
| Postgres timing | SQLite hasta qué tamaño antes de migrar | Antes de v1.0 |
| Billing provider | Stripe vs Paddle vs manual invoicing | Antes de v1.0 |
| Cloud hosting | Fly.io vs Railway vs self-hosted VPS | Antes de v1.0 |
| Compliance priority | SOC2 vs GDPR vs HIPAA — depende del primer enterprise buyer | Antes de v1.0 |

---

*Lo que NO está en este roadmap*: features que ningún doc menciona. Este roadmap es una síntesis fiel de los documentos existentes (PRD, ADRs, ROADMAP, AUTH_SPEC, SCOPE_CHANGELOG) sin inventar scope.
