# NexusMind — MVP Plan v1: "The Integrator" (Archivado)

> ⚠️ **Este plan fue reemplazado por [06-ENTERPRISE_MVP.md](./06-ENTERPRISE_MVP.md).**
> El foco cambió de "memoria cross-tool para un developer" a "control centralizado multi-usuario para vender a empresas".
> Este documento se mantiene como referencia histórica.

> **Objetivo original**: MVP que demuestre valor real en **4 semanas**.
> **Filosofía**: Un solo endpoint funcional + un plugin real > 10 docs de arquitectura.
> **Stack**: **Rust** (según ADR-001), no Go.
> **Estado actual del repo**: Fase conceptual (documentos). Cero código de backend, solo landing page Astro + bootcamp-tracker (proyecto legacy no relacionado).

---

## 1. ⚡ Filosofía MVP

El PRD promete mucho (policy engine, audit trail, multi-agent orchestration). Para un MVP real:

| ❌ No construir | ✅ Construir |
|---|---|
| Policy Engine complejo con Rego/OPA | YAML simple + validación hardcodeada |
| SDKs en 3 lenguajes | SDK solo TypeScript |
| Plugin para 5+ herramientas | Plugin solo para **Claude Code** (MCP) |
| Audit trail inmutable + hash chain | Audit trail en SQLite append-only |
| SSO (SAML/OIDC/SCIM) | API Key + JWT simple |
| Multi-agent orchestration | Ni tocarlo |
| Admin Console completa | Admin mínima con React + Tailwind |
| sqlite-vec (vector search) | FTS5 search básico |
| On-prem single binary | Docker compose inicial + `cargo build --release` |
| TUI (ratatui) | Ni tocarlo — solo CLI + REST API |
| Extensibilidad vía crates | Todo en un solo crate durante MVP |

### Principio Rector

> **Un developer usando Claude Code debe poder guardar memoria y recuperarla en otra sesión. Eso es el MVP. Todo lo demás es bonus.**

---

## 2. 🗺️ Sprint Plan (4 Semanas)

### Semana 1: Fundación — Backend Core (Rust)

**Objetivo**: API REST funcional con SQLite, auth básica, endpoints de memoria.

| Día | Tarea | Artefacto |
|---|---|---|
| 1 | Setup Rust project (cargo init, workspace, dependencias), Dockerfile, docker-compose.yml | `Cargo.toml`, `src/main.rs`, `Dockerfile`, `docker-compose.yml` |
| 2 | SQLite DB layer con FTS5 + migraciones | `src/db/` (connection, migrations, schema) |
| 3 | Auth middleware (API key + JWT) | `src/auth/` (key gen, validation, middleware) |
| 4 | REST API: `POST /v1/memory/store`, `POST /v1/memory/search`, `DELETE /v1/memory/:id` | `src/api/memory.rs`, router |
| 5 | REST API: `GET /v1/health` + rate limiting básico | `src/api/health.rs`, rate limiter |
| 6-7 | Tests + polish + documentación OpenAPI | `api/openapi.yaml`, `tests/`, integration tests |

**Definition of Done**:
- `cargo run` arranca el server
- `curl localhost:8080/v1/health` returns `{"status":"ok"}`
- `POST /v1/memory/store` con body válido → 201
- `POST /v1/memory/search` con query → resultados de FTS
- Auth con API key funciona (401 si falta)
- Docker compose levanta todo

### Semana 2: Primer Plugin — MCP Server para Claude Code

**Objetivo**: Plugin funcional que conecta Claude Code con NexusMind Memory API.

| Día | Tarea | Artefacto |
|---|---|---|
| 1 | Setup MCP Server en TypeScript | `plugins/mcp-server/package.json`, `plugins/mcp-server/src/index.ts` |
| 2 | Implementar recursos MCP: `memory/search`, `memory/store` | `plugins/mcp-server/src/resources/memory.ts` |
| 3 | Config: conexión a NexusMind API, API key | `plugins/mcp-server/src/config.ts` |
| 4 | README de instalación + prueba manual con Claude Code | `plugins/mcp-server/README.md` |
| 5-6 | Buffer memory tool: contexto auto-inyectado antes de cada prompt | `plugins/mcp-server/src/tools/buffer-context.ts` |
| 7 | Testing E2E con Claude Code real | Test report |

**Definition of Done**:
- Claude Code se conecta al MCP server
- `/nexusmind-store "El proyecto usa Rust 1.85 con Axum"` → se guarda en NexusMind
- `/nexusmind-search "¿qué stack?"` → retorna resultados cross-session
- README con instrucciones de instalación

### Semana 3: Memoria Cross-Tool + Admin Mínima

