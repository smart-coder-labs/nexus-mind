# NexusMind — MVP Architecture

> **Documento**: 02-ARCHITECTURE_MVP.md
> **Versión**: 0.1.0
> **Fecha**: Mayo 2026
> **Propósito**: Arquitectura reducida para el MVP de 4 semanas. Derivada de ARCHITECTURE.md pero con scope recortado.

---

## 1. Diagrama de Alto Nivel (MVP)

```
                    ┌─────────────────────────┐
                    │   DEVELOPER             │
                    │   (Claude Code / curl)  │
                    └───────────┬─────────────┘
                                │
                                ▼
                    ┌─────────────────────────┐
                    │   MCP SERVER            │
                    │   (TypeScript)          │
                    │   plugins/mcp-server/   │
                    │                         │
                    │   Traduce comandos      │
                    │   MCP → REST API calls  │
                    └───────────┬─────────────┘
                                │ HTTP
                                ▼
                    ┌─────────────────────────┐
                    │   NEXUSMIND BACKEND     │
                    │   (Go)                  │
                    │   cmd/nexusmind/        │
                    │                         │
                    │   ┌─────────────────┐   │
                    │   │ Auth (API Key)   │   │
                    │   └────────┬────────┘   │
                    │            │            │
                    │   ┌────────┴────────┐   │
                    │   │ Memory API       │   │
                    │   │ store/search/del │   │
                    │   └────────┬────────┘   │
                    │            │            │
                    │   ┌────────┴────────┐   │
                    │   │ Audit Log       │   │
                    │   │ (append-only)   │   │
                    │   └────────┬────────┘   │
                    └────────────┼────────────┘
                                │
                                ▼
                    ┌─────────────────────────┐
                    │   SQLITE                │
                    │   (WAL mode + FTS5)     │
                    │                         │
                    │   Tables:               │
                    │   - memories            │
                    │   - audit_logs          │
                    │   - api_keys            │
                    └─────────────────────────┘
```

---

## 2. Componentes MVP

### 2.1 Backend (Go)

| Componente | Archivo | Responsabilidad |
|---|---|---|
| **Entry point** | `cmd/nexusmind/main.go` | Config, DB init, start server |
| **Config** | `internal/config/config.go` | ENV parsing, defaults |
| **DB Layer** | `internal/db/db.go` | SQLite connection, WAL mode |
| **Migrations** | `internal/db/migrations.go` | Schema creation on startup |
| **Queries** | `internal/db/queries.go` | CRUD para memories y audit |
| **Auth** | `internal/auth/auth.go` | API key generation + validation |
| **Auth Middleware** | `internal/auth/middleware.go` | HTTP Bearer token check |
| **Router** | `internal/api/router.go` | Chi router setup |
| **Middleware** | `internal/api/middleware.go` | Logging, CORS, rate limit |
| **Health** | `internal/api/health.go` | GET /v1/health |
| **Memory** | `internal/api/memory.go` | POST store/search, DELETE |
| **Audit** | `internal/api/audit.go` | POST log, GET query |

### 2.2 MCP Server (TypeScript)

| Componente | Archivo | Responsabilidad |
|---|---|---|
| **Entry** | `plugins/mcp-server/src/index.ts` | MCP server setup, tool registration |
| **Config** | `plugins/mcp-server/src/config.ts` | API URL, key from env |
| **Memory Resources** | `plugins/mcp-server/src/resources/memory.ts` | search/store resources |
| **Context Tools** | `plugins/mcp-server/src/tools/buffer-context.ts` | Inyecta contexto antes de prompts |

### 2.3 Admin UI (React + Vite)

| Componente | Archivo | Responsabilidad |
|---|---|---|
| **API Client** | `admin/src/api/client.ts` | Fetch wrapper para NexusMind API |
| **Memories Page** | `admin/src/pages/Memories.tsx` | Ver, buscar, borrar memorias |
| **Audit Page** | `admin/src/pages/AuditLog.tsx` | Ver audit trail |
| **Settings Page** | `admin/src/pages/Settings.tsx` | API keys, proyectos |

---

## 3. Data Model (MVP)

