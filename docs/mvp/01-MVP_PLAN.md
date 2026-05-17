# NexusMind — MVP Plan: "The Integrator"

> **Objetivo**: MVPr que demuestre valor real en **4 semanas** (no 6 meses).
> **Filosofía**: Un solo endpoint funcional + un plugin real > 10 docs de arquitectura.
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
| sqlite-vss (vector search) | FTS5 search básico |
| On-prem single binary | Docker compose inicial |

### Principio Rector

> **Un developer usando Claude Code debe poder guardar memoria y recuperarla en otra sesión. Eso es el MVP. Todo lo demás es bonus.**

---

## 2. 🗺️ Sprint Plan (4 Semanas)

### Semana 1: Fundación — Backend Core

**Objetivo**: API REST funcional con SQLite, auth básica, endpoints de memoria.

| Día | Tarea | Artefacto |
|---|---|---|
| 1 | Setup Go project, Dockerfile, docker-compose.yml | `cmd/nexusmind/main.go`, `Dockerfile`, `docker-compose.yml` |
| 2 | SQLite DB layer con FTS5 + migraciones | `internal/db/db.go`, `internal/db/migrations.go` |
| 3 | Auth middleware (API key + JWT) | `internal/auth/auth.go`, `internal/auth/middleware.go` |
| 4 | REST API: `POST /v1/memory/store`, `POST /v1/memory/search`, `DELETE /v1/memory/:id` | `internal/api/memory.go` (router + handlers) |
| 5 | REST API: `GET /v1/health` + rate limiting básico | `internal/api/health.go`, `internal/api/middleware.go` |
| 6-7 | Tests + polish + documentación OpenAPI | `api/openapi.yaml`, tests |

**Definition of Done**:
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
- `claude-code> /memory-store "El proyecto usa Go"` → se guarda en NexusMind
- `claude-code> /memory-search "¿qué stack?"` → retorna resultados cross-session
- README con instrucciones de instalación

### Semana 3: Memoria Cross-Tool + Admin Mínima

**Objetivo**: Que la memoria persista entre sesiones + admin web para ver qué hay.

| Día | Tarea | Artefacto |
|---|---|---|
| 1 | Memoria por proyecto (isolation básica) + tags | `internal/db/queries.go` |
| 2 | API: `GET /v1/memory?project=X` (listar + filtrar) | `internal/api/memory.go` |
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
| 4 | Config por ENV + .env.example | `.env.example`, `config/config.go` |
| 5 | Landing page update: sección "Try the MVP" | `apps/landing/` update |
| 6 | Bug bash + fixes | Issues cerrados |
| 7 | **MVP Release v0.1.0** | GitHub Release |

**Definition of Done**:
- `git clone && docker compose up` → todo funcionando en < 2 min
- Cualquier developer puede conectar Claude Code en < 5 min
- Documentación de API + plugin publicada
- Release tag v0.1.0 en GitHub

---

## 3. 📦 Package Structure

```
nexus-mind/
├── cmd/
│   └── nexusmind/
│       └── main.go              # Entry point del backend
├── internal/
│   ├── auth/
│   │   ├── auth.go              # API key validation + JWT
│   │   └── middleware.go         # HTTP middleware
│   ├── db/
│   │   ├── db.go                # SQLite connection + init
│   │   ├── migrations.go        # Schema migrations
│   │   └── queries.go           # Memory CRUD queries
│   ├── api/
│   │   ├── router.go            # HTTP router setup
│   │   ├── middleware.go        # Logging, rate limit, CORS
│   │   ├── health.go            # Health endpoint
│   │   ├── memory.go            # Memory store/search/delete
│   │   └── audit.go             # Audit log endpoints
│   └── config/
│       └── config.go            # Config from ENV/file
├── plugins/
│   └── mcp-server/
│       ├── package.json
│       ├── tsconfig.json
│       ├── src/
│       │   ├── index.ts         # MCP server entry
│       │   ├── config.ts        # MCP config
│       │   ├── resources/
│       │   │   └── memory.ts    # MCP memory resources
│       │   └── tools/
│       │       └── buffer-context.ts  # Context injection
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
│       │   └── client.ts       # NexusMind API client
│       └── pages/
│           ├── Memories.tsx
│           ├── AuditLog.tsx
│           └── Settings.tsx
├── api/
│   └── openapi.yaml            # OpenAPI 3.0 spec
├── docker-compose.yml          # Backend + Admin + MCP
├── Dockerfile                  # Multi-stage Go build
├── Makefile                    # dev/build/test shortcuts
├── .env.example
├── .github/
│   └── workflows/
│       └── ci.yml
└── README.md                   # Quickstart + docs links
```

### ¿Por qué Go para el backend?