**Objetivo**: Que la memoria persista entre sesiones + admin web para ver qué hay.

| Día | Tarea | Artefacto |
|---|---|---|
| 1 | Memoria por proyecto (isolation básica) + tags | `src/db/queries.rs` |
| 2 | API: `GET /v1/memory?project=X` (listar + filtrar) | `src/api/memory.rs` |
| 3 | Admin UI: setup React + Vite + Tailwind | `admin/` (nuevo workspace) |
| 4 | Admin UI: vista de memorias (buscar, ver, borrar) | `admin/src/pages/Memories.tsx` |
| 5 | Admin UI: vista de audit trail simple | `admin/src/pages/AuditLog.tsx` |
| 6 | Admin UI: settings (API keys, proyectos) | `admin/src/pages/Settings.tsx` |
| 7 | Integración E2E + polish | QA pass |

**Definition of Done**:
- Memoria de sesión A de Claude Code aparece en sesión B
- Admin UI funcional en `localhost:3000`
- Tags + filtros por proyecto funcionan
- Audit trail básico registra cada operación

### Semana 4: Polish, Deploy, y Go-to-MVP

**Objetivo**: Release listo para que early adopters lo prueben.

| Día | Tarea | Artefacto |
|---|---|---|
| 1 | README general + quickstart | `README.md` |
| 2 | Script `make run` + `make build` + `make test` | `Makefile` |
| 3 | GitHub Actions CI (lint, test, build) | `.github/workflows/ci.yml` |
| 4 | Config por ENV + .env.example | `.env.example`, `src/config.rs` |
| 5 | Landing page update: sección "Try the MVP" | `apps/landing/` update |
| 6 | Bug bash + fixes | Issues cerrados |
| 7 | **MVP Release v0.1.0** | GitHub Release |

**Definition of Done**:
- `git clone && docker compose up` → todo funcionando en < 2 min
- Cualquier developer puede conectar Claude Code en < 5 min
- Documentación de API + plugin publicada
- Release tag v0.1.0 en GitHub

---

## 3. 📦 Package Structure (MVP)

```
nexus-mind/
├── Cargo.toml                     # Rust project (un solo crate en MVP)
├── Cargo.lock
├── rust-toolchain.toml            # Pin toolchain
├── deny.toml                      # cargo-deny policy
├── src/
│   ├── main.rs                    # Entry point: CLI dispatcher
│   ├── config.rs                  # ENV-based config con clap
│   ├── lib.rs                     # Re-export de módulos
│   ├── auth/
│   │   ├── mod.rs
│   │   └── api_keys.rs            # API key generation + validation
│   ├── db/
│   │   ├── mod.rs
│   │   ├── connection.rs          # SQLite conexión + WAL mode
│   │   ├── migrations.rs          # Schema on startup
│   │   └── queries.rs             # Memory CRUD queries
│   ├── api/
│   │   ├── mod.rs
│   │   ├── router.rs              # Axum router setup
│   │   ├── middleware.rs          # Logging, CORS, rate limit, auth
│   │   ├── health.rs              # GET /v1/health
│   │   ├── memory.rs              # Memory store/search/delete handlers
│   │   └── audit.rs               # Audit log handlers
│   └── models/
│       ├── mod.rs
│       └── memory.rs              # Memory, SearchQuery, MemoryResult structs
├── plugins/
│   └── mcp-server/
│       ├── package.json
│       ├── tsconfig.json
│       ├── src/
│       │   ├── index.ts           # MCP server entry (TypeScript SDK)
│       │   ├── config.ts          # MCP config
│       │   ├── resources/
│       │   │   └── memory.ts      # MCP memory resources
│       │   └── tools/
│       │       └── buffer-context.ts  # Context injection tool
│       └── README.md
├── admin/
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   ├── tailwind.config.ts
│   ├── index.html
│   └── src/
│       ├── main.tsx
│       ├── App.tsx
│       ├── api/
│       │   └── client.ts          # NexusMind API client
│       └── pages/
│           ├── Memories.tsx
│           ├── AuditLog.tsx
│           └── Settings.tsx
├── api/
│   └── openapi.yaml               # OpenAPI 3.0 spec
├── docker-compose.yml             # Backend + Admin + MCP
├── Dockerfile                     # Multi-stage Rust build
├── Makefile                       # dev/build/test shortcuts
├── .env.example
├── .github/
│   └── workflows/
│       └── ci.yml
└── README.md                      # Quickstart + docs links
```

### Stack Técnico (basado en ADR-001 y ADR-002)

