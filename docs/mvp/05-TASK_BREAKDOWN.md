# NexusMind — Enterprise MVP Task Breakdown (v2)

> **Documento**: 05-TASK_BREAKDOWN.md
> **Versión**: 2.0
> **Fecha**: Mayo 2026
> **Propósito**: Desglose granular del plan enterprise (06-ENTERPRISE_MVP.md). Stack: Rust + SQLite + Axum (backend), React + Vite + Tailwind (admin panel).

---

## Semana 1: Backend Multi-Tenant (Rust + SQLite + Axum)

### Día 1 — Project Scaffold + Modelo de Datos

- [x] Inicializar Rust project:

```bash
cargo init --name nexusmind
```

- [x] `Cargo.toml` con dependencias:
  - `tokio` (async runtime)
  - `axum` (HTTP framework)
  - `tower-http` (CORS, tracing middleware)
  - `serde` + `serde_json` (serialization)
  - `rusqlite` con feature `bundled`
  - `clap` con feature `env` (CLI + ENV config)
  - `uuid` (ID generation)
  - `sha2` + `hex` (API key hashing)
  - `tracing` + `tracing-subscriber` (logging)
  - `anyhow` (error handling)
  - `chrono` (timestamps)

- [x] `rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.85.0"
targets = ["x86_64-unknown-linux-gnu"]
```

- [x] Estructura de directorios:

```
src/
├── main.rs
├── config.rs
├── lib.rs
├── auth/
│   ├── mod.rs
│   └── api_keys.rs
├── db/
│   ├── mod.rs
│   ├── connection.rs
│   ├── migrations.rs
│   └── queries.rs
├── api/
│   ├── mod.rs
│   ├── router.rs
│   ├── middleware.rs
│   ├── health.rs
│   ├── memory.rs
│   ├── audit.rs
│   ├── users.rs
│   └── admin.rs
└── models/
    ├── mod.rs
    └── types.rs
```

- [x] `src/config.rs`:

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
}
```

- [x] `src/main.rs` — entry point:

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

- [x] `Dockerfile` multi-stage (rust:1.85-slim → debian:bookworm-slim)
- [x] `docker-compose.yml` básico (backend + admin en Semana 3)
- [x] Verificar compilación: `cargo build`
- [ ] PR #1: "chore: scaffold Rust project with multi-tenant structure"

---

### Día 2 — DB Layer Multi-Tenant

- [ ] `src/db/connection.rs`:

```rust
pub fn connect(path: &str) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA cache_size=-8000;"
    )?;
    Ok(conn)
}
```

- [ ] `src/db/migrations.rs` — schema multi-tenant:

```sql
CREATE TABLE IF NOT EXISTS organizations (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    slug        TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS users (
    id          TEXT PRIMARY KEY,
    org_id      TEXT NOT NULL REFERENCES organizations(id),
    email       TEXT NOT NULL,
    name        TEXT NOT NULL,
    role        TEXT NOT NULL DEFAULT 'member',  -- 'admin' | 'member' | 'viewer'
    status      TEXT NOT NULL DEFAULT 'active',  -- 'active' | 'invited' | 'suspended'
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(org_id, email)
);

CREATE TABLE IF NOT EXISTS api_keys (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id),
    org_id      TEXT NOT NULL REFERENCES organizations(id),
    key_hash    TEXT NOT NULL UNIQUE,
    label       TEXT NOT NULL,
    last_used   TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    revoked     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS memories (
    id          TEXT PRIMARY KEY,
    org_id      TEXT NOT NULL REFERENCES organizations(id),
    user_id     TEXT NOT NULL REFERENCES users(id),
    project     TEXT NOT NULL DEFAULT 'default',
    tool        TEXT NOT NULL,
    content     TEXT NOT NULL,
    tags        TEXT DEFAULT '[]',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    content, tags,
    content='memories',
    content_rowid='rowid'
);

-- FTS5 triggers
CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, content, tags) VALUES (new.rowid, new.content, new.tags);
END;
CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content, tags) VALUES ('delete', old.rowid, old.content, old.tags);
END;
CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content, tags) VALUES ('delete', old.rowid, old.content, old.tags);
    INSERT INTO memories_fts(rowid, content, tags) VALUES (new.rowid, new.content, new.tags);
END;

