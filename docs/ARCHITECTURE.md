# NexusMind — Arquitectura del Sistema

> **Documento**: ARCHITECTURE.md
> **Versión**: 1.0
> **Fecha**: Mayo 2026
> **Propósito**: Descripción detallada de la arquitectura, componentes, flujos de datos y decisiones técnicas de NexusMind.

---

## 1. Diagrama de Arquitectura General

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           ENTERPRISE LAYER                              │
│  ┌────────────┐ ┌──────────┐ ┌──────┐ ┌──────┐ ┌────────┐ ┌────────┐  │
│  │ Admin      │ │  RBAC   │ │Audit │ │ SSO │ │Billing │ │ Quotas│  │
│  │ Console    │ │ Engine  │ │Trail │ │      │ │        │ │       │  │
│  └────────────┘ └──────────┘ └──────┘ └──────┘ └────────┘ └────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
┌─────────────────────────────────────────────────────────────────────────┐
│                         ORCHESTRATION LAYER                             │
│  ┌────────────┐ ┌──────────────┐ ┌──────────────┐ ┌────────────────┐  │
│  │  Agent     │ │   Workflow   │ │    Task      │ │    State       │  │
│  │  Manager   │ │   Engine     │ │  Scheduler   │ │   Machine      │  │
│  └────────────┘ └──────────────┘ └──────────────┘ └────────────────┘  │
│  ┌────────────┐ ┌──────────────┐ ┌────────────────────────────────┐  │
│  │  Sub-agent │ │   Handoff    │ │  Result Aggregation Engine    │  │
│  │  Lifecycle │ │   Protocol   │ │                               │  │
│  └────────────┘ └──────────────┘ └────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
┌─────────────────────────────────────────────────────────────────────────┐
│                           MEMORY LAYER                                  │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌──────────────┐  │
│  │   SQLite     │ │  Vector DB   │ │   Episodic   │ │   Semantic   │  │
│  │   + FTS5     │ │(sqlite-vss /│ │   Memory     │ │   Memory     │  │
│  │              │ │  pgvector)  │ │   Store      │ │   Store      │  │
│  └──────────────┘ └──────────────┘ └──────────────┘ └──────────────┘  │
│  ┌──────────────┐ ┌──────────────────────────────────────────────┐  │
│  │   Auto-      │ │  Context Window Management                   │  │
│  │summarization │ │  (selective injection, sliding window)       │  │
│  └──────────────┘ └──────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
┌─────────────────────────────────────────────────────────────────────────┐
│                           GATEWAY LAYER                                 │
│  ┌────────────┐ ┌──────────┐ ┌──────┐ ┌───────────┐ ┌─────┐          │
│  │  MCP       │ │ HTTP API │ │ CLI  │ │WebSocket  │ │ SSE │          │
│  │  Server    │ │(REST)    │ │      │ │           │ │     │          │
│  └────────────┘ └──────────┘ └──────┘ └───────────┘ └─────┘          │
│  ┌────────────┐ ┌──────────┐ ┌──────────────────────────────────────┐ │
│  │  Model     │ │  Rate    │ │  Cache Layer (LRU + semantic)       │ │
│  │  Router    │ │  Limiter │ │                                     │ │
│  └────────────┘ └──────────┘ └──────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
┌─────────────────────────────────────────────────────────────────────────┐
│                          AGENT RUNTIME                                  │
│  ┌────────────┐ ┌──────────┐ ┌──────────┐ ┌────────────┐             │
│  │  Code      │ │  Chat    │ │  Data    │ │  Custom    │             │
│  │  Agent     │ │  Agent   │ │  Agent   │ │  Agent     │             │
│  └────────────┘ └──────────┘ └──────────┘ └────────────┘             │
│  ┌────────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────────────┐   │
│  │  Tool      │ │ Sandbox  │ │  File    │ │  Network Access      │   │
│  │  Executor  │ │ (gvisor) │ │  System  │ │  (allow-listed)      │   │
│  └────────────┘ └──────────┘ └──────────┘ └──────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Tech Stack

### 2.1 Backend (Go)