| Capa | Tecnología | Justificación |
|---|---|---|
| **Lenguaje** | Rust | ADR-001: performance determinista, sin GC, concurrencia real, borrow checker |
| **Runtime async** | Tokio | Async nativo, scheduler cooperativo |
| **HTTP/API** | Axum | Middleware typed, integración nativa con tower |
| **Serialización** | Serde + JSON | Zero-copy deserialization, estándar Rust |
| **SQLite** | Rusqlite (bundled) | Sin CGO, sin dependencias externas |
| **FTS** | FTS5 + Tantivy (post-MVP) | FTS5 built-in en SQLite para MVP, Tantivy post-MVP |
| **Auth** | API Key + JWT simple | jsonwebtoken crate |
| **CLI** | Clap | Estándar de la industria Rust |
| **Config** | ENV + clap | 12-factor app |

> **Nota sobre la divergencia con ADR-001**: El ADR propone una arquitectura modular de 8+ crates con TUI, Merkle audit trail, policy engine, etc. Para el MVP usamos **un solo crate** con la estructura plana `src/`. La división en crates se hace post-MVP cuando haya demanda. Esto reduce el tiempo de compilación de ~3 min a ~30s y mantiene el scope manejable.

### ¿Por qué Rust en vez de Go?

| Razón | Detalle |
|---|---|
| **Sin GC** | Latencia determinística para policy engine futuro (<50μs por regla) |
| **Concurrencia real sobre SQLite** | Rusqlite + Tokio permite >100 conexiones sin lock contention grave |
| **Borrow checker** | Zero bugs de memoria en auth module — crítico para seguridad |
| **Cross-compilation ARM** | `cargo build --target aarch64-unknown-linux-gnu` without CGO |
| **Single binary** | `cargo build --release` → `./target/release/nexusmind` sin librerías externas |
| **Ecosistema embedding** | Candle/ort para ONNX local en Fase 2 |

### ¿Por qué el MCP Server va en TypeScript (no en Rust)?

El ecosistema MCP de Anthropic tiene SDK oficial en TypeScript. El MCP server es un adaptador delgado que traduce protocolo MCP a REST API calls. No necesita ser rápido — es I/O bound. Hacerlo en Rust añadiría complejidad sin beneficio para el MVP.

---

## 4. 🎯 Feature Map: PRD → MVP

De las features listadas en el PRD, estas son las que entran en el MVP:

| PRD Feature | MVP Scope | Prioridad |
|---|---|---|
| **Memory System** | SQLite FTS5, store/search/delete, por proyecto + tags | **P0** ✅ |
| **Tool Integrations API** | REST API (MCP en v0.2) | **P0** ✅ |
| **Policy Engine** | **NO** — ni tocarlo. Post-MVP | P2 ❌ |
| **Audit Trail** | Log simple append-only (sin hash chain, sin Merkle) | P1 ⏳ |
| **Multi-agent Orchestration** | **NO** | P3 ❌ |
| **Admin Console** | Admin web mínima (solo ver memorias + audit) | P1 ⏳ |
| **MCP / Open-Context Plugins** | MCP server para Claude Code | P0 ✅ |
| **Enterprise Admin Console** | **NO** | P3 ❌ |
| **Non-developer Agents** | **NO** | P3 ❌ |
| **Analytics & Cost Control** | **NO** | P3 ❌ |
| **Custom Agent Builder** | **NO** | P3 ❌ |

### MVP Scope Total

```
Semana 1: Backend Core (Rust + SQLite + Axum)
Semana 2: Claude Code Plugin (MCP Server en TS)
Semana 3: Cross-tool Memory + Admin UI
Semana 4: Polish, Deploy, Release v0.1.0
```

---

## 5. ⚙️ Technical Decisions

### SQLite vs PostgreSQL

Basado en ADR-002 → SQLite para MVP, Postgres para enterprise tier.

| | SQLite | PostgreSQL |
|---|---|---|
| Setup | Archivo `.db` | Servicio aparte |
| Deploy | Sin dependencias | Conexión TCP |
| FTS5 | Built-in desde 2016 | Extension tsvector |
| Concurrencia | WAL mode | MVCC multi-writer |
| Límite | ~30 usuarios concurrentes | Ilimitado |

**Decisión**: **SQLite** para MVP. Store Abstraction Trait en Rust permite migrar a Postgres sin cambiar la app (ADR-002 §6.3).

### FTS5 vs Vector Search

| | FTS5 | Vector (sqlite-vec) |
|---|---|---|
| Exactitud | Keyword matching | Semantic similarity |
| Setup | Built-in en SQLite | Extension externa |
| Mantenimiento | Cero | Comunitario |
| Performance | <10ms | <100ms |

