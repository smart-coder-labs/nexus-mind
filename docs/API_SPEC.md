# NexusMind — API Specification

> **Documento**: API_SPEC.md  
> **Versión**: 1.0  
> **Fecha**: Mayo 2026  
> **Propósito**: Especificación completa de la API REST de NexusMind.

---

## 1. Base URL

```
Producción:  https://api.nexusmind.ai/v1
Staging:     https://api-staging.nexusmind.ai/v1
Local:       http://localhost:8080/v1
```

---

## 2. Authentication

### 2.1 Métodos

| Método | Header | Uso |
|---|---|---|
| **API Key** | `Authorization: Bearer <api_key>` | Machine-to-machine, CI/CD |
| **JWT** | `Authorization: Bearer <jwt_token>` | User sessions (web app) |
| **OAuth2** | `Authorization: Bearer <oauth_token>` | SSO integrations |

### 2.2 API Keys

```json
POST /v1/admin/api-keys
Content-Type: application/json
Authorization: Bearer <admin_token>

{
  "name": "CI/CD Pipeline",
  "scopes": ["agents:run", "memory:write"],
  "expires_in": "90d"
}

Response 201:
{
  "id": "ak_abc123",
  "key": "nxm_sk_live_xxxxxxxxxxxxx",
  "name": "CI/CD Pipeline",
  "scopes": ["agents:run", "memory:write"],
  "created_at": "2026-05-10T14:00:00Z",
  "expires_at": "2026-08-08T14:00:00Z"
}
```

**Nota**: La key completa solo se muestra una vez al crearla.

### 2.3 Scopes Disponibles

| Scope | Permisos |
|---|---|
| `memory:read` | Leer memoria |
| `memory:write` | Guardar/actualizar memoria |
| `agents:run` | Ejecutar agentes |
| `agents:spawn` | Crear sub-agentes |
| `agents:admin` | CRUD de agentes |
| `workflows:execute` | Ejecutar workflows |
| `workflows:admin` | CRUD de workflows |
| `sessions:manage` | Gestionar sesiones |
| `admin:read` | Leer datos de admin |
| `admin:write` | Modificar configuración |

---

## 3. Endpoints

### 3.1 Memory System

#### Save Memory

```
POST /v1/memory/save
```

**Description**: Guarda una entrada en el sistema de memoria.

**Request**:
```json
{
  "content": "The authentication service uses JWT with RS256. Keys are rotated every 90 days.",
  "type": "semantic",
  "entity": "authentication",
  "session_id": "sess_abc123",
  "metadata": {
    "project": "nexusmind-core",
    "confidence": 0.95
  }
}
```

**Response**:
```json
{
  "id": "mem_xyz789",
  "type": "semantic",
  "tokens": 15,
  "embedding_status": "completed",
  "created_at": "2026-05-10T14:00:00Z"
}
```

**Parameters**:

| Field | Type | Required | Description |
|---|---|---|---|
| `content` | string | yes | Content to memorize |
| `type` | enum | yes | `episodic`, `semantic`, `procedural` |
| `entity` | string | no | Entity/concept this relates to |
| `session_id` | string | no | Session context |
| `metadata` | object | no | Custom metadata (max 10 keys) |

---

#### Search Memory

```
GET /v1/memory/search?q=authentication+JWT&type=semantic&limit=5&offset=0
```

**Description**: Búsqueda híbrida (FTS + vector) sobre la memoria del usuario.

**Query Parameters**:

| Parameter | Type | Default | Description |
|---|---|---|---|
| `q` | string | — | **Required**. Search query |
| `type` | enum | `all` | `episodic`, `semantic`, `procedural`, `all` |
| `entity` | string | — | Filter by entity |
| `session_id` | string | — | Filter by session |
| `limit` | int | `10` | Max results (1-50) |
| `offset` | int | `0` | Pagination offset |
| `mode` | enum | `hybrid` | `fts`, `vector`, `hybrid` |
| `from` | ISO8601 | — | Date range start |
| `to` | ISO8601 | — | Date range end |
| `min_score` | float | `0.0` | Minimum relevance score |

**Response**:
```json
{
  "results": [
    {
      "id": "mem_xyz789",
      "content": "The authentication service uses JWT with RS256...",
      "type": "semantic",
      "entity": "authentication",
      "score": 0.92,
      "fts_score": 8.5,
      "vector_score": 0.89,
      "created_at": "2026-05-10T14:00:00Z",
      "session_id": "sess_abc123"
    }
  ],
  "total": 42,
  "limit": 10,
  "offset": 0,
  "mode": "hybrid"
}
```

---

#### Get Timeline