| Componente | Tecnología | Justificación |
|---|---|---|
| **Lenguaje** | Go 1.24+ | Rendimiento, concurrencia nativa, compilación rápida, ideal para tooling CLI |
| **HTTP Framework** | Chi / Gin | Ligero, middleware pipeline, performance |
| **Database** | SQLite (mattn/go-sqlite3) + PostgreSQL | SQLite para single-binary, PostgreSQL para multi-node |
| **FTS** | SQLite FTS5 | Full-text search nativo en SQLite |
| **Vector DB** | sqlite-vss (dev) → pgvector (prod) | Evolución gradual desde MVP a escala |
| **Queue** | NATS / Redis Streams | Mensajería ligera para eventos de agentes |
| **Auth** | OAuth2 + JWT + Casbin RBAC | Flexible, granular, estándar |
| **SSO** | SAML 2.0 via crewsaml / OIDC | Compatibilidad enterprise |
| **Embeddings** | text-embedding-3-small (OpenAI) + local alternatives | Balance costo/calidad |

### 2.2 Frontend (React)

| Componente | Tecnología | Justificación |
|---|---|---|
| **Framework** | React 19 + TypeScript | Ecosistema maduro, rendimiento |
| **Build Tool** | Vite | Dev experience superior, HMR rápido |
| **State** | Zustand + React Query | Simple, eficiente, cache optimista |
| **UI** | Tailwind CSS + Radix UI | Accesibilidad, diseño system-ready |
| **Rich Text** | TipTap / ProseMirror | Editor de prompts avanzado |
| **Charts** | Recharts / D3.js | Analytics dashboard |
| **SSE Client** | EventSource + fetch | Streaming de respuestas AI |

### 2.3 Infraestructura

| Componente | Tecnología |
|---|---|
| **Container** | Docker |
| **Orchestration** | Kubernetes (K3s para on-prem) |
| **CI/CD** | GitHub Actions |
| **Monitoring** | Prometheus + Grafana |
| **Logging** | OpenTelemetry + Loki |
| **Tracing** | Jaeger |
| **Deploy** | Helm Charts + Docker Compose (dev) |

---

## 3. Arquitectura por Capas (Detalle)

### 3.1 Gateway Layer

```
Cliente (IDE/CLI/Browser)
        │
        ▼
┌───────────────────────┐
│   Load Balancer       │  ← SSL termination, rate limiting global
└───────────┬───────────┘
            │
┌───────────▼───────────┐
│   API Gateway (Nginx) │  ← Routing, auth, rate limiting per-key
└───────────┬───────────┘
            │
     ┌──────┼──────┐
     ▼      ▼      ▼
┌──────┐┌──────┐┌──────┐
│HTTP  ││WS    ││MCP   │  ← Protocol handlers
│REST  ││      ││      │
└──┬───┘└──┬───┘└──┬───┘
   │       │       │
   └───────┼───────┘
           ▼
   ┌───────────────┐
   │  Middleware    │  ← Auth, logging, tracing, rate limit
   └───────┬───────┘
           ▼
   ┌───────────────┐
   │  Model Router │  ← Heuristic routing + fallback chain
   └───────┬───────┘
           ▼
   ┌───────────────┐
   │  Cache Layer   │  ← LRU cache + semantic dedup
   └───────────────┘
```

### 3.2 Memory Layer

```
┌──────────────────────────────────────────────────────────┐
│                  MEMORY SYSTEM ARCHITECTURE               │
│                                                          │
│  ┌───────────────┐                                       │
│  │  Memory API   │  ← gRPC + HTTP endpoints              │
│  └───────┬───────┘                                       │
│          │                                                │
│  ┌───────▼───────────────┐                               │
│  │  Memory Orchestrator  │  ← Routing to correct store    │
│  │  (decides: FTS?       │                                │
│  │   Vector? Hybrid?)    │                                │
│  └───────┬───────────────┘                               │
│          │                                                │
│     ┌────┼────────────┐                                   │
│     ▼    ▼            ▼                                   │
│  ┌────┐┌────┐    ┌───────┐                               │
│  │FTS5││VDB │    │Hybrid │  ← Weighted combination       │
│  └─┬──┘└─┬──┘    └───┬───┘                               │
│    │      │           │                                    │
│    ▼      ▼           ▼                                    │
│  ┌──────────────────────┐                                 │
│  │   Embedding Service  │  ← Batch/async embedding gen    │
│  │   (OpenAI + local)   │                                 │
│  └──────────────────────┘                                 │
│                                                          │
│  ┌──────────────────────┐                                 │
│  │   Summarizer         │  ← LLM-based chunk compression  │
│  │   (scheduled +       │                                 │
│  │    on-demand)        │                                 │
│  └──────────────────────┘                                 │
│                                                          │
│  Mem Stores:                                             │
│  ┌────────┐ ┌────────┐ ┌──────────────────┐            │
│  │Episodic│ │Semantic│ │Procedural        │            │
│  │(logs)  │ │(facts) │ │(prefs + config)  │            │
│  └────────┘ └────────┘ └──────────────────┘            │
└──────────────────────────────────────────────────────────┘
```

