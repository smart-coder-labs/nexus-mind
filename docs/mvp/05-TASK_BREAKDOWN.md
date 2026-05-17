# NexusMind — MVP Task Breakdown (v1 — Archivado)

> ⚠️ **Este desglose corresponde al plan v1 (developer-focused).**
> Para el plan enterprise, ver [06-ENTERPRISE_MVP.md](./06-ENTERPRISE_MVP.md).

> **Documento**: 05-TASK_BREAKDOWN.md
> **Versión**: 0.1.0 (archivado)
> **Propósito**: Desglose granular del plan v1. **Stack: Rust** (ADR-001) + SQLite (ADR-002) + MCP Server en TypeScript.

---

## Semana 1: Backend Core (Rust + SQLite + Axum)

### Día 1 — Project Scaffold

- [ ] Inicializar Rust project:

```bash
cargo init --name nexusmind
```

- [ ] `Cargo.toml` con dependencias:
  - `tokio` (async runtime)
  - `axum` (HTTP framework)
  - `tower-http` (CORS, tracing middleware)
  - `serde` + `serde_json` (serialization)
  - `rusqlite` con feature `bundled` (SQLite sin dependencias externas)
  - `clap` con feature `env` (CLI + ENV config)
  - `uuid` (ID generation)
  - `sha2` + `hex` (API key hashing)
  - `jsonwebtoken` (JWT — opcional en MVP)
  - `tracing` + `tracing-subscriber` (logging)
- [ ] `rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.85.0"
targets = ["x86_64-unknown-linux-gnu"]
```