| Razón | Detalle |
|---|---|
| Single binary deploy | `go build` → `./nexusmind` listo para on-prem |
| Sin runtime dependencies | Ni Python, ni Node, ni JVM |
| Concurrencia nativa | Goroutines para manejar múltiples tools simultáneas |
| Compilación cruzada fácil | `GOOS=linux GOARCH=arm64 go build` |
| Benchmark heredado | sqlite-vss ya tiene bindings Go |

### ¿Por qué el admin no es prioridad esta semana?

Porque el valor del MVP está en la **API + plugin MCP**, no en una UI. El admin se construye solo para debugging interno y para que early adopters puedan ver el estado. No más de 3 días de esfuerzo.

---

## 4. 🎯 Feature Map: PRD → MVP

De las features listadas en el PRD, estas son las que entran en el MVP:

| PRD Feature | MVP Scope | Prioridad |
|---|---|---|
| **Memory System** | SQLite FTS5, store/search/delete, por proyecto + tags | **P0** ✅ |
| **Tool Integrations API** | Solo REST (no MCP en v0.1, MCP en v0.2) | **P0** ✅ |
| **Policy Engine** | **NO** — ni tocarlo. Post-MVP | P2 ❌ |
| **Audit Trail** | Log simple append-only (sin hash chain) | P1 ⏳ |
| **Multi-agent Orchestration** | **NO** | P3 ❌ |
| **Admin Console** | Admin web mínima (solo ver memorias + audit) | P1 ⏳ |
| **MCP / Open-Context Plugins** | MCP server para Claude Code | P0 ✅ |
| **Enterprise Admin Console** | **NO** | P3 ❌ |
| **Non-developer Agents** | **NO** | P3 ❌ |
| **Analytics & Cost Control** | **NO** | P3 ❌ |
| **Custom Agent Builder** | **NO** | P3 ❌ |

### MVP Scope Total

```
Semana 1: Backend Core (Go + SQLite)
Semana 2: Claude Code Plugin (MCP Server)
Semana 3: Cross-tool Memory + Admin UI
Semana 4: Polish, Deploy, Release
```

---

## 5. ⚙️ Technical Decisions

### SQLite vs PostgreSQL

| | SQLite | PostgreSQL |
|---|---|---|
| Setup | `apt install sqlite3` | Servicio aparte |
| Deploy | Archivo `.db` | Conexión TCP |
| FTS5 | Built-in | Extension aparte |
| Vectors | sqlite-vss (comunitario) | pgvector (maduro) |
| Concurrencia | WAL mode maneja lectura/concurrente | Excelente |

**Decisión**: **SQLite** para MVP. PostgreSQL en Fase 2 si hay demanda de concurrencia >10k requests/min.

### FTS5 vs Vector Search

| | FTS5 | Vector (sqlite-vss) |
|---|---|---|
| Exactitud | Keyword matching | Semantic similarity |
| Setup | Built-in en SQLite | Extension externa |
| Mantenimiento | Cero | Comunitario |
| Performance | <10ms | <100ms |

**Decisión**: **FTS5** para MVP. Vector search como mejora post-MVP cuando tengamos embeddings pipeline.

### Auth: API Key Simple

Para MVP no necesitamos SSO, OIDC, ni SCIM. Una API Key por proyecto/organización es suficiente:
- Se genera en startup si no existe (`nexusmind_*`)
- Se pasa como `Authorization: Bearer <key>`
- Admin puede regenerarla

---

## 6. 🚨 Riesgos y Mitigaciones

| Riesgo | Probabilidad | Impacto | Mitigación |
|---|---|---|---|
| **MCP protocol cambia** | Alta (early stage) | Alto | Usar REST como fallback, MCP es adaptador |
| **Go + SQLite bindings bug** | Media | Medio | Tests + version pinning |
| **Claude Code no soporta MCP bien** | Media | Alto | Tener REST API independiente funcionando |
| **Developer adoption lenta** | Alta | Medio | Enfocar en early adopters del Discord de Claude |
| **Scope creep** | Muy alta | Alto | **Hard cutoff: Semana 4 release o se corta** |

---

## 7. 📋 Checklist de Release (v0.1.0)

### Must-have
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
- [ ] Vector search
- [ ] Multi-agent orchestration
- [ ] SSO
- [ ] SDKs Python/Go
- [ ] Plugins para Cursor/Copilot
- [ ] On-prem single binary (Docker compose es suficiente)

---

## 8. 📐 Estimación de Archivos

| Componente | Archivos | Líneas estimadas |
|---|---|---|
| Go backend | 10-12 | ~800-1000 |
| MCP plugin | 5-6 | ~400-500 |
| Admin UI | 8-10 | ~600-800 |
| Config/Docs | 5-6 | ~200-300 |
| CI/Infra | 4-5 | ~100-150 |
| **Total** | **~32-39** | **~2100-2750** |

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
  -d '{"tool":"cli","project":"demo","type":"semantic","content":"El proyecto usa Go 1.22 con SQLite","tags":["tech-stack"]}'

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
# > /nexusmind-store "El proyecto usa Go 1.22"
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