**Esquema de Tablas SQLite**:

```sql
-- Memoria Episódica: historial de interacciones
CREATE TABLE memory_episodic (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    role TEXT NOT NULL, -- 'user' | 'assistant' | 'system'
    content TEXT NOT NULL,
    tokens INTEGER,
    timestamp INTEGER NOT NULL,
    metadata JSON,
    parent_id TEXT, -- para threading
    FOREIGN KEY (parent_id) REFERENCES memory_episodic(id)
);

CREATE VIRTUAL TABLE memory_episodic_fts USING fts5(
    content, content=memory_episodic, content_rowid=rowid
);

-- Memoria Semántica: conocimiento extraído
CREATE TABLE memory_semantic (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    entity TEXT NOT NULL, -- concepto, persona, proyecto
    fact TEXT NOT NULL,
    confidence REAL DEFAULT 1.0,
    source TEXT, -- qué interacción generó esto
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    embedding BLOB, -- vector embedding
    metadata JSON
);

CREATE INDEX idx_semantic_user_entity ON memory_semantic(user_id, entity);

-- Memoria Procedural: preferencias y config
CREATE TABLE memory_procedural (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    type TEXT NOT NULL, -- 'preference' | 'config' | 'pattern'
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(user_id, key)
);
```

### 3.3 Orchestration Layer

```
┌────────────────────────────────────────────────────────┐
│              WORKFLOW ENGINE                            │
│                                                        │
│  ┌──────────────────────────────────────────┐         │
│  │              DAG Definition               │         │
│  │                                            │         │
│  │    ┌───┐     ┌───┐                        │         │
│  │    │ A │────▶│ B │────▶┐                  │         │
│  │    └───┘     └───┘     │    ┌───┐         │         │
│  │                │       ├───▶│ D │──▶ Done │         │
│  │                ▼       │    └───┘         │         │
│  │    ┌───┐     ┌───┐     │                  │         │
│  │    │ C │────▶│ E │────▶┘                  │         │
│  │    └───┘     └───┘                        │         │
│  └──────────────────────────────────────────┘         │
│                                                        │
│  Ejecución:                                            │
│  1. A → (B || C) en paralelo                          │
│  2. B → E, C → E                                      │
│  3. (B && C) → E, B → D                               │
│  4. (D && E) → Done                                   │
│                                                        │
│  Cada nodo = agente/herramienta con:                   │
│  - Input schema (JSON Schema)                          │
│  - Output schema                                       │
│  - Timeout configurable                                │
│  - Retry policy                                        │
│  - Error handler                                       │
└────────────────────────────────────────────────────────┘
```

### 3.4 Agent Runtime

```
┌────────────────────────────────────────────────────────┐
│                  AGENT SANDBOX                           │
│                                                        │
│  ┌────────────────────────────────────────────┐        │
│  │  gVisor / runc container                    │        │
│  │                                            │        │
│  │  ┌──────────┐  ┌──────────┐  ┌─────────┐ │        │
│  │  │ Python   │  │  Node    │  │   Go    │ │        │
│  │  │ Runtime  │  │ Runtime  │  │ Runtime │ │        │
│  │  └──────────┘  └──────────┘  └─────────┘ │        │
│  │                                            │        │
│  │  Network policy: allow-listed only          │        │
│  │  File system: ephemeral + bind mounts      │        │
│  │  Memory limit: 512MB                       │        │
│  │  CPU limit: 1 core                         │        │
│  │  Timeout: 60s (default)                    │        │
│  └────────────────────────────────────────────┘        │
│                                                        │
│  Tools disponibles:                                    │
│  ┌────────────┐ ┌────────────┐ ┌──────────────┐      │
│  │ File Read  │ │ File Write │ │ Shell Exec   │      │
│  │ & Write    │ │ (sandbox)  │ │ (sandbox)    │      │
│  └────────────┘ └────────────┘ └──────────────┘      │
│  ┌────────────┐ ┌────────────┐ ┌──────────────┐      │
│  │ Web Search │ │ HTTP Fetch │ │ DB Query     │      │
│  │ (allowlist)│ │(allowlist) │ │ (config db)  │      │
│  └────────────┘ └────────────┘ └──────────────┘      │
│  ┌────────────┐ ┌────────────┐ ┌──────────────┐      │
│  │ Memory     │ │ Agent      │ │ Notification │      │
│  │ Access     │ │ Spawn      │ │ (webhook)    │      │
│  └────────────┘ └────────────┘ └──────────────┘      │
└────────────────────────────────────────────────────────┘
```