- [ ] `src/main.rs` — entry point mínimo:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::init();
    let config = config::Config::parse();
    let db = db::connect(&config.db_path)?;
    db::migrate(&db)?;
    let app = api::router(db, config.clone());
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("NexusMind listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] Crear estructura de directorios:

```bash
mkdir -p src/{auth,db,api,models}
```

- [ ] `Dockerfile` multi-stage build (rust:1.85-slim → debian:bookworm-slim)
- [ ] `docker-compose.yml` (nexusmind backend + nginx para admin en Semana 3)
- [ ] Verificar que compila: `cargo build`
- [ ] PR #1: "chore: scaffold Rust project"

### Día 2 — DB Layer

- [ ] `src/db/connection.rs`:

```rust
use rusqlite::Connection;

pub fn connect(path: &str) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA cache_size=-8000;")?;
    Ok(conn)
}
```

- [ ] `src/db/migrations.rs`:

```rust
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS memories (
            id TEXT PRIMARY KEY,
            project TEXT NOT NULL,
            tool TEXT NOT NULL,
            user_id TEXT NOT NULL DEFAULT 'anonymous',
            memory_type TEXT NOT NULL DEFAULT 'semantic',
            content TEXT NOT NULL,
            tags TEXT DEFAULT '[]',
            metadata TEXT DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
            content, tags,
            content='memories',
            content_rowid='rowid'
        );
        -- Triggers FTS5
        CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
            INSERT INTO memories_fts(rowid, content, tags)
            VALUES (new.rowid, new.content, new.tags);
        END;
        CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, tags)
            VALUES ('delete', old.rowid, old.content, old.tags);
        END;
        CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, tags)
            VALUES ('delete', old.rowid, old.content, old.tags);
            INSERT INTO memories_fts(rowid, content, tags)
            VALUES (new.rowid, new.content, new.tags);
        END;
        CREATE TABLE IF NOT EXISTS audit_logs (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL DEFAULT (datetime('now')),
            user_id TEXT NOT NULL,
            tool TEXT NOT NULL,
            action TEXT NOT NULL,
            resource_type TEXT NOT NULL DEFAULT 'memory',
            resource_id TEXT,
            metadata TEXT DEFAULT '{}'
        );
        CREATE TABLE IF NOT EXISTS api_keys (
            id TEXT PRIMARY KEY,
            key_hash TEXT NOT NULL UNIQUE,
            label TEXT NOT NULL,
            scopes TEXT NOT NULL DEFAULT '[\"memory:read\",\"memory:write\"]',
            project TEXT DEFAULT '*',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            revoked INTEGER NOT NULL DEFAULT 0
        );
    ")?;
    Ok(())
}
```

- [ ] `src/db/queries.rs` — funciones CRUD:

```rust
pub fn store_memory(conn: &Connection, input: StoreInput) -> Result<String>
pub fn search_memory(conn: &Connection, query: &str, project: &str, limit: u32) -> Result<Vec<MemoryResult>>
pub fn delete_memory(conn: &Connection, id: &str) -> Result<bool>
pub fn list_memories(conn: &Connection, project: Option<&str>, tool: Option<&str>, limit: u32, offset: u32) -> Result<Vec<Memory>>
pub fn log_audit(conn: &Connection, event: AuditEvent) -> Result<String>
pub fn query_audit(conn: &Connection, filters: AuditFilter) -> Result<Vec<AuditEntry>>
pub fn insert_api_key(conn: &Connection, key_hash: &str, label: &str) -> Result<String>
pub fn validate_api_key(conn: &Connection, key_hash: &str) -> Result<bool>
```

- [ ] Tests unitarios con SQLite in-memory (`rusqlite::Connection::open_in_memory()`)
- [ ] PR #2: "feat: SQLite database layer"

### Día 3 — Auth + Middleware

- [ ] `src/auth/api_keys.rs`:

```rust
pub fn generate_key() -> (String, String) {
    // Retorna (raw_key, sha256_hash)
    let key = format!("nm_{}", hex::encode(thread_rng().gen::<[u8; 32]>()));
    let hash = hex::encode(Sha256::digest(key.as_bytes()));
    (key, hash)
}

pub fn hash_key(key: &str) -> String {
    hex::encode(Sha256::digest(key.as_bytes()))
}
```

- [ ] `src/api/middleware.rs` — auth middleware:

```rust
pub async fn auth_middleware(
    mut req: Request<Body>,
    next: Next,
    db: Arc<Mutex<Connection>>,
) -> Result<Response, StatusCode> {
    let auth_header = req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let hash = auth::hash_key(auth_header);
    let db = db.lock().unwrap();
    if !db::validate_api_key(&db, &hash).unwrap_or(false) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    // Si es válido, pasar al handler
    Ok(next.run(req).await)
}
```

- [ ] Endpoint `POST /v1/keys` (sin auth en dev) para generar primera key
- [ ] PR #3: "feat: auth with API keys"

### Día 4 — Memory API Endpoints

- [ ] `src/models/memory.rs` — structs compartidos:

```rust
#[derive(Serialize, Deserialize)]
pub struct StoreInput {
    pub tool: String,
    pub project: String,
    #[serde(rename = "type")]
    pub memory_type: String,
    pub content: String,
    pub tags: Option<Vec<String>>,
    pub user_id: Option<String>,
}

#[derive(Serialize)]
pub struct StoreResponse {
    pub id: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub id: String,
    pub content: String,
    pub score: f64,
    pub tool: String,
    pub tags: Vec<String>,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct SearchInput {
    pub query: String,
    pub project: Option<String>,
    pub limit: Option<u32>,
    pub min_score: Option<f64>,
}
```

- [ ] `src/api/router.rs` — Axum Router:

```rust
pub fn router(db: Connection, config: Config) -> Router {
    let db = Arc::new(Mutex::new(db));

    Router::new()
        .route("/v1/health", get(health::handler))
        .route("/v1/memory/store", post(memory::store))
        .route("/v1/memory/search", post(memory::search))
        .route("/v1/memory/{id}", delete(memory::delete))
        .route("/v1/memory", get(memory::list))
        .route("/v1/audit/log", post(audit::log))
        .route("/v1/audit", get(audit::query))
        .layer(middleware::from_fn_with_state(db.clone(), move |req, next| auth_middleware(req, next, db.clone())))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(db)
}
```

- [ ] `src/api/health.rs` — `GET /v1/health`:
  - DB ping, uptime (desde startup), version del binary
- [ ] `src/api/memory.rs` — handlers:
  - `POST /v1/memory/store` — validar input, store, log audit, retornar 201
  - `POST /v1/memory/search` — FTS5 MATCH query, retornar resultados con score
  - `DELETE /v1/memory/:id` — soft delete con `deleted_at`
  - `GET /v1/memory` — list con filtros (project, tool, type, tags, limit, offset)
- [ ] `src/api/audit.rs` — handlers:
  - `POST /v1/audit/log` — evento audit (llamado internamente por otros handlers)
  - `GET /v1/audit` — query con filtros (user, tool, action, from, to, limit)
- [ ] `src/config.rs`:

```rust
#[derive(Parser, Clone)]
pub struct Config {
    #[arg(long, env, default_value = "8080")]
    pub port: u16,
    #[arg(long, env, default_value = "./data/nexusmind.db")]
    pub db_path: String,
    #[arg(long, env, default_value = "info")]
    pub log_level: String,
    #[arg(long, env, default_value = "*")]
    pub cors_origins: String,
    #[arg(long, env, default_value = "1000")]
    pub rate_limit_per_min: u32,
}
```

- [ ] Tests de integración con reqwest (iniciar server en background, curl endpoints)
- [ ] PR #4: "feat: memory and audit API endpoints"

### Día 5 — Polish + OpenAPI Spec

- [ ] `api/openapi.yaml` — OpenAPI 3.0 spec de todos los endpoints
- [ ] Error handling consistente: todos los handlers retornan `Json<ErrorResponse>`
- [ ] Rate limiter simple (governor o custom token bucket)
- [ ] Test E2E script (`make test-e2e`)
- [ ] PR #5: "chore: OpenAPI spec, polish, and tests"

---

## Semana 2: MCP Server (Claude Code Plugin)

### Día 1 — Scaffold MCP

- [ ] `plugins/mcp-server/package.json`:

```json
{
  "name": "nexusmind-mcp-server",
  "version": "0.1.0",
  "type": "module",
  "dependencies": {
    "@modelcontextprotocol/sdk": "^1.0.0",
    "zod": "^3.24.0"
  },
  "devDependencies": {
    "typescript": "^5.7.0",
    "@types/node": "^22.0.0"
  },
  "scripts": {
    "build": "tsc",
    "start": "node dist/index.js"
  }
}
```

- [ ] `plugins/mcp-server/tsconfig.json`
- [ ] `plugins/mcp-server/src/index.ts` — MCP server entry:

```typescript
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";

const server = new Server({
  name: "nexusmind",
  version: "0.1.0",
}, {
  capabilities: { tools: {}, resources: {} }
});

// Register tools
registerMemoryTools(server);
registerContextTool(server);

// Start
const transport = new StdioServerTransport();
await server.connect(transport);
```

- [ ] `plugins/mcp-server/src/config.ts`:

```typescript
export const config = {
  apiUrl: process.env.NEXUSMIND_API_URL || "http://localhost:8080",
  apiKey: process.env.NEXUSMIND_API_KEY || "",
  defaultProject: process.env.NEXUSMIND_PROJECT || "default",
};
```

- [ ] Verificar que el MCP server inicia: `npm run build && npm start`
- [ ] PR #6: "feat: MCP server scaffold"

### Día 2 — MCP Memory Tools

- [ ] `plugins/mcp-server/src/tools/memory-store.ts`:

```typescript
server.tool(
  "nexusmind-store",
  "Store a memory in NexusMind",
  { content: z.string(), tags: z.string().optional(), project: z.string().optional() },
  async ({ content, tags, project }) => {
    const res = await fetch(`${config.apiUrl}/v1/memory/store`, {
      method: "POST",
      headers: { "Authorization": `Bearer ${config.apiKey}`, "Content-Type": "application/json" },
      body: JSON.stringify({
        tool: "claude-code",
        project: project || config.defaultProject,
        type: "semantic",
        content,
        tags: tags ? JSON.parse(tags) : [],
      }),
    });
    const data = await res.json();
    return { content: `✅ Memory stored: ${data.id}` };
  }
);
```

- [ ] `plugins/mcp-server/src/tools/memory-search.ts`:

```typescript
server.tool(
  "nexusmind-search",
  "Search memories in NexusMind",
  { query: z.string(), project: z.string().optional(), limit: z.number().optional() },
  async ({ query, project, limit }) => {
    const res = await fetch(`${config.apiUrl}/v1/memory/search`, {
      method: "POST",
      headers: { "Authorization": `Bearer ${config.apiKey}`, "Content-Type": "application/json" },
      body: JSON.stringify({ query, project: project || config.defaultProject, limit: limit || 5 }),
    });
    const data = await res.json();
    return { content: JSON.stringify(data.results, null, 2) };
  }
);
```

- [ ] PR #7: "feat: MCP memory tools"

### Día 3-4 — Buffer Context Tool

- [ ] `plugins/mcp-server/src/tools/buffer-context.ts`:

```typescript
// Tool que se ejecuta implícitamente para inyectar contexto
// Busca memorias relevantes y las devuelve como contexto adicional
server.tool(
  "nexusmind-buffer-context",
  "Get relevant context from NexusMind for the current session",
  { project: z.string().optional(), task: z.string().optional() },
  async ({ project, task }) => {
    const memories = await fetch(`${config.apiUrl}/v1/context/project/${project || config.defaultProject}`);
    const data = await memories.json();
    return {
      content: `📚 Project Context:\n${data.memories.map(m => `- ${m.content}`).join('\n')}`
    };
  }
);
```

- [ ] Performance: caché de resultados recientes (TTL 60s) en memoria
- [ ] PR #8: "feat: buffer context tool"

### Día 5-7 — README + Tests E2E

- [ ] `plugins/mcp-server/README.md`:
  - Requisitos (Node 18+, Rust backend running)
  - Instalación
  - Configuración en `~/.claude/settings.json`
  - Ejemplos de uso
  - Troubleshooting
- [ ] Tests unitarios con mocks
- [ ] Verificar con Claude Code real
- [ ] PR #9: "docs: MCP server installation and testing"

---

## Semana 3: Admin UI + Cross-Tool Memory

### Día 1-2 — Admin UI Scaffold

- [ ] `admin/package.json` (Vite + React 19 + Tailwind)
- [ ] `admin/vite.config.ts` — proxy `/api` → `localhost:8080`
- [ ] `admin/tailwind.config.ts`
- [ ] `admin/src/main.tsx` + `App.tsx` + react-router
- [ ] PR #10: "feat: admin UI scaffold"

### Día 3-4 — API Client + Pages

- [ ] `admin/src/api/client.ts` — TypeScript fetch wrapper:

```typescript
class NexusMindClient {
  constructor(private baseUrl: string, private apiKey: string) {}
  async storeMemory(data: StoreInput): Promise<Memory>
  async searchMemory(q: SearchInput): Promise<SearchResults>
  async listMemories(project?: string): Promise<Memory[]>
  async deleteMemory(id: string): Promise<void>
  async getAuditLog(params: AuditParams): Promise<AuditEntry[]>
  async listApiKeys(): Promise<ApiKey[]>
  async createApiKey(label: string): Promise<ApiKey>
}
```

- [ ] `admin/src/pages/Memories.tsx` — CRUD de memorias
- [ ] `admin/src/pages/AuditLog.tsx` — visualización de audit trail
- [ ] PR #11: "feat: admin UI pages"

### Día 5-7 — Settings + Polish

- [ ] `admin/src/pages/Settings.tsx` — API keys, projects
- [ ] Responsive design
- [ ] Dark mode (opcional)
- [ ] PR #12: "feat: admin settings page"

---

## Semana 4: Polish, Deploy, Release

### Día 1 — README + Makefile + Scripts

- [ ] `README.md` — quickstart end-to-end:

```markdown
# NexusMind

Control plane unificado para herramientas AI.

Stack: Rust + SQLite + Axum + MCP (Claude Code)

## Quickstart

\`\`\`bash
git clone https://github.com/smart-coder-labs/nexus-mind
cd nexus-mind
docker compose up -d

# Get your API key
docker compose exec nexusmind ./nexusmind keygen --label default
\`\`\`
```

- [ ] `Makefile`:

```makefile
.PHONY: dev build test run clean

dev:
	docker compose up

build:
	cargo build --release

test:
	cargo test
	cd plugins/mcp-server && npm test
	cd admin && npm run build

run:
	cargo run -- serve

lint:
	cargo clippy
	cd plugins/mcp-server && npx tsc --noEmit

e2e:
	./scripts/test-e2e.sh
```

- [ ] `scripts/test-e2e.sh`:

```bash
#!/bin/bash
set -euo pipefail

echo "🚀 Starting NexusMind..."
cargo run -- serve &
SERVER_PID=$!
sleep 2

echo "✅ Health check..."
curl -s http://localhost:8080/v1/health | grep -q '"status":"ok"'

echo "✅ Creating API key..."
RESP=$(curl -s -X POST http://localhost:8080/v1/keys -d '{"label":"test"}')
KEY=$(echo $RESP | jq -r '.key')

echo "✅ Storing memory..."
curl -s -X POST http://localhost:8080/v1/memory/store \
  -H "Authorization: Bearer $KEY" \
  -H "Content-Type: application/json" \
  -d '{"tool":"cli","project":"test","type":"semantic","content":"Test memory"}' | grep -q '"id"'

echo "✅ Searching memory..."
curl -s -X POST http://localhost:8080/v1/memory/search \
  -H "Authorization: Bearer $KEY" \
  -H "Content-Type: application/json" \
  -d '{"query":"test","project":"test"}' | grep -q '"results"'

echo "🎉 All tests passed!"
kill $SERVER_PID
```

### Día 2 — CI/CD

- [ ] `.github/workflows/ci.yml`:

```yaml
name: CI
on: [push, pull_request]

jobs:
  backend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - uses: Swatinem/rust-cache@v2
      - run: cargo build
      - run: cargo test
      - run: cargo clippy -- -D warnings

  mcp-server:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: '18' }
      - run: cd plugins/mcp-server && npm ci && npm test

  admin-ui:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: '18' }
      - run: cd admin && npm ci && npm run build
```

### Día 3 — Landing Page Update

- [ ] Añadir sección "Try the MVP" en `apps/landing/`
- [ ] Link al README con quickstart
- [ ] Badge: "Built with Rust"
- [ ] CTA: "Deploy in 2 minutes with Docker"

### Día 4 — Bug Bash

- [ ] Checklist:
  - [ ] `cargo build --release` compila sin errores
  - [ ] `docker compose up` levanta todo
  - [ ] `curl v1/health` funciona
  - [ ] API key se genera y funciona
  - [ ] Store/Search/Delete memory vía curl
  - [ ] MCP server se conecta a Claude Code
  - [ ] Admin UI carga sin errores
  - [ ] CI pasa en GitHub Actions

### Día 5 — Release v0.1.0

- [ ] Tag: `git tag v0.1.0 && git push --tags`
- [ ] GitHub Release:
  - Asset: `cargo build --release` binary
  - Changelog: qué incluye v0.1.0
  - Quickstart links
- [ ] Post en Discord de Claude Code
- [ ] Post en r/rust (opcional)

### Día 6-7 — Retro + Buffer

- [ ] Revisar métricas:
  - ¿Cuántos developers probaron?
  - ¿Cuántas memorias se almacenaron?
  - Feedback cualitativo
- [ ] Decidir: ¿continuar como está, pivotear, o escalar?
- [ ] Escribir "Post-MVP Plan v0.2.0"

---

## Estimación Total

| Componente | Días | Archivos | Líneas |
|---|---|---|---|
| Rust backend | 7 | 12 | ~1200 |
| MCP plugin (TS) | 5 | 6 | ~500 |
| Admin UI (React) | 5 | 10 | ~800 |
| Docs/CI/Infra | 4 | 6 | ~300 |
| Bug bash + Release | 4 | - | - |
| Buffer | 3 | - | - |
| **Total** | **28** | **~34** | **~2800** |

---

*Fin de 05-TASK_BREAKDOWN.md*
