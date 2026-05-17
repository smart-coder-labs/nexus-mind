# NexusMind — MVP Task Breakdown

> **Documento**: 05-TASK_BREAKDOWN.md
> **Versión**: 0.1.0
> **Propósito**: Desglose granular de tareas para ejecutar el MVP en 4 semanas.

---

## Semana 1: Backend Core (Go + SQLite)

### Día 1 — Project Scaffold

- [ ] Inicializar Go module: `go mod init github.com/smart-coder-labs/nexus-mind`
- [ ] Crear estructura de directorios:

```bash
mkdir -p cmd/nexusmind
mkdir -p internal/{auth,db,api,config}
mkdir -p api
```

- [ ] `go.mod` con dependencias:
  - `github.com/go-chi/chi/v5` (router)
  - `github.com/mattn/go-sqlite3` (SQLite driver)
  - `github.com/google/uuid` (ID generation)
  - `golang.org/x/crypto` (SHA-256 para keys)
- [ ] `cmd/nexusmind/main.go` — entry point mínimo:

```go
func main() {
    cfg := config.Load()
    db := db.Connect(cfg.DBPath)
    db.Migrate()
    r := api.NewRouter(db, cfg)
    log.Printf("NexusMind listening on :%d", cfg.Port)
    http.ListenAndServe(fmt.Sprintf(":%d", cfg.Port), r)
}
```

- [ ] `Dockerfile` multistage build
- [ ] `docker-compose.yml` básico
- [ ] Verificar que compila: `go build ./cmd/nexusmind`
- [ ] PR #1: "chore: scaffold Go project"

### Día 2 — DB Layer

- [ ] `internal/db/db.go`:
  - Open SQLite con `?mode=wal&_journal_mode=WAL&_synchronous=NORMAL&cache_size=-8000`
  - `Connect(path string) *sql.DB`
  - `Ping()` check
- [ ] `internal/db/migrations.go`:
  - Tabla `memories`
  - Indice virtual FTS5 `memories_fts`
  - Triggers FTS5 (INSERT, DELETE, UPDATE)
  - Tabla `audit_logs`
  - Tabla `api_keys`
  - Auto-run en startup con `db.Migrate()`
- [ ] `internal/db/queries.go`:
  - `StoreMemory(ctx, m Memory) (string, error)` — INSERT + FTS trigger
  - `SearchMemory(ctx, q SearchQuery) ([]MemoryResult, error)` — FTS5 MATCH
  - `DeleteMemory(ctx, id string) error`
  - `ListMemories(ctx, project string, limit, offset int) ([]Memory, error)`
- [ ] Tests unitarios para queries (SQLite in-memory)
- [ ] PR #2: "feat: SQLite database layer"

### Día 3 — Auth + Middleware

- [ ] `internal/auth/auth.go`:
  - `GenerateKey() (string, string)` — retorna (key, hash)
  - `ValidateKey(key, hash string) bool`
  - Key format: `nexusmind_<32 random hex chars>`
- [ ] `internal/auth/middleware.go`:
  - `AuthMiddleware(db *sql.DB) func(http.Handler) http.Handler`
  - Extrae `Authorization: Bearer <key>`
  - Verifica hash contra DB
  - Setea `user_id` y `project` en context
- [ ] Endpoint `POST /v1/keys` (sin auth, solo en dev) para generar primera key
- [ ] Endpoint `GET /v1/keys` (solo con auth, listar keys activas)
- [ ] PR #3: "feat: auth with API keys"

### Día 4-5 — Memory API Endpoints

- [ ] `internal/api/router.go`:
  - Router con chi
  - Middleware: Logging, CORS, Rate Limit
  - Grupo protegido con auth
  - Grupo público (health)
- [ ] `internal/api/middleware.go`:
  - `RequestLogger` — log método, path, duración
  - `CORS` — permitir orígenes de config
  - `RateLimiter` — simple token bucket
- [ ] `internal/api/health.go`:
  - `GET /v1/health` → DB ping, uptime, version
- [ ] `internal/api/memory.go`:
  - `POST /v1/memory/store` — store memory
  - `POST /v1/memory/search` — search FTS5
  - `DELETE /v1/memory/:id` — delete
  - `GET /v1/memory` — list con filtros (project, tool, type, tags, limit, offset)
- [ ] `internal/api/audit.go`:
  - `POST /v1/audit/log` — log event
  - `GET /v1/audit` — query events (user, tool, action, dates)