CREATE TABLE IF NOT EXISTS audit_logs (
    id              TEXT PRIMARY KEY,
    org_id          TEXT NOT NULL REFERENCES organizations(id),
    user_id         TEXT NOT NULL REFERENCES users(id),
    timestamp       TEXT NOT NULL DEFAULT (datetime('now')),
    action          TEXT NOT NULL,
    resource_type   TEXT NOT NULL,
    resource_id     TEXT,
    metadata        TEXT DEFAULT '{}'
);
```

- [ ] `src/models/types.rs` — structs compartidos:

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct Org {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct User {
    pub id: String,
    pub org_id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AuthContext {
    pub org_id: String,
    pub user_id: String,
    pub role: String,
}

#[derive(Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub org_id: String,
    pub user_id: String,
    pub project: String,
    pub tool: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: String,
}

#[derive(Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub org_id: String,
    pub user_id: String,
    pub timestamp: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub metadata: serde_json::Value,
}
```

- [ ] Tests unitarios con SQLite in-memory
- [ ] PR #2: "feat: multi-tenant SQLite schema and migrations"

---

### Día 3 — Auth con Org Scoping

- [ ] `src/auth/api_keys.rs`:

```rust
pub fn generate_key() -> (String, String) {
    // Retorna (raw_key, sha256_hash)
    let raw = format!("nm_{}", hex::encode(rand::random::<[u8; 32]>()));
    let hash = hex::encode(Sha256::digest(raw.as_bytes()));
    (raw, hash)
}

pub fn hash_key(key: &str) -> String {
    hex::encode(Sha256::digest(key.as_bytes()))
}
```

- [ ] `src/db/queries.rs` — validación de key con org_id:

```rust
pub fn validate_api_key(conn: &Connection, key_hash: &str) -> Result<Option<AuthContext>>
// Retorna AuthContext { org_id, user_id, role } si la key existe y no está revocada
// Actualiza last_used en api_keys
```

- [ ] `src/api/middleware.rs` — auth middleware que inyecta `AuthContext` en extensions:

```rust
pub async fn auth_middleware(
    State(db): State<Arc<Mutex<Connection>>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let key = extract_bearer(&req).ok_or(StatusCode::UNAUTHORIZED)?;
    let hash = auth::hash_key(&key);
    let ctx = db.lock().unwrap()
        .validate_api_key(&hash)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}
```

- [ ] Endpoint `POST /v1/bootstrap` — sin auth, solo corre si DB vacía → crea primera org + admin key
- [ ] PR #3: "feat: API key auth with org scoping"

---

### Día 4 — Memory API + Users API

- [ ] `src/api/memory.rs`:
  - `POST /v1/memory/store` — valida input, usa `org_id` del AuthContext, log audit, 201
  - `POST /v1/memory/search` — FTS5 MATCH **filtrado por org_id** del AuthContext
  - `DELETE /v1/memory/:id` — verifica que la memoria pertenece a la org
  - `GET /v1/memory` — list con filtros (project, tool, user_id, tags, limit, offset), scoped por org

- [ ] `src/api/users.rs`:
  - `GET /v1/users` — lista miembros de la org (requiere rol admin)
  - `POST /v1/users/invite` — crea user con status=invited + genera API key
  - `DELETE /v1/users/:id` — suspende user + revoca todas sus keys (requiere admin)
  - `POST /v1/users/:id/rotate-key` — genera nueva key, revoca la anterior

- [ ] `src/api/admin.rs`:
  - `GET /v1/admin/stats` — total memorias, usuarios activos 24h, búsquedas hoy, tools usadas
  - `GET /v1/admin/org` — info de la org
  - `PATCH /v1/admin/org` — update nombre de org (requiere admin)

- [ ] Regla de oro: **todo query en `queries.rs` recibe `org_id` del AuthContext. Nunca cruza datos entre orgs.**
- [ ] Tests de integración con SQLite in-memory
- [ ] PR #4: "feat: memory, users, and admin API endpoints"

---

### Día 5 — Audit Trail + Health

- [ ] `src/api/audit.rs`:
  - `GET /v1/audit` — lista eventos de la org con filtros (user_id, action, resource_type, from, to, limit, offset)
  - Todos los handlers de memory y users llaman a `log_audit` internamente

- [ ] `src/api/health.rs`:
  - `GET /v1/health` → `{ status: "ok", version: "0.2.0", db: "ok", uptime_secs: 123 }`

- [ ] `src/api/router.rs` — Axum Router completo:

```rust
pub fn router(db: Connection, config: Config) -> Router {
    let db = Arc::new(Mutex::new(db));
    Router::new()
        .route("/v1/health", get(health::handler))
        .route("/v1/bootstrap", post(admin::bootstrap))
        .route("/v1/memory/store", post(memory::store))
        .route("/v1/memory/search", post(memory::search))
        .route("/v1/memory/:id", delete(memory::delete))
        .route("/v1/memory", get(memory::list))
        .route("/v1/users", get(users::list))
        .route("/v1/users/invite", post(users::invite))
        .route("/v1/users/:id", delete(users::remove))
        .route("/v1/users/:id/rotate-key", post(users::rotate_key))
        .route("/v1/audit", get(audit::query))
        .route("/v1/admin/stats", get(admin::stats))
        .route("/v1/admin/org", get(admin::get_org).patch(admin::update_org))
        .layer(middleware::from_fn_with_state(db.clone(), auth_middleware))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(db)
}
```

- [ ] Error handling consistente: todos los handlers retornan `Json<ApiError>` con `{ error: string, code: string }`
- [ ] PR #5: "feat: audit trail, health, and full router"

---

### Días 6-7 — Seed Data + Tests

- [ ] `scripts/seed.rs` (o Rust binary en `src/bin/seed.rs`) que crea:
  - 3 organizaciones: "Acme Corp", "TechStartup", "DevShop"
  - 5 usuarios por org con roles variados
  - ~60 memorias por org con tags y tools variados
  - 1 API key admin por org (hardcoded para demo)

- [ ] Tests E2E en `tests/integration_test.rs`:
  - Bootstrap → crear org
  - Invite user → validar key
  - Store memory → verify org isolation
  - Search memory → solo resultados de la org
  - Audit log → entries con org_id correcto
  - Remove user → key revocada

- [ ] PR #6: "feat: seed data script and integration tests"

---

## Semana 2: Admin Panel Enterprise (React + Vite + Tailwind)

### Día 1 — Scaffold + Login

- [ ] `admin/package.json` (React 18, Vite, React Router v6, TanStack Query, Tailwind CSS v4)
- [ ] `admin/vite.config.ts` — proxy `/api` → `http://localhost:8080`
- [ ] `admin/src/api/client.ts` — TypeScript fetch wrapper:

```typescript
class NexusMindClient {
  constructor(private baseUrl: string, private apiKey: string) {}

  async getStats(): Promise<OrgStats>
  async getOrg(): Promise<Org>
  async listUsers(): Promise<User[]>
  async inviteUser(data: InviteUserInput): Promise<User>
  async removeUser(id: string): Promise<void>
  async rotateKey(userId: string): Promise<ApiKey>
  async listMemories(filters: MemoryFilters): Promise<Memory[]>
  async searchMemories(query: string): Promise<Memory[]>
  async getAuditLog(filters: AuditFilters): Promise<AuditEntry[]>
  async updateOrg(data: Partial<Org>): Promise<Org>
}
```

- [ ] `admin/src/pages/Login.tsx`:
  - Input para API key
  - On submit: `GET /v1/admin/org` para validar key y detectar rol
  - Guarda `{ apiKey, org, user }` en localStorage
  - Redirect a Dashboard si válido

- [ ] PR #7: "feat: admin panel scaffold and login page"

---

### Día 2 — Dashboard

- [ ] `admin/src/pages/Dashboard.tsx`:
  - Cards: Total Memories, Active Users (24h), Searches Today, Top Tools
  - Activity timeline: últimas 20 acciones del audit log
  - Cada item muestra: avatar (iniciales), nombre, acción, tool, tiempo relativo
  - Refresh automático cada 30s con TanStack Query

- [ ] Componentes compartidos:
  - `StatCard` — número grande + label + trend icon
  - `ActivityItem` — avatar + texto + timestamp
  - `Badge` — para roles, tools, status

- [ ] PR #8: "feat: dashboard with org stats and activity timeline"

---

### Día 3 — User Management

- [ ] `admin/src/pages/Users.tsx`:
  - Tabla: Avatar | Nombre | Email | Rol | Status | API Key (truncada) | Acciones
  - Botón "Invite User" → modal con email + nombre + rol
  - Botón "Revoke Access" → confirma → `DELETE /v1/users/:id`
  - Botón "Rotate Key" → confirma → muestra nueva key (solo una vez)
  - Badge de status: active (verde), invited (amarillo), suspended (rojo)
  - Solo visible para rol `admin`

