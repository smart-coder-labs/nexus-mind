# NexusMind — MVP Architecture

> **Documento**: 02-ARCHITECTURE_MVP.md
> **Versión**: 0.1.0
> **Fecha**: Mayo 2026
> **Propósito**: Arquitectura reducida para el MVP de 4 semanas. Basada en ADR-001 (Rust) y ADR-002 (SQLite).

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
                    │   (Rust + Axum)         │
                    │   src/                  │
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
                    │   (rusqlite, WAL mode)  │
                    │                         │
                    │   Tablas:               │
                    │   - memories            │
                    │   - memories_fts (FTS5) │
                    │   - audit_logs          │
                    │   - api_keys            │
                    └─────────────────────────┘
```

---

## 2. Componentes MVP

### 2.1 Backend (Rust — un solo crate)

| Componente | Archivo | Responsabilidad |
|---|---|---|
| **Entry point** | `src/main.rs` | Config, DB init, start server |
| **Config** | `src/config.rs` | ENV parsing + clap CLI args |
| **DB Layer** | `src/db/connection.rs` | Rusqlite connection, WAL mode |
| **Migrations** | `src/db/migrations.rs` | Schema creation on startup |
| **Queries** | `src/db/queries.rs` | CRUD para memories + audit |
| **Auth** | `src/auth/api_keys.rs` | API key generation + SHA-256 hash |
| **Router** | `src/api/router.rs` | Axum Router setup |
| **Middleware** | `src/api/middleware.rs` | Logging, CORS, rate limit, auth |
| **Health** | `src/api/health.rs` | GET /v1/health |
| **Memory** | `src/api/memory.rs` | POST store/search, DELETE, GET list |
| **Audit** | `src/api/audit.rs` | POST log, GET query |
| **Models** | `src/models/memory.rs` | Memory, SearchQuery, MemoryResult |

### 2.2 MCP Server (TypeScript)

| Componente | Archivo | Responsabilidad |
|---|---|---|
| **Entry** | `plugins/mcp-server/src/index.ts` | MCP server setup, tool registration |
| **Config** | `plugins/mcp-server/src/config.ts` | API URL, key from env |
| **Memory Resources** | `plugins/mcp-server/src/resources/memory.ts` | search/store resources |
| **Context Tools** | `plugins/mcp-server/src/tools/buffer-context.ts` | Inyecta contexto antes de prompts |

### 2.3 Admin UI (React + Vite + Tailwind)

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
    id          TEXT PRIMARY KEY,          -- UUID v4
    project     TEXT NOT NULL,             -- aislamiento por proyecto
    tool        TEXT NOT NULL,             -- "claude-code", "cli", "cursor"
    user_id     TEXT NOT NULL DEFAULT 'anonymous',
    memory_type TEXT NOT NULL DEFAULT 'semantic',  -- episodic, semantic, procedural
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

CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content, tags)
    VALUES ('delete', old.rowid, old.content, old.tags);
END;

CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content, tags)
    VALUES ('delete', old.rowid, old.content, old.tags);
    INSERT INTO memories_fts(rowid, content, tags)
    VALUES (new.rowid, new.content, new.tags);
END;

-- Audit trail (append-only, simple — sin hash chain aún)
CREATE TABLE audit_logs (
    id              TEXT PRIMARY KEY,
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

-- API keys (hasheadas con SHA-256)
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
| GET | /v1/health | No | Health check con DB ping |
| POST | /v1/memory/store | Sí | Guardar memoria |
| POST | /v1/memory/search | Sí | Buscar memoria (FTS5) |
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
NEXUSMIND_ADMIN_KEY=nm_admin_<random>   # Primera API key, se genera si no existe
```

Config en `src/config.rs` con clap + env vars:
- Puerto default: 8080
- DB default: `./data/nexusmind.db`
- Log level: info
- CORS default: `*`
- Rate limit: 1000/min

---

## 6. Stack Técnico Detallado

### Dependencias Rust (Cargo.toml)

```toml
[package]
name = "nexusmind"
version = "0.1.0"
edition = "2024"

[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }

# HTTP Server
axum = "0.8"
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# SQLite
rusqlite = { version = "0.32", features = ["bundled", "vtab"] }

# CLI
clap = { version = "4", features = ["derive", "env"] }

# Auth
jsonwebtoken = "9"
sha2 = "0.10"
hex = "0.4"
uuid = { version = "1", features = ["v4", "serde"] }

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Rate limiting
governor = "0.6"

[dev-dependencies]
reqwest = { version = "0.12", features = ["json"] }
tokio-test = "0.4"

[profile.release]
opt-level = 3
strip = true
lto = true
codegen-units = 1
```

### Dependencias MCP (plugins/mcp-server/package.json)

```json
{
  "name": "nexusmind-mcp-server",
  "version": "0.1.0",
  "dependencies": {
    "@modelcontextprotocol/sdk": "^1.0.0",
    "zod": "^3.24.0"
  },
  "devDependencies": {
    "typescript": "^5.7.0",
    "@types/node": "^22.0.0"
  }
}
```

---

## 7. Diferencia con la Arquitectura Target

| Aspecto | Arquitectura Target (ADR-001) | MVP |
|---|---|---|
| **Estructura** | 8+ crates separados | 1 crate plano `src/` |
| **Auth** | SSO (SAML/OIDC) + MFA + device fingerprint | API Key simple + JWT |
| **Memory** | SQLite + sqlite-vec embeddings + Tantivy | SQLite + FTS5 sola |
| **Policy Engine** | ABAC custom, <50μs por regla | No existe |
| **Audit** | Merkle tree + Ed25519 + hash chain | Append-only simple |
| **MCP** | 19+ tools con resources completos | 3 tools básicas |
| **Admin** | Dashboard + analytics + reports | CRUD básico de memorias |
| **SDKs** | Python + TypeScript + Go | Solo REST API + curl |
| **Plugins** | Cursor, Copilot, Cline, OpenCode | Solo Claude Code (MCP) |
| **TUI** | Ratatui full app | No existe |
| **Sync** | Git chunks + cloud sync engine | No existe |
| **Deploy** | K8s + Helm + single binary | Docker compose |
| **Dependencias** | SQLite + Redis + OPA | Solo SQLite (rusqlite bundled) |

---

## 8. Docker Compose (MVP)

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
      - NEXUSMIND_ADMIN_KEY=${NEXUSMIND_ADMIN_KEY:-}
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

## 9. Dockerfile (Rust multi-stage)

```dockerfile
# Stage 1: Build
FROM rust:1.85-slim-bookworm AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

RUN cargo build --release --target-dir /app/target

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/nexusmind /usr/local/bin/nexusmind

EXPOSE 8080

ENTRYPOINT ["nexusmind", "serve"]
```

---

*Fin de 02-ARCHITECTURE_MVP.md*