```
GET /v1/memory/timeline?from=2026-04-01&to=2026-05-10&entity=auth
```

**Response**:
```json
{
  "events": [
    {
      "date": "2026-05-10",
      "entries": [
        {
          "id": "mem_xyz789",
          "summary": "Documented JWT RS256 key rotation policy",
          "type": "semantic"
        }
      ]
    }
  ],
  "total_events": 15,
  "date_range": {
    "from": "2026-04-01",
    "to": "2026-05-10"
  }
}
```

---

#### Memory Stats

```
GET /v1/memory/stats
```

**Response**:
```json
{
  "total_entries": 15230,
  "episodic": 14200,
  "semantic": 980,
  "procedural": 50,
  "storage_bytes": 45875200,
  "avg_entry_size": 3012,
  "last_week_activity": 2340,
  "top_entities": [
    {"entity": "authentication", "count": 120},
    {"entity": "database", "count": 95}
  ]
}
```

---

### 3.2 Agents

#### Run Agent

```
POST /v1/agents/run
```

**Description**: Ejecuta un agente con un prompt y contexto opcional.

**Request**:
```json
{
  "prompt": "Analiza los logs del servidor y encuentra patrones de error",
  "model": "claude-sonnet-4",
  "system_prompt": "Eres un analista de sistemas experto.",
  "tools": ["file_read", "web_search", "shell_exec"],
  "context": {
    "session_id": "sess_abc123",
    "memory_mode": "auto"
  },
  "stream": true,
  "max_tokens": 4096,
  "temperature": 0.3
}
```

**Response (stream: false)**:
```json
{
  "id": "agent_run_456",
  "status": "completed",
  "output": "Se encontraron 3 patrones de error...",
  "tool_calls": [
    {
      "tool": "file_read",
      "input": {"path": "/var/log/server.log"},
      "output": "[truncated 15KB of logs]",
      "duration_ms": 12
    }
  ],
  "tokens_used": {
    "input": 1542,
    "output": 890,
    "total": 2432
  },
  "model": "claude-sonnet-4",
  "duration_ms": 3450,
  "memory_saved": true
}
```

**Response (stream: true)** — SSE Stream:
```
event: token
data: {"token": "Se", "index": 0}

event: token
data: {"token": " encontraron", "index": 1}

event: tool_call
data: {"tool": "file_read", "status": "running"}

event: tool_result
data: {"tool": "file_read", "status": "completed", "duration_ms": 12}

event: complete
data: {
  "id": "agent_run_456",
  "status": "completed",
  "tokens_used": {"input": 1542, "output": 890, "total": 2432},
  "duration_ms": 3450,
  "tool_calls_count": 3
}
```

---

#### Spawn Sub-agent

```
POST /v1/agents/spawn
```

**Description**: Crea un sub-agente dentro de la ejecución de un agente padre.

**Request**:
```json
{
  "parent_run_id": "agent_run_456",
  "name": "log-parser",
  "prompt": "Extrae los errores HTTP 5xx del archivo de logs",
  "model": "gpt-4o-mini",
  "tools": ["file_read", "grep"],
  "timeout": 60,
  "inherit_context": true
}
```

**Response**:
```json
{
  "id": "subagent_789",
  "parent_run_id": "agent_run_456",
  "status": "running",
  "created_at": "2026-05-10T14:00:05Z",
  "estimated_completion": "2026-05-10T14:01:05Z"
}
```

---

#### Get Agent Status

```
GET /v1/agents/:id/status
```

**Response**:
```json
{
  "id": "agent_run_456",
  "status": "running",
  "progress": {
    "current_step": "analyze_errors",
    "total_steps": 4,
    "completed_steps": 2,
    "elapsed_seconds": 45
  },
  "sub_agents": [
    {"id": "subagent_789", "status": "completed"},
    {"id": "subagent_790", "status": "running"}
  ],
  "tools_called": 3,
  "tokens_so_far": 1234
}
```

---

#### List Agents (Definitions)

```
GET /v1/agents?scope=team&limit=20
```

**Response**:
```json
{
  "agents": [
    {
      "id": "agent_code",
      "name": "Code Assistant",
      "description": "AI coding assistant with full tool access",
      "model": "auto",
      "tools": ["file_read", "file_write", "shell_exec", "web_search"],
      "created_by": "user_123",
      "created_at": "2026-04-01T00:00:00Z",
      "updated_at": "2026-05-01T00:00:00Z",
      "usage_count": 1423
    }
  ],
  "total": 5,
  "limit": 20
}
```

---

### 3.3 Workflows

#### Execute Workflow

```
POST /v1/workflows/execute
```

**Description**: Ejecuta un workflow DAG.