- [ ] Modal `InviteUserModal.tsx`:
  - Campos: Email, Name, Role (select: admin/member/viewer)
  - On success: muestra API key generada con botón "Copy"
  - Warning: "Esta key solo se muestra una vez"

- [ ] PR #9: "feat: user management with invite and revoke"

---

### Día 4 — Memory Browser

- [ ] `admin/src/pages/Memories.tsx`:
  - Filtros: Usuario (select), Tool (select), Proyecto (input), Fecha desde/hasta
  - Search input full-text (debounce 300ms → `POST /v1/memory/search`)
  - Tabla: Fecha | Usuario | Tool | Proyecto | Contenido (truncado) | Tags
  - Click en fila → modal con contenido completo
  - Botón "Export CSV" → descarga todas las memorias con filtros aplicados

- [ ] `MemoryDetailModal.tsx`:
  - Muestra contenido completo, tags, metadata
  - Botón "Delete" (solo admin)

- [ ] PR #10: "feat: memory browser with search and filters"

---

### Día 5 — Audit Log

- [ ] `admin/src/pages/AuditLog.tsx`:
  - Filtros: Usuario, Acción, Resource Type, From, To
  - Tabla: Timestamp | Usuario | Acción | Resource | Tool | Metadata
  - Acciones con color: `store` (azul), `search` (gris), `delete` (rojo), `invite` (verde), `revoke` (naranja)
  - Paginación: 50 items por página
  - Botón "Export CSV"

- [ ] PR #11: "feat: audit log with filters and export"

---

### Día 6 — Settings

- [ ] `admin/src/pages/Settings.tsx`:
  - Sección "Organization": nombre, slug (readonly), fecha creación
  - Botón "Save" → `PATCH /v1/admin/org`
  - Sección "My API Key": muestra key truncada + botón "Rotate"
  - Sección "Danger Zone" (solo admin): botón "Export All Data" (JSON)

- [ ] PR #12: "feat: settings page with org config"

---

### Día 7 — Polish + Responsive

- [ ] Layout principal: sidebar (desktop) / bottom nav (mobile)
- [ ] Sidebar con: logo, Dashboard, Users, Memories, Audit Log, Settings, Logout
- [ ] Dark mode toggle (guarda en localStorage)
- [ ] Empty states con copy útil ("No memories yet. Start using a tool connected to NexusMind.")
- [ ] Loading skeletons en todas las tablas
- [ ] Error boundaries en cada página
- [ ] PR #13: "feat: responsive layout, dark mode, and polish"

---

## Semana 3: Demo Mode + Docker

### Día 1 — Script Reset Demo

- [ ] `scripts/reset-demo.sh`:

```bash
#!/bin/bash
set -euo pipefail

echo "Resetting demo database..."
rm -f ./data/nexusmind.db
mkdir -p ./data

echo "Starting server briefly to run migrations..."
./target/release/nexusmind &
SERVER_PID=$!
sleep 1
kill $SERVER_PID

echo "Seeding demo data..."
./target/release/nexusmind-seed

echo "Demo ready!"
echo ""
echo "Organizations:"
echo "  Acme Corp    → admin key: nm_demo_acme_admin_xxx"
echo "  TechStartup  → admin key: nm_demo_tech_admin_xxx"
echo "  DevShop      → admin key: nm_demo_dev_admin_xxx"
echo ""
echo "Open: http://localhost:3000"
```

- [ ] `scripts/reset-demo.ps1` — equivalente para Windows
- [ ] PR #14: "feat: demo reset script"

---

### Día 2 — Datos de Ejemplo Realistas

- [ ] `src/bin/seed.rs` crea datos convincentes para la demo:

```
Acme Corp (5 usuarios):
  - Sarah Chen (admin) — usa Claude Code, Cursor
  - Marcus Johnson (member) — usa Claude Code
  - Ana García (member) — usa Cursor, GitHub Copilot
  - David Park (member) — usa Claude Code
  - Emma Wilson (viewer) — no usa tools

~80 memorias de Acme Corp con contenido como:
  - "Migrated auth from JWT to OAuth2 — see PR #234"
  - "Database connection pool set to 20 — was causing timeouts at 10"
  - "Use snake_case for all API endpoints per team convention"
  - "The payments service uses Stripe v3 API, not v2"
  ...con tags: ["auth", "performance", "convention", "payments"]

~30 audit events de los últimos 7 días
```