---

## 4. Flujos de Datos Principales

### 4.1 Chat con Memoria

```
User: "¿Qué estábamos haciendo con el módulo de pagos?"
        │
        ▼
  ┌─────────────┐
  │ HTTP API    │  ← POST /v1/agents/run
  └──────┬──────┘
         │
  ┌──────▼──────┐
  │  Model      │  ← Decide: este request necesita memoria
  │  Router     │     Busca en episódica y semántica
  └──────┬──────┘
         │
  ┌──────▼──────┐
  │  Memory     │  ← Search("módulo de pagos")
  │  System     │     1. FTS5 sobre memory_episodic
  │             │     2. Vector search sobre memory_semantic
  │             │     3. Hybrid rank → top 5 resultados
  └──────┬──────┘
         │ context
  ┌──────▼──────┐
  │  LLM Call   │  ← Prompt = user_query + context_history
  └──────┬──────┘
         │ response
  ┌──────▼──────┐
  │  Save       │  ← memory_episodic += response
  │  Memory     │     memory_semantic.update(pagos → facts)
  └──────┬──────┘
         │
  ┌──────▼──────┐
  │  SSE Stream │  ← Response streamed to user
  └─────────────┘
```

### 4.2 Ejecución de Workflow Multi-Agente

```
User: "Analiza los últimos 30 días de logs, encuentra errores,
       sugiere fixes y genera un reporte"
        │
        ▼
  ┌─────────────────┐
  │ Orchestrator    │  ← Descompone en DAG:
  │                 │     1. DataAgent → query logs (30d)
  │                 │     2. CodeAgent → analiza errores
  │                 │     3. CodeAgent → sugiere fixes
  │                 │     4. DataAgent → genera reporte
  └────────┬────────┘
           │
     ┌─────▼─────┐
     │  Workflow │  ← Crea instancia del DAG
     │  Engine   │
     └─────┬─────┘
           │
     ┌─────▼─────┐
     │ Execute   │  ← A→ paralelo (B||C) → D
     │ DAG       │     Timeout: 300s total
     └─────┬─────┘
           │
     ┌─────▼─────┐
     │ Aggregate │  ← Merge outputs, resolve conflicts
     │ Results   │
     └─────┬─────┘
           │ final_response
     ┌─────▼─────┐
     │ Return to │  ← Full report con errores + fixes
     │ User      │
     └───────────┘
```

---

## 5. Consideraciones de Seguridad

### 5.1 Sandbox de Ejecución de Código

```
┌─────────────────────────────────────────┐
│           Sandbox Security Model        │
│                                         │
│  ● gVisor container sin acceso a host   │
│  ● Network: allow-list de dominios      │
│  ● Filesystem: efímero + bind /tmp      │
│  ● CPU/Memoria limitados por agente     │
│  ● Tiempo máximo de ejecución           │
│  ● No device access (/dev/null only)    │
│  ● No capabilities (cap_drop all)       │
│  ● Seccomp profile restrictivo          │
│  ● Read-only root filesystem            │
│  ● No privileged mode                   │
└─────────────────────────────────────────┘
```

### 5.2 Data Security

| Aspecto | Implementación |
|---|---|
| **Encryption at rest** | SQLite WAL encryption (AES-256-CBC) |
| **Encryption in transit** | TLS 1.3 mínimo, mTLS entre servicios |
| **Key management** | Vault / AWS KMS (rotación automática) |
| **Data isolation** | Por tenant: database prefix + row-level security |
| **Audit trail** | Tabla append-only firmada (HMAC chain) |