**Request**:
```json
{
  "workflow_id": "wf_log_analysis",
  "input": {
    "log_source": "cloudwatch",
    "duration_days": 30,
    "output_format": "markdown"
  },
  "timeout": 300,
  "notify_on_complete": true
}
```

**Response**:
```json
{
  "id": "wf_run_123",
  "status": "running",
  "created_at": "2026-05-10T14:00:00Z",
  "estimated_completion": "2026-05-10T14:05:00Z",
  "node_count": 4
}
```

#### Get Workflow Status

```
GET /v1/workflows/:id/status
```

**Response**:
```json
{
  "id": "wf_run_123",
  "workflow_id": "wf_log_analysis",
  "status": "running",
  "progress": {
    "completed_nodes": 2,
    "total_nodes": 4,
    "failed_nodes": 1,
    "running_nodes": 1,
    "pending_nodes": 1
  },
  "nodes": [
    {"id": "query_logs", "status": "completed", "duration_ms": 2300},
    {"id": "analyze_errors", "status": "completed", "duration_ms": 4500},
    {"id": "suggest_fixes", "status": "failed", "error": "Model timeout", "duration_ms": 65000},
    {"id": "generate_report", "status": "pending"}
  ],
  "started_at": "2026-05-10T14:00:00Z"
}
```

#### Define Workflow

```
POST /v1/workflows
```

**Request**:
```json
{
  "name": "Log Analysis Pipeline",
  "version": "1.0",
  "description": "Analiza logs, encuentra errores y sugiere fixes",
  "nodes": [
    {
      "id": "query_logs",
      "agent": "data_agent",
      "input_schema": {
        "type": "object",
        "properties": {
          "source": {"type": "string"},
          "duration": {"type": "integer"}
        }
      },
      "timeout": 120
    },
    {
      "id": "analyze_errors",
      "depends_on": ["query_logs"],
      "agent": "code_agent",
      "input_schema": {},
      "timeout": 60,
      "retry": {"max_attempts": 2, "backoff": "exponential"}
    }
  ],
  "output_schema": {
    "type": "object",
    "properties": {
      "errors": {"type": "array"},
      "fixes": {"type": "array"},
      "report": {"type": "string"}
    }
  }
}
```

---

### 3.4 Sessions

#### Start Session

```
POST /v1/sessions/start
```

**Request**:
```json
{
  "project": "nexusmind-core",
  "context": {
    "branch": "feature/memory-system",
    "task": "Implement hybrid search"
  },
  "inherit_memory_from": ["sess_previous_id"]
}
```

**Response**:
```json
{
  "id": "sess_new_123",
  "status": "active",
  "started_at": "2026-05-10T14:00:00Z",
  "memory_count": 42,
  "context_size_tokens": 3200
}
```

#### End Session

```
POST /v1/sessions/:id/end
```

**Response**:
```json
{
  "id": "sess_abc123",
  "status": "completed",
  "duration_seconds": 3600,
  "memory_count": 15,
  "auto_summary": "En esta sesión se implementó...",
  "summary_tokens": 350
}
```

---

### 3.5 Enterprise Admin

#### List Users

```
GET /v1/admin/users?role=developer&status=active&team=engineering&limit=20&offset=0
```

**Response**:
```json
{
  "users": [
    {
      "id": "user_123",
      "email": "alice@company.com",
      "name": "Alice Martínez",
      "role": "developer",
      "team": "engineering",
      "status": "active",
      "last_active": "2026-05-10T12:00:00Z",
      "created_at": "2026-03-01T00:00:00Z",
      "usage_this_month": {
        "agents_run": 145,
        "tokens_used": 250000,
        "memory_entries": 320
      }
    }
  ],
  "total": 45,
  "page": 1
}
```

#### Get Audit Logs

```
GET /v1/admin/audit?user=user_123&action=agent.run&from=2026-05-01&to=2026-05-10&limit=50
```

**Response**:
```json
{
  "entries": [
    {
      "id": "audit_001",
      "timestamp": "2026-05-10T14:00:00Z",
      "user_id": "user_123",
      "user_email": "alice@company.com",
      "action": "agent.run",
      "resource": "agents/code_assistant",
      "result": "success",
      "details": {
        "agent_id": "agent_run_456",
        "model": "claude-sonnet-4",
        "tokens_used": 2432,
        "duration_ms": 3450
      },
      "ip": "192.168.1.100",
      "prev_hash": "abc...",
      "signature": "def..."
    }
  ],
  "total": 120,
  "limit": 50,
  "page": 1
}
```

#### Verify Audit Chain

```
GET /v1/admin/audit/verify?from=2026-05-01&to=2026-05-10
```