- [ ] Las API keys demo son deterministas (generadas desde seed fijo) para que el script siempre produzca las mismas keys
- [ ] PR #15: "feat: realistic demo seed data"

---

### Día 3 — Demo Script + Guía

- [ ] `demo/DEMO_SCRIPT.md`:

```markdown
# NexusMind Enterprise Demo — Guía paso a paso

**Duración estimada**: 8-10 minutos
**Audiencia**: CTO, VP Engineering, Compliance Officer

## Setup (antes de la llamada)
\`\`\`bash
docker compose up -d
./scripts/reset-demo.sh
\`\`\`

## Escena 1: Setup en 2 minutos (1 min)
"Miro, tu equipo puede estar corriendo esto en 2 comandos."
→ Mostrar terminal con `docker compose up -d`
→ Abrir http://localhost:3000

## Escena 2: Dashboard (2 min)
"Esta es tu organización. 5 developers activos, 89 memorias."
→ Dashboard de Acme Corp
→ Señalar: usuarios activos hoy, tools más usadas

## Escena 3: User Management (2 min)
"Cada developer tiene su propia API Key."
→ Users page: mostrar Sarah Chen, Marcus Johnson, Ana García
→ Demo: invitar nuevo usuario → copiar su key
→ Demo: revocar acceso → "Ya no puede hacer nada"

## Escena 4: Memory Browser (2 min)
"Toda la memoria del equipo, searchable."
→ Buscar "authentication" → resultados de 3 usuarios distintos
→ Filtrar por tool "Claude Code"
→ Abrir una memoria: contenido completo

## Escena 5: Audit Trail (1 min)
"Sabés exactamente qué pasó y cuándo."
→ Audit Log: "Ayer 14:32, Ana García guardó 'cambiar a OAuth2'"
→ Export CSV

## Escena 6: Cierre (1 min)
"¿Preguntas? Podés tenerlo corriendo en tu infra esta semana."
```

- [ ] PR #16: "docs: enterprise demo script"

---

### Día 4 — Landing Page Update

- [ ] `apps/landing/` — agregar sección "For Teams":
  - Headline: "Your team's AI memory, under control"
  - 3 bullets: Multi-user, Audit trail, Admin panel
  - CTA: "Book a Demo" → link a Calendly/email

- [ ] Badge: "Built with Rust" en el footer
- [ ] Screenshots del admin panel en la sección de features
- [ ] PR #17: "feat: landing page enterprise section"

---

### Día 5 — Docker Compose Optimizado

- [ ] `docker-compose.yml` final:

```yaml
version: "3.9"
services:
  backend:
    build: .
    ports:
      - "8080:8080"
    environment:
      DB_PATH: /data/nexusmind.db
      LOG_LEVEL: info
    volumes:
      - nexusmind_data:/data
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/v1/health"]
      interval: 10s
      timeout: 5s
      retries: 3

  admin:
    build: ./admin
    ports:
      - "3000:3000"
    environment:
      VITE_API_URL: http://backend:8080
    depends_on:
      backend:
        condition: service_healthy

volumes:
  nexusmind_data:
```

- [ ] `admin/Dockerfile` (nginx para servir el build de React)
- [ ] `Makefile`:

```makefile
.PHONY: dev build test reset-demo

dev:
	docker compose up

build:
	cargo build --release
	cd admin && npm run build

test:
	cargo test
	cd admin && npm run build  # type-check

reset-demo:
	./scripts/reset-demo.sh

demo:
	docker compose up -d && sleep 3 && ./scripts/reset-demo.sh
	@echo "Demo ready at http://localhost:3000"
```

- [ ] PR #18: "chore: docker compose and Makefile for demo"

---

### Días 6-7 — Bug Bash Enterprise

- [ ] Checklist demo flow:
  - [ ] `docker compose up -d` levanta sin errores
  - [ ] `./scripts/reset-demo.sh` crea datos y muestra keys
  - [ ] Login con admin key → Dashboard carga con stats reales
  - [ ] Users: listar, invitar, revocar
  - [ ] Invite flow: nueva key visible → copiar → login con esa key
  - [ ] Revoke: key revocada → 401 inmediato
  - [ ] Memories: buscar "authentication" → resultados de Acme Corp únicamente
  - [ ] Memories de org A **no aparecen** en sesión de org B (isolation check)
  - [ ] Audit Log: todas las acciones registradas correctamente
  - [ ] Export CSV desde Memories y desde Audit Log
  - [ ] Settings: cambiar nombre de org → persiste
  - [ ] Dark mode toggle funciona
  - [ ] Mobile responsive: sidebar colapsa correctamente