---

## 6. Escalabilidad

### 6.1 Single-Node (MVP/Dev)

```
┌───────────────────────────────────┐
│           nexusmind binary        │
│                                   │
│  ┌─────────┐ ┌──────────────────┐ │
│  │  HTTP   │ │   SQLite         │ │
│  │  Server │ │   (embedded)     │ │
│  └─────────┘ └──────────────────┘ │
│  ┌─────────┐ ┌──────────────────┐ │
│  │  Agent  │ │   File Store     │ │
│  │  Runner │ │   (local FS)     │ │
│  └─────────┘ └──────────────────┘ │
└───────────────────────────────────┘
```

### 6.2 Multi-Node (Producción)

```
                     ┌──────────┐
                     │  Load    │
                     │ Balancer │
                     └────┬─────┘
                          │
          ┌───────────────┼───────────────┐
          │               │               │
     ┌────▼────┐    ┌────▼────┐    ┌────▼────┐
     │ nexusmind│    │ nexusmind│    │ nexusmind│
     │ node 1  │    │ node 2  │    │ node N  │
     └────┬────┘    └────┬────┘    └────┬────┘
          │               │               │
          └───────────────┼───────────────┘
                          │
                    ┌─────▼──────┐
                    │ PostgreSQL │
                    │ + pgvector │
                    └─────┬──────┘
                          │
                    ┌─────▼──────┐
                    │   Redis    │
                    │ (cache +   │
                    │  queue)    │
                    └────────────┘
```

---

## 7. APIs Internas (gRPC entre servicios)

| Servicio | Método | Descripción |
|---|---|---|
| `MemoryService` | `Save(ctx, Entry) → ID` | Guardar entrada de memoria |
| `MemoryService` | `Search(ctx, Query) → Results` | Búsqueda híbrida |
| `MemoryService` | `Summarize(ctx, SessionID) → Summary` | Resumir sesión |
| `AgentService` | `Run(ctx, Request) → Stream<Event>` | Ejecutar agente |
| `AgentService` | `Spawn(ctx, Spec) → AgentID` | Crear sub-agente |
| `AgentService` | `Status(ctx, AgentID) → Status` | Estado del agente |
| `WorkflowService` | `Execute(ctx, DAG) → Result` | Ejecutar DAG |
| `WorkflowService` | `Status(ctx, WorkflowID) → Status` | Estado del workflow |
| `AdminService` | `GetAuditLogs(ctx, Filter) → Logs` | Audit trail |
| `AdminService` | `InviteUser(ctx, Invite) → Result` | Invitar usuario |

---

## 8. Decisiones Arquitecturales Clave (ADRs)

### ADR-001: Go como lenguaje principal
**Contexto**: Necesitamos un lenguaje rápido, concurrente, con buen soporte CLI y single-binary deployable.
**Decisión**: Go 1.24+
**Consecuencias**: Ecosistema menor que Python/JS para AI/ML, pero superior para tooling, APIs y CLI.

### ADR-002: SQLite-first, PostgreSQL on demand
**Contexto**: MVP necesita ser single-binary, simple de deployar, sin dependencias externas.
**Decisión**: SQLite para MVP, PostgreSQL+pgvector para producción multi-node.
**Consecuencias**: Migración de datos necesaria al escalar. Schema diseñado para ambos desde el día 1.

### ADR-003: MCP Server como protocolo primario de agente
**Contexto**: Necesitamos un protocolo estándar para comunicación agente-herramienta.
**Decisión**: Implementar servidor MCP (Model Context Protocol) como interfaz principal.
**Consecuencias**: Interoperabilidad con ecosistema MCP existente. Menos control que protocolo propio.

### ADR-004: Embeddings local-first, cloud como fallback
**Contexto**: Costo de APIs de embeddings escala con el uso. Privacidad de datos importante.
**Decisión**: Usar modelos locales (all-MiniLM-L6-v2) por defecto, API cloud como fallback.
**Consecuencias**: Menor calidad de embeddings inicial, pero sin dependencia externa y costo predecible.

---

*Fin de ARCHITECTURE.md*