**Response**:
```json
{
  "status": "verified",
  "entries_checked": 120,
  "tampered_entries": 0,
  "chain_integrity": true,
  "verification": "HMAC-SHA256 chain intact desde entry aud_001 hasta aud_120"
}
```

#### Invite User

```
POST /v1/admin/users/invite
```

**Request**:
```json
{
  "email": "bob@company.com",
  "name": "Bob Johnson",
  "role": "developer",
  "team": "engineering",
  "send_invite_email": true
}
```

**Response**:
```json
{
  "id": "user_456",
  "email": "bob@company.com",
  "status": "invited",
  "invited_at": "2026-05-10T14:00:00Z",
  "invite_expires": "2026-05-17T14:00:00Z"
}
```

---

### 3.6 Billing

#### Get Usage Report

```
GET /v1/admin/billing/usage?period=monthly&year=2026&month=4
```

**Response**:
```json
{
  "period": "2026-04",
  "total_users": 45,
  "active_users": 32,
  "total_tokens": 12500000,
  "tokens_by_model": {
    "gpt-4o": 5000000,
    "claude-sonnet-4": 4500000,
    "gpt-4o-mini": 2000000,
    "gemini-pro": 1000000
  },
  "estimated_cost_usd": 125.50,
  "agents_executed": 3400,
  "workflows_executed": 120,
  "storage_gb": 2.5
}
```

---

## 4. Error Handling

### 4.1 Error Response Format

```json
{
  "error": {
    "code": "RATE_LIMIT_EXCEEDED",
    "message": "Has excedido el límite de requests. Intenta de nuevo en 30 segundos.",
    "details": {
      "limit": 100,
      "window_seconds": 60,
      "retry_after": 30
    },
    "request_id": "req_abc123",
    "documentation_url": "https://docs.nexusmind.ai/api/errors#rate-limit"
  }
}
```

### 4.2 Códigos de Error

| HTTP Status | Code | Description |
|---|---|---|
| 400 | `INVALID_INPUT` | Request validation failed |
| 401 | `UNAUTHORIZED` | Missing or invalid auth |
| 403 | `FORBIDDEN` | Scope insufficient |
| 404 | `NOT_FOUND` | Resource not found |
| 409 | `CONFLICT` | Resource conflict |
| 422 | `UNPROCESSABLE` | Semantic validation error |
| 429 | `RATE_LIMIT_EXCEEDED` | Rate limit hit |
| 500 | `INTERNAL_ERROR` | Server error |
| 502 | `UPSTREAM_FAILURE` | LLM provider error |
| 503 | `SERVICE_UNAVAILABLE` | Temporary maintenance |
| 504 | `GATEWAY_TIMEOUT` | LLM timeout |

### 4.3 Rate Limiting

```
Headers en cada response:
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 87
X-RateLimit-Reset: 1700000000
```

| Tier | Requests/min | Burst |
|---|---|---|
| Developer | 60 | 100 |
| Team | 200 | 300 |
| Enterprise | Custom | Custom |

---

## 5. Pagination

Todos los endpoints de listado usan paginación cursor o offset:

```json
{
  "data": [...],
  "pagination": {
    "next_cursor": "eyJvZmZzZXQiOjEwfQ==",
    "prev_cursor": null,
    "total": 142,
    "limit": 10
  }
}
```

---

## 6. WebSocket Events

```
wss://api.nexusmind.ai/v1/ws?token=<jwt_or_api_key>
```

### Events

| Event | Direction | Description |
|---|---|---|
| `agent:progress` | Server→Client | Agent execution progress update |
| `agent:tool_call` | Server→Client | Agent called a tool |
| `agent:complete` | Server→Client | Agent execution completed |
| `agent:error` | Server→Client | Agent execution error |
| `memory:updated` | Server→Client | Memory was saved/updated |
| `session:state` | Server→Client | Session state changed |
| `admin:alert` | Server→Client | Admin alert (usage threshold, error rate) |

---

## 7. CLI Reference

```bash
# Core
nexusmind init                    # Initialize config
nexusmind start                   # Start server
nexusmind version                 # Show version

# Memory
nexusmind memory save "content" --type semantic --entity auth
nexusmind memory search "query" --limit 10 --mode hybrid
nexusmind memory timeline --from 2026-04-01

# Agents
nexusmind agent run "prompt" --model claude-sonnet-4 --stream
nexusmind agent list
nexusmind agent status <id>

# Workflows
nexusmind workflow execute <workflow_id> --input '{"key": "value"}'
nexusmind workflow status <id>
nexusmind workflow list

# Sessions
nexusmind session start --project my-project
nexusmind session end
nexusmind session list

# Admin
nexusmind admin users list
nexusmind admin audit --action agent.run --days 7
nexusmind admin billing usage --monthly

# Config
nexusmind config set api_key <key>
nexusmind config set default_model gpt-4o
nexusmind config show
```