- [ ] PR #19: "fix: bug bash enterprise demo fixes"

---

## Semana 4: Polish, CI, Release v0.2.0

### Día 1 — README Enterprise

- [ ] `README.md` reescrito con foco enterprise:

```markdown
# NexusMind

Centralized memory control plane for AI-powered engineering teams.

## What it does
- Every developer on your team gets an API key
- Their AI tools (Claude Code, Cursor, Copilot) store decisions and context
- You see everything from a single admin panel
- Complete audit trail of who stored what, when

## Quickstart (2 commands)

\`\`\`bash
docker compose up -d
./scripts/reset-demo.sh   # loads demo data
\`\`\`

Open http://localhost:3000 and log in with the admin key printed by the script.

## Stack
Rust + SQLite + Axum (backend) — React + Vite + Tailwind (admin panel)
```

- [ ] PR #20: "docs: enterprise-focused README"

---

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

  admin:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: '20' }
      - run: cd admin && npm ci && npm run build

  e2e:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --release
      - run: ./scripts/test-e2e.sh
```

- [ ] `scripts/test-e2e.sh` — smoke test del flow enterprise:
  - Bootstrap org
  - Invite user
  - Store memory
  - Search memory (verifica isolation)
  - Audit log tiene entries

- [ ] PR #21: "chore: GitHub Actions CI with E2E smoke test"

---

### Días 3-4 — Screenshots + Video

- [ ] Screenshots del admin panel para pitch deck:
  - Dashboard con stats de Acme Corp
  - Users page con 5 miembros
  - Memory browser con resultados de "authentication"
  - Audit log con timeline
- [ ] `demo/screenshots/` — al menos 4 screenshots en 1440px de ancho
- [ ] Grabación de demo (Loom o similar) — opcional pero recomendado
- [ ] PR #22: "docs: demo screenshots and assets"

---

### Día 5 — Release v0.2.0

- [ ] Checklist final:
  - [ ] `cargo build --release` compila sin errores ni warnings
  - [ ] `docker compose up -d` levanta en 30s
  - [ ] Demo flow completo sin errores
  - [ ] CI pasa en GitHub Actions
  - [ ] README tiene quickstart que funciona
  - [ ] Screenshots en `demo/screenshots/`

- [ ] Tag y release:

```bash
git tag v0.2.0
git push --tags
```

- [ ] GitHub Release:
  - Título: "v0.2.0 — Enterprise Demo Ready"
  - Binary: `cargo build --release` (linux/amd64)
  - Changelog: multi-tenant, admin panel, audit trail, demo mode
  - Link al demo video (si existe)

- [ ] PR #23: "chore: release v0.2.0"

---

### Días 6-7 — Retro + Buffer

- [ ] Revisar métricas de la demo:
  - ¿Cuántas demos se hicieron?
  - ¿Cuál fue el feedback del flow?
  - ¿Qué preguntas hicieron los prospectos?
- [ ] Documentar friction points en `docs/mvp/07-POST_MVP.md`
- [ ] Decidir: ¿pricing, on-prem, o plugin MCP primero?
- [ ] Buffer para fixes urgentes post-release

---

## Estimación Total

| Componente | Días | PRs | Prioridad |
|---|---|---|---|
| Rust backend multi-tenant | 7 | #1–#6 | P0 |
| Admin panel React | 7 | #7–#13 | P0 |
| Demo mode + Docker | 7 | #14–#19 | P0 |
| CI/CD + README + Release | 7 | #20–#23 | P0 |
| **Total** | **28** | **23 PRs** | |

---

## Orden de Dependencias

```
Semana 1 (Backend) → debe completarse antes de Semana 2 (Admin Panel)
Semana 2 (Admin)   → puede empezar con mocks mientras el backend está en Días 4-7
Semana 3 (Demo)    → necesita backend + admin panel funcionando
Semana 4 (Release) → necesita todo
```

---

*Fin de 05-TASK_BREAKDOWN.md*
