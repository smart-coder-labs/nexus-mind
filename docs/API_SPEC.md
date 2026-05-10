# NexusMind — API Specification

> **Documento**: API_SPEC.md
> **Versión**: 2.0
> **Fecha**: Mayo 2026
> **Propósito**: API del control plane. Endpoints MCP + REST para que cualquier herramienta AI se integre.

---

## 1. API Philosophy

- **MCP (Model Context Protocol) primero** — Compatibilidad con el estándar emergente de Anthropic
- **REST API paralela** — Para herramientas que no soporten MCP
- **Sin dependencia de LLM** — La API funciona sin modelos. BYOM es responsabilidad del cliente.
- **Rate limiting por API key** — Control de uso por herramienta/usuario

---

## 2. Base URLs

| Environment | URL |
|---|---|
| Local (dev) | `http://localhost:8080` |
| Cloud (SaaS) | `https://api.nexusmind.ai` |
| Self-hosted | Configurable vía `--api-addr` |

---

## 3. Authentication

```
Authorization: Bearer <nexusmind_api_key>
```

API keys se gestionan desde Admin Console. Cada key puede tener scopes (memory, policy, audit).

---

## 4. Endpoints

### 4.1 Memory API

#### Store Memory
```
POST /v1/memory/store
```

```json
{
  "tool": "claude-code",
  "project": "acme-webapp",
  "user": "ana@acme.com",
  "type": "semantic",
  "content": "El proyecto usa FastAPI con Pydantic v2",
  "tags": ["tech-stack", "backend"],
  "metadata": {
    "session_id": "sess_abc123",
    "model": "claude-opus-4"
  }
}
```

Response: `201 Created`
```json
{
  "id": "mem_9a8b7c",
  "created_at": "2026-05-10T19:00:00Z"
}
```

#### Search Memory
```
POST /v1/memory/search
```

```json
{
  "query": "¿Qué stack usa el proyecto?",
  "project": "acme-webapp",
  "user": "ana@acme.com",
  "limit": 5,
  "min_score": 0.5
}
```

Response:
```json
{
  "results": [
    {
      "id": "mem_9a8b7c",
      "content": "El proyecto usa FastAPI con Pydantic v2",
      "score": 0.92,
      "tool": "claude-code",
      "tags": ["tech-stack", "backend"],
      "created_at": "2026-05-10T18:30:00Z"
    },
    {
      "id": "mem_7c6d5e",
      "content": "Decidimos usar PostgreSQL como base de datos principal",
      "score": 0.87,
      "tool": "cursor",
      "tags": ["tech-stack", "database"],
      "created_at": "2026-05-09T14:20:00Z"
    }
  ],
  "total": 12
}
```

#### Delete Memory
```
DELETE /v1/memory/:id
```

### 4.2 Policy API

#### Check Policy
```
POST /v1/policy/check
```

```json
{
  "user": "ana@acme.com",
  "tool": "cursor",
  "model": "gpt-4",
  "action": "chat",
  "prompt_hash": "sha256:abc123...",
  "prompt_preview": "50 chars preview",
  "estimated_tokens": 1200,
  "estimated_cost": 0.003
}
```

Response:
```json
{
  "allowed": true,
  "decisions": [
    {"policy": "P-001", "name": "no-pii-exfiltration", "result": "passed"},
    {"policy": "P-015", "name": "budget-monthly", "result": "passed", "remaining": 45.50}
  ],
  "latency_ms": 12
}
```

#### List Policies
```
GET /v1/policies
```

```json
{
  "policies": [
    {
      "id": "P-001",
      "name": "no-pii-exfiltration",
      "enabled": true,
      "match": {"tools": ["*"], "models": ["*"]},
      "updated_at": "2026-05-01T00:00:00Z"
    }
  ]
}
```

### 4.3 Audit API

#### Log Interaction
```
POST /v1/audit/log
```

```json
{
  "user": "ana@acme.com",
  "tool": "cursor",
  "model": "gpt-4",
  "action": "chat",
  "prompt_hash": "sha256:abc123...",
  "response_hash": "sha256:def456...",
  "tokens_in": 1200,
  "tokens_out": 350,
  "cost": 0.0034,
  "policy_decisions": ["P-001:passed", "P-015:passed"],
  "status": "allowed"
}
```

Response: `201 Created`

#### Query Audit
```
GET /v1/audit?user=ana@acme.com&tool=cursor&from=2026-05-01&to=2026-05-10&limit=50
```

```json
{
  "entries": [
    {
      "id": "aud_001",
      "timestamp": "2026-05-10T18:30:00Z",
      "user": "ana@acme.com",
      "tool": "cursor",
      "model": "gpt-4",
      "action": "chat",
      "tokens_in": 1200,
      "tokens_out": 350,
      "cost": 0.0034,
      "status": "allowed",
      "previous_hash": "sha256:prev...",
      "hash": "sha256:current..."
    }
  ],
  "total": 234,
  "page": 1
}
```

### 4.4 Context API

#### Get Project Context
```
GET /v1/context/project/:project
```

```json
{
  "project": "acme-webapp",
  "recent_memories": ["mem_1", "mem_2", "mem_3"],
  "active_agents": 3,
  "tools_connected": ["cursor", "claude-code", "copilot"],
  "last_active": "2026-05-10T19:00:00Z",
  "summary": "Proyecto de migración a microservicios. Stack: Go + PostgreSQL + Redis."
}
```

### 4.5 Health Check
```
GET /v1/health
```

```json
{
  "status": "ok",
  "version": "2.0.0",
  "uptime": 3600,
  "memory_size_mb": 128,
  "tools_connected": 5,
  "policies_active": 12
}
```

---

## 5. MCP Server Endpoints

Para herramientas que soporten MCP (Claude Code, Cline, Roo Code):

```
mcp/
├── memory/        ← MCP resources for memory
│   ├── search     ← Search across all tools
│   └── store      ← Store memory from any tool
├── policy/        ← MCP tools for policy
│   ├── check      ← Validate against policies
│   └── list       ← List applicable policies
└── context/       ← MCP resources for context
    ├── project    ← Full project context
    └── session    ← Current session context
```

---

## 6. SDKs

### Python
```python
from nexusmind import NexusMind

nm = NexusMind(api_key="nm_key_123")
nm.memory.store("El proyecto usa Go 1.22", project="acme", tags=["tech-stack"])
results = nm.memory.search("¿Qué stack usa el proyecto?", project="acme")
```

### TypeScript
```typescript
import { NexusMind } from 'nexusmind-sdk';

const nm = new NexusMind({ apiKey: 'nm_key_123' });
await nm.memory.store({
  content: 'El proyecto usa Go 1.22',
  project: 'acme',
  tags: ['tech-stack']
});
```

### Go
```go
import "github.com/smart-coder-labs/nexus-mind/sdk/go"

nm := nexusmind.New("nm_key_123")
nm.Memory.Store(ctx, MemoryEntry{
  Content: "El proyecto usa Go 1.22",
  Project: "acme",
  Tags:    []string{"tech-stack"},
})
```

---

## 7. Rate Limits

| Tier | Requests/min | Burst |
|---|---|---|
| Open Source | 100 | 200 |
| Team | 1,000 | 2,000 |
| Enterprise | 10,000 | 20,000 |

---

## 8. Errors

| Code | Description |
|---|---|
| `401` | Invalid or missing API key |
| `403` | Policy denied the request |
| `429` | Rate limit exceeded |
| `500` | Internal error |

---

*Fin de API_SPEC.md v2.0*