- [ ] `internal/config/config.go`:
  - Load from env vars
  - Sensible defaults
- [ ] Request/Response structs en `internal/api/types.go`
- [ ] Tests de integración (start server, curl endpoints)
- [ ] PR #4: "feat: memory and audit API endpoints"

### Día 6-7 — Polish + OpenAPI Spec

- [ ] `api/openapi.yaml` — OpenAPI 3.0 spec completa de todos los endpoints
- [ ] Error handling consistente (todos los endpoints retornan `{"error":"message"}`)
- [ ] Rate limiter funcional (configurable)
- [ ] End-to-end test script (`make test-e2e`)
- [ ] Review + cleanup

---

## Semana 2: MCP Server (Claude Code Plugin)

### Día 1 — Scaffold MCP

- [ ] `plugins/mcp-server/package.json`:
  - Dependencias: `@modelcontextprotocol/sdk`
  - Build: `tsc`
  - Scripts: `dev`, `build`, `start`
- [ ] `plugins/mcp-server/tsconfig.json`
- [ ] `plugins/mcp-server/src/index.ts`:

```typescript
// MCP server entry
const server = new McpServer({
  name: "nexusmind",
  version: "0.1.0"
});
// Register resources & tools
server.start();
```

- [ ] `plugins/mcp-server/src/config.ts` — leer `NEXUSMIND_API_URL` y `NEXUSMIND_API_KEY` de env
- [ ] Verificar que el MCP server inicia
- [ ] PR #5: "feat: MCP server scaffold"

### Día 2 — MCP Memory Resources

- [ ] `plugins/mcp-server/src/resources/memory.ts`:

```typescript
// MCP Resource: nexusmind://memory/search?q=...
server.resource(
  "memory",
  "nexmind://memory/{type}",
  async (uri, { type }) => {
    // GET from NexusMind API
  }
);
```

- [ ] `plugins/mcp-server/src/tools/buffer-context.ts`:

```typescript
// MCP Tool: nexusmind-store
server.tool(
  "nexusmind-store",
  { content: z.string(), tags: z.string().optional() },
  async ({ content, tags }) => {
    // POST /v1/memory/store
  }
);

// MCP Tool: nexusmind-search
server.tool(
  "nexusmind-search",
  { query: z.string(), project: z.string().optional() },
  async ({ query, project }) => {
    // POST /v1/memory/search
  }
);
```

- [ ] Error handling: si NexusMind API no responde, mensaje claro
- [ ] PR #6: "feat: MCP memory tools and resources"

### Día 3-4 — Config + README + Tests

- [ ] `plugins/mcp-server/README.md`:
  - Requisitos (Node 18+, NexusMind running)
  - Instalación
  - Configuración en `~/.claude/settings.json`
  - Ejemplos de uso
  - Troubleshooting
- [ ] Tests unitarios del MCP server (mock NexusMind API)
- [ ] Verificar con Claude Code real
- [ ] PR #7: "docs: MCP server installation guide"

### Día 5-7 — Buffer Context Tool + Polish

- [ ] Tool `nexusmind-buffer-context`:
  - Se ejecuta automáticamente antes de cada prompt
  - Busca memorias relevantes del proyecto actual
  - Inyecta contexto en el system prompt
- [ ] Performance: caché de resultados frecuentes (TTL 30s)
- [ ] Logging: qué contexto se inyectó
- [ ] PR #8: "feat: automatic context injection tool"

---

## Semana 3: Admin UI + Cross-Tool Memory

### Día 1-2 — Admin UI Scaffold

- [ ] `admin/package.json`:
  - Vite + React 18 + TypeScript
  - Tailwind CSS v4
  - `lucide-react` para íconos
  - `react-router-dom` para navegación
- [ ] `admin/vite.config.ts` — proxy a `localhost:8080` para desarrollo
- [ ] `admin/tailwind.config.ts`
- [ ] `admin/src/main.tsx` + `App.tsx` + router
- [ ] Layout básico: sidebar + header + content area
- [ ] PR #9: "feat: admin UI scaffold"

### Día 3-4 — Memories + Audit Pages

- [ ] `admin/src/api/client.ts` — fetch wrapper:

```typescript
class NexusMindClient {
  constructor(private baseUrl: string, private apiKey: string) {}
  async storeMemory(data: StoreMemoryInput): Promise<Memory>
  async searchMemory(query: SearchQuery): Promise<SearchResult[]>
  async listMemories(project?: string): Promise<Memory[]>
  async deleteMemory(id: string): Promise<void>
  async getAuditLog(params: AuditParams): Promise<AuditEntry[]>
}
```

- [ ] `admin/src/pages/Memories.tsx`:
  - Lista de memorias con búsqueda
  - Filtrar por proyecto, tool, tags
  - Ver detalle de memoria
  - Borrar memoria (con confirmación)
- [ ] `admin/src/pages/AuditLog.tsx`:
  - Tabla de audit events
  - Filtros por fecha, usuario, tool
- [ ] PR #10: "feat: admin UI pages"

### Día 5-7 — Settings + Polish

- [ ] `admin/src/pages/Settings.tsx`:
  - Mostrar/regenerar API keys
  - Configurar proyectos
- [ ] Responsive design (mobile-friendly)
- [ ] Dark mode (opcional)
- [ ] PR #11: "feat: admin settings page"

---

## Semana 4: Polish, Deploy, Release

### Día 1-2 — README + Makefile + Docs

- [ ] README.md general:

```markdown
# NexusMind

Control plane unificado para herramientas AI.

## Quickstart

\`\`\`bash
git clone https://github.com/smart-coder-labs/nexus-mind
cd nexus-mind
cp .env.example .env
docker compose up -d

# Get your API key
docker compose exec nexusmind ./nexusmind keygen
\`\`\`

## Claude Code Integration

[Ver plugins/mcp-server/README.md](./plugins/mcp-server/README.md)
```

- [ ] `Makefile`:

```makefile
.PHONY: dev build test run clean

dev:
	docker compose up

build:
	go build -o bin/nexusmind ./cmd/nexusmind

test:
	go test ./...
	cd plugins/mcp-server && npm test

run:
	go run ./cmd/nexusmind

lint:
	golangci-lint run

e2e:
	./scripts/test-e2e.sh
```

- [ ] `scripts/test-e2e.sh` — script que:
  1. Inicia docker compose
  2. Espera health check
  3. Store una memoria
  4. Search esa memoria
  5. Verifica resultados
  6. Cleanup

### Día 3 — CI/CD

- [ ] `.github/workflows/ci.yml`:

```yaml
name: CI
on: [push, pull_request]
jobs:
  backend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-go@v5
        with: { go-version: '1.22' }
      - run: go build ./cmd/nexusmind
      - run: go test ./...
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

### Día 4 — Landing Page Update

- [ ] Añadir sección "Try the MVP" en `apps/landing/`
- [ ] Link al README con quickstart
- [ ] Botón "Get API Key" → lleva al README
- [ ] Testimonial placeholder: "NexusMind me permitió mantener contexto entre sesiones de Claude Code"

### Día 5 — Bug Bash

- [ ] Checklist:
  - `git clone && docker compose up` → todo funciona
  - Cualquier dev con Node 18+ puede conectar Claude Code en <5 min
  - API responde a todos los endpoints
  - Admin UI carga sin errores
  - CI pasa en GitHub
- [ ] Invitar a 3 developers externos a probar
- [ ] Documentar bugs encontrados

### Día 6 — Release v0.1.0

- [ ] Tag: `git tag v0.1.0 && git push --tags`
- [ ] GitHub Release:
  - Asset: binary compilado (linux/amd64)
  - Changelog: qué incluye v0.1.0
  - Quickstart links
- [ ] Post en el Discord de Claude Code (si existe)
- [ ] Post en Hacker News / Lobsters (opcional)

### Día 7 — Retrospective

- [ ] Revisar métricas:
  - ¿Cuántos developers probaron?
  - ¿Cuántas memorias se almacenaron?
  - ¿Feedback cualitativo?
- [ ] Decidir: ¿continuar como está, pivotear, o escalar?
- [ ] Escribir "Post-MVP Plan" v0.2.0

---

## Estimación Total

| Componente | Días | Archivos | Líneas |
|---|---|---|---|
| Go backend | 7 | 12 | ~1000 |
| MCP plugin | 5 | 6 | ~500 |
| Admin UI | 5 | 10 | ~800 |
| Docs/CI/Infra | 4 | 6 | ~300 |
| Bug bash + Release | 3 | - | - |
| Buffer | 4 | - | - |
| **Total** | **28** | **~34** | **~2600** |

---

*Fin de 05-TASK_BREAKDOWN.md*