**Decisión**: **FTS5** para MVP. Vector search post-MVP con Candle/ONNX para embeddings locales.

### Auth: API Key Simple

Para MVP no necesitamos SSO, OIDC, ni SCIM. Una API Key por proyecto/organización es suficiente:
- Se genera en startup si no existe (`nm_*`)
- Se pasa como `Authorization: Bearer <key>`
- Se hashea con SHA-256 antes de almacenar
- Admin puede regenerarla

---

## 6. 🚨 Riesgos y Mitigaciones

| Riesgo | Probabilidad | Impacto | Mitigación |
|---|---|---|---|
| **MCP protocol cambia** | Alta (early stage) | Alto | REST como fallback, MCP es adaptador delgado |
| **Rust + Rusqlite binding bug** | Baja | Medio | Tests + version pinning, SQLite bundled |
| **Compilación lenta en CI** | Alta | Medio | `cargo-chef` + `sccache` para caching de dependencias |
| **Claude Code no soporta MCP bien** | Media | Alto | REST API independiente funcionando desde Semana 1 |
| **Developer adoption lenta** | Alta | Medio | Early adopters del Discord de Claude + comunidad Rust |
| **Scope creep** | Muy alta | Alto | **Hard cutoff: Semana 4 release o se corta** |
| **Curva de aprendizaje Rust** | Media | Medio | MVP usa un solo crate, sin async complejo, sin macros pesadas |

---

## 7. 📋 Checklist de Release (v0.1.0)

### Must-have
- [ ] `cargo build --release` compila sin errores
- [ ] `docker compose up` funciona sin errores
- [ ] POST/GET memoria funciona via curl
- [ ] Búsqueda FTS5 devuelve resultados relevantes
- [ ] Auth retorna 401 si falta API key
- [ ] MCP server se conecta a Claude Code
- [ ] Claude Code puede guardar y recuperar memoria
- [ ] README con quickstart para 2 escenarios (curl + Claude Code)
- [ ] CI pasa en GitHub Actions

### Nice-to-have
- [ ] Admin UI funcional
- [ ] Audit trail básico
- [ ] .env.example con defaults

### Excluido (post-MVP)
- [ ] Policy engine
- [ ] Vector search (sqlite-vec / Candle)
- [ ] Multi-agent orchestration
- [ ] SSO
- [ ] SDKs Python/Go
- [ ] Plugins para Cursor/Copilot
- [ ] TUI (ratatui)
- [ ] Arquitectura multi-crate
- [ ] Merkle audit trail

---

## 8. 📐 Estimación de Archivos

| Componente | Archivos | Líneas estimadas |
|---|---|---|
| Rust backend | 12-15 | ~1000-1200 |
| MCP plugin (TS) | 5-6 | ~400-500 |
| Admin UI | 8-10 | ~600-800 |
| Config/Docs | 5-6 | ~200-300 |
| CI/Infra | 4-5 | ~100-150 |
| **Total** | **~34-42** | **~2300-2950** |

---

## 9. 🧪 Cómo Probar el MVP

### Escenario 1: API pura (cualquier tool)
```bash
git clone https://github.com/smart-coder-labs/nexus-mind
cd nexus-mind
docker compose up -d

# 1. Store memory
curl -X POST http://localhost:8080/v1/memory/store \
  -H "Authorization: Bearer $NEXUSMIND_KEY" \
  -H "Content-Type: application/json" \
  -d '{"tool":"cli","project":"demo","type":"semantic","content":"El proyecto usa Rust 1.85 con Axum y SQLite","tags":["tech-stack"]}'

# 2. Search memory
curl -X POST http://localhost:8080/v1/memory/search \
  -H "Authorization: Bearer $NEXUSMIND_KEY" \
  -H "Content-Type: application/json" \
  -d '{"query":"¿qué stack?","project":"demo"}'
```

### Escenario 2: Claude Code (MCP)
```bash
# En ~/.claude/settings.json añadir:
{
  "mcpServers": {
    "nexusmind": {
      "command": "node",
      "args": ["./plugins/mcp-server/dist/index.js"],
      "env": {
        "NEXUSMIND_API_URL": "http://localhost:8080",
        "NEXUSMIND_API_KEY": "$KEY"
      }
    }
  }
}

# En Claude Code:
# > /nexusmind-store "El proyecto usa Rust 1.85 con Axum"
# > /nexusmind-search "¿qué stack?"
```

### Escenario 3: Admin UI
```
Abrir http://localhost:3000
→ Ver memorias almacenadas
→ Buscar por proyecto/tags
→ Ver audit trail
```

---

*Fin de 01-MVP_PLAN.md*