```sql
-- Tabla principal de memoria
CREATE TABLE memories (
    id          TEXT PRIMARY KEY,          -- mem_xxx
    project     TEXT NOT NULL,             -- aislamiento por proyecto
    tool        TEXT NOT NULL,             -- "claude-code", "cli", "cursor"
    user_id     TEXT NOT NULL DEFAULT 'anonymous',
    type        TEXT NOT NULL DEFAULT 'semantic',  -- episodic, semantic, procedural
    content     TEXT NOT NULL,             -- el contenido de la memoria
    tags        TEXT DEFAULT '[]',         -- JSON array de strings
    metadata    TEXT DEFAULT '{}',         -- JSON object
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indice FTS5 para búsqueda textual
CREATE VIRTUAL TABLE memories_fts USING fts5(
    content,
    tags,
    content='memories',
    content_rowid='rowid'
);

-- Triggers para mantener FTS sincronizado
CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, content, tags)
    VALUES (new.rowid, new.content, new.tags);
END;

-- Audit trail (append-only)
CREATE TABLE audit_logs (
    id              TEXT PRIMARY KEY,       -- aud_xxx
    timestamp       TEXT NOT NULL DEFAULT (datetime('now')),
    user_id         TEXT NOT NULL,
    tool            TEXT NOT NULL,
    action          TEXT NOT NULL,          -- "store", "search", "delete"
    resource_type   TEXT NOT NULL DEFAULT 'memory',
    resource_id     TEXT,
    metadata        TEXT DEFAULT '{}',
    ip_address      TEXT,
    api_key_id      TEXT
);

-- API keys
CREATE TABLE api_keys (
    id          TEXT PRIMARY KEY,
    key_hash    TEXT NOT NULL UNIQUE,       -- SHA-256 del key
    label       TEXT NOT NULL,              -- "default", "ci", etc.
    scopes      TEXT NOT NULL DEFAULT '["memory:read","memory:write"]',
    project     TEXT DEFAULT '*',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    revoked     INTEGER NOT NULL DEFAULT 0
);
```

---

## 4. API Endpoints (MVP)

| Método | Path | Auth | Descripción |
|---|---|---|---|
| GET | /v1/health | No | Health check |
| POST | /v1/memory/store | Sí | Guardar memoria |
| POST | /v1/memory/search | Sí | Buscar memoria |
| DELETE | /v1/memory/:id | Sí | Borrar memoria |
| GET | /v1/memory | Sí | Listar memorias (con filtros) |
| POST | /v1/audit/log | Sí | Registrar evento audit |
| GET | /v1/audit | Sí | Query audit trail |
| GET | /v1/keys | Sí | Listar API keys |
| POST | /v1/keys | Sí | Crear API key |

---

## 5. Configuración (MVP)

```env
# .env
NEXUSMIND_PORT=8080
NEXUSMIND_DB_PATH=./data/nexusmind.db
NEXUSMIND_LOG_LEVEL=info
NEXUSMIND_CORS_ORIGINS=http://localhost:3000
NEXUSMIND_RATE_LIMIT_PER_MIN=1000
```

Config defaults en `internal/config/config.go` permiten correr sin .env:
- Puerto: 8080
- DB: `./data/nexusmind.db`
- Log level: info
- CORS: `*`
- Rate limit: 1000/min

---

## 6. Diferencia con la Arquitectura Target

| Aspecto | Arquitectura Target (ARCHITECTURE.md) | MVP |
|---|---|---|
| **Auth** | SSO (SAML/OIDC) + JWT + device fingerprint | API Key simple + JWT opcional |
| **Memory** | SQLite + sqlite-vss (vectors) | SQLite + FTS5 |
| **Policy Engine** | Rego/OPA, YAML versionado, RBAC+ABAC | No existe |
| **Audit** | Append-only con hash chain, export PDF/CSV | Append-only simple |
| **MCP** | Recursos + tools completos | Solo memory/search + memory/store |
| **Admin** | Dashboard completo con analytics | CRUD básico de memorias |
| **SDKs** | Python + TypeScript + Go | Solo REST API (cualquier tool usa curl) |
| **Plugins** | Cursor, Copilot, Cline, OpenCode | Solo Claude Code (MCP) |
| **Deploy** | K8s + Helm + single binary | Docker compose |
| **Dependencias** | PostgreSQL, Redis, OPA | Solo SQLite |
| **Integraciones** | 5+ tools | 1 tool (Claude Code) |

---

## 7. Docker Compose (MVP)

```yaml
version: '3.8'
services:
  nexusmind:
    build:
      context: .
      dockerfile: Dockerfile
    ports:
      - "8080:8080"
    environment:
      - NEXUSMIND_PORT=8080
      - NEXUSMIND_DB_PATH=/data/nexusmind.db
      - NEXUSMIND_CORS_ORIGINS=http://localhost:3000
      - NEXUSMIND_LOG_LEVEL=info
    volumes:
      - nexusmind-data:/data

  admin:
    build:
      context: ./admin
      dockerfile: Dockerfile
    ports:
      - "3000:80"
    environment:
      - VITE_API_URL=http://localhost:8080
    depends_on:
      - nexusmind

volumes:
  nexusmind-data:
```

---

*Fin de 02-ARCHITECTURE_MVP.md*