---

## 8. MCP Server Tools

El servidor MCP expone las mismas capacidades como tools MCP estándar:

```json
{
  "tools": [
    {
      "name": "mem_save",
      "description": "Save an entry to persistent memory",
      "inputSchema": {
        "type": "object",
        "properties": {
          "content": {"type": "string"},
          "type": {"type": "string", "enum": ["episodic", "semantic", "procedural"]},
          "entity": {"type": "string"}
        },
        "required": ["content", "type"]
      }
    },
    {
      "name": "mem_search",
      "description": "Search memory with hybrid FTS + vector search",
      "inputSchema": {
        "type": "object",
        "properties": {
          "query": {"type": "string"},
          "limit": {"type": "integer"},
          "mode": {"type": "string", "enum": ["fts", "vector", "hybrid"]}
        },
        "required": ["query"]
      }
    },
    {
      "name": "agent_run",
      "description": "Run an agent with a prompt",
      "inputSchema": {
        "type": "object",
        "properties": {
          "prompt": {"type": "string"},
          "model": {"type": "string"},
          "tools": {"type": "array", "items": {"type": "string"}}
        },
        "required": ["prompt"]
      }
    },
    {
      "name": "agent_spawn",
      "description": "Spawn a sub-agent",
      "inputSchema": {
        "type": "object",
        "properties": {
          "parent_run_id": {"type": "string"},
          "prompt": {"type": "string"},
          "timeout": {"type": "integer"}
        },
        "required": ["parent_run_id", "prompt"]
      }
    },
    {
      "name": "session_start",
      "description": "Start a new session",
      "inputSchema": {
        "type": "object",
        "properties": {
          "project": {"type": "string"},
          "context": {"type": "object"}
        }
      }
    },
    {
      "name": "session_end",
      "description": "End current session",
      "inputSchema": {
        "type": "object",
        "properties": {}
      }
    }
  ]
}
```

---

## 9. SDK Examples

### Python

```python
from nexusmind import NexusMind

client = NexusMind(api_key="nxm_sk_live_xxx")

# Save memory
client.memory.save(
    content="The API uses Bearer token authentication",
    type="semantic", 
    entity="authentication"
)

# Search memory
results = client.memory.search("authentication tokens", limit=5)
for r in results:
    print(f"[{r.score:.2f}] {r.content}")

# Run agent
response = client.agents.run(
    prompt="Explain the architecture to me",
    stream=True
)
for token in response:
    print(token, end="")
```

### JavaScript

```javascript
import { NexusMind } from '@nexusmind/sdk';

const client = new NexusMind({
  apiKey: 'nxm_sk_live_xxx'
});

// Run agent with streaming
const stream = await client.agents.run({
  prompt: 'Analyze this codebase',
  model: 'claude-sonnet-4',
  stream: true
});

for await (const chunk of stream) {
  process.stdout.write(chunk.token);
}
```

### Go

```go
package main

import (
    "context"
    "fmt"
    "github.com/nexusmind/sdk-go"
)

func main() {
    client := nexusmind.New("nxm_sk_live_xxx")
    
    ctx := context.Background()
    
    // Save memory
    err := client.Memory.Save(ctx, &nexusmind.MemoryEntry{
        Content: "Use Go 1.24 for the backend",
        Type:    "semantic",
        Entity:  "tech-stack",
    })
    
    // Search with hybrid mode
    results, err := client.Memory.Search(ctx, &nexusmind.SearchQuery{
        Query: "Go backend",
        Mode:  "hybrid",
        Limit: 10,
    })
    
    for _, r := range results {
        fmt.Printf("[%.2f] %s\n", r.Score, r.Content)
    }
}
```

---

## 10. Webhook Events (Enterprise)

```
POST /v1/admin/webhooks
```

```json
{
  "name": "Usage Alerts",
  "url": "https://hooks.company.com/nexusmind",
  "events": ["agent.completed", "agent.failed", "usage.threshold"],
  "secret": "whsec_xxx",
  "active": true
}
```

**Payload**:
```json
{
  "event": "usage.threshold",
  "timestamp": "2026-05-10T14:00:00Z",
  "organization_id": "org_123",
  "data": {
    "metric": "tokens_used",
    "current": 12500000,
    "threshold": 10000000,
    "period": "2026-05"
  }
}
```

---

*Fin de API_SPEC.md*
