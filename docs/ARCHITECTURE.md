# NexusMind — Architecture Document

> **Documento**: ARCHITECTURE.md
> **Versión**: 2.0
> **Fecha**: Mayo 2026
> **Propósito**: Arquitectura del Control Plane — capa de memoria, políticas y orquestación que se integra con herramientas AI existentes.

---

## 1. Visión Arquitectónica

NexusMind NO es una herramienta AI más. Es un **control plane** que se sienta *entre* las herramientas AI y los LLMs, proveyendo:

1. **Memoria persistente cross-tool** — El contexto del proyecto vive en NexusMind, no en la herramienta
2. **Policy Engine** — Reglas de negocio que se aplican a cualquier herramienta
3. **Audit Trail** — Registro inmutable de todas las interacciones
4. **Orquestación multi-agent** — Coordinación de herramientas heterogéneas

```
                    ┌─────────────────────────────────┐
                    │   TOOLS (BYOT)                   │
                    │  ┌────────┐ ┌────────┐ ┌──────┐ │
                    │  │Claude  │ │ Cursor │ │Cline │ │
                    │  │ Code   │ │        │ │      │ │
                    │  └───┬────┘ └───┬────┘ └──┬───┘ │
                    │      │          │         │      │
                    └──────┼──────────┼─────────┼──────┘
                           │          │         │
                    ┌──────┴──────────┴─────────┴──────┐
                    │   NEXUSMIND CONTROL PLANE         │
                    │                                   │
                    │  ┌─────────────────────────────┐  │
                    │  │  Policy Gateway             │  │
                    │  │  (RBAC, Data Rules, Model   │  │
                    │  │   Compliance, Cost Controls)│  │
                    │  └─────────────┬───────────────┘  │
                    │                │                  │
                    │  ┌─────────────┴───────────────┐  │
                    │  │  Memory Layer               │  │
                    │  │  (Episodic, Semantic,       │  │
                    │  │   Procedural)               │  │
                    │  └─────────────┬───────────────┘  │
                    │                │                  │
                    │  ┌─────────────┴───────────────┐  │
                    │  │  Audit Trail                │  │
                    │  │  (Append-only, Hash Chain)  │  │
                    │  └─────────────────────────────┘  │
                    └───────────────────────────────────┘
                                      │
                    ┌─────────────────┴─────────────────┐
                    │   LLMs (BYOM)                      │
                    │  ┌────────┐ ┌────────┐ ┌──────┐   │
                    │  │OpenAI  │ │Claude  │ │Local │   │
                    │  │        │ │        │ │LLaMA │   │
                    │  └────────┘ └────────┘ └──────┘   │
                    └────────────────────────────────────┘
```

---

## 2. Componentes Principales

### 2.1 Policy Gateway

El corazón del control plane. Cada request de cualquier herramienta pasa por aquí.

```
Request → Policy Gateway → [Auth Check] → [Data Rules] → [Model Policy] → [Cost Check]
                                │               │              │               │
                                ▼               ▼              ▼               ▼
                            ¿Usuario      ¿Datos       ¿Modelo        ¿Dentro de
                            autenticado?  sensibles?   permitido?     presupuesto?
```

**Implementación**:
- Go (REST API) — mínimo overhead, alta concurrencia
- Políticas en YAML versionado (git-ops)
- Evaluación <50ms por política
- Caché de decisiones frecuentes

**Ejemplo de política**:
```yaml
apiVersion: nexusmind.io/v1
kind: Policy
metadata:
  name: no-pii-to-external
spec:
  match:
    tools: ["claude-code", "cursor", "copilot"]
    models: ["claude-opus-4", "gpt-4"]
  rules:
    - action: redact
      patterns:
        - "[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}"  # emails
        - "\\b\\d{3}-\\d{2}-\\d{4}\\b"                        # SSN
      label: "PII"
    - action: block
      when: "cost_per_request > 0.05"
      label: "COST_LIMIT"
  on_violation: log_and_continue
  audit: always
```

### 2.2 Memory Layer

Memoria persistente accesible por cualquier herramienta. Tres tipos:

| Tipo | Descripción | Storage |
|---|---|---|
| **Episodic** | Historial cronológico de interacciones | SQLite (FTS5) |
| **Semantic** | Conocimiento extraído + vectores | SQLite + sqlite-vss |
| **Procedural** | Config, preferencias, patrones | SQLite (key-value) |

**API Memory**:
```typescript
// Escribir en memoria
POST /v1/memory/store
{
  "tool": "claude-code",
  "type": "semantic",
  "content": "El proyecto usa FastAPI con Pydantic v2",
  "tags": ["project", "tech-stack"],
  "project": "nexusmind"
}

// Buscar en memoria (cross-tool)
POST /v1/memory/search
{
  "query": "¿Qué stack usa el proyecto?",
  "project": "nexusmind",
  "limit": 5
}
// → Retorna resultados de Claude Code, Cursor, Copilot combinados
```

### 2.3 Audit Trail

Registro inmutable y consultable de todas las interacciones.

**Diseño**:
- Append-only log (WAL mode)
- Cada entrada tiene: timestamp, user, tool, model, action, prompt_hash, tokens, cost, policy_decisions
- Hash chain para verificar integridad
- Exportable a JSON/CSV/PDF
- Retention configurable por política

### 2.4 Integration Layer (MCP / REST)

Capa que permite a cualquier herramienta conectarse a NexusMind.

**Protocolos soportados**:
1. **REST API** — Genérica, cualquier herramienta puede llamarla
2. **MCP (Model Context Protocol)** — Estándar Anthropic, compatible con Claude Desktop/Code
3. **SDKs** — Python, TypeScript, Go para integración custom

**Plugins oficiales** (a desarrollar):
- `nexusmind-mcp` — MCP server para Claude Desktop/Code
- `nexusmind-cursor` — Plugin Cursor
- `nexusmind-copilot` — GitHub Copilot Extension
- `nexusmind-claude-code` — Claude Code hook

---

## 3. Data Flow: Escenario Típico

**Escenario**: Un developer escribe código en Cursor con memoria cross-tool y políticas de empresa.

```
1. Developer escribe prompt en Cursor
2. Cursor (vía plugin NexusMind) envía a Policy Gateway:
   POST /v1/policy/check
   { user: "ana@acme.com", tool: "cursor", model: "gpt-4", prompt: "..." }

3. Policy Gateway evalúa:
   ✓ Ana tiene rol "developer" → permitido
   ✓ Prompt no contiene PII → permitido
   ✓ Modelo gpt-4 está en whitelist → permitido
   ✓ Ana tiene budget suficiente → permitido

4. Cursor envía request a NexusMind Memory:
   POST /v1/memory/search
   { query: "..., project: "acme-webapp" }

5. Memory retorna contexto relevante (de sesiones previas en Cursor, Claude Code, etc.)

6. NexusMind inyecta contexto + policies en el prompt

7. Cursor envía prompt + contexto al LLM (gpt-4)

8. Respuesta del LLM vuelve a través de NexusMind para logging:
   POST /v1/audit/log
   { user, tool, model, prompt_hash, response_hash, tokens, cost, policy_decisions }

9. Respuesta se muestra al developer en Cursor

10. Memory se actualiza automáticamente:
    POST /v1/memory/store
    { tool: "cursor", type: "episodic", content: "...", project: "acme-webapp" }
```

---

## 4. Stack Tecnológico

| Capa | Tecnología | Justificación |
|---|---|---|
| **Backend API** | Go 1.22+ | Baja latencia, concurrencia nativa |
| **Base de datos** | SQLite (WAL mode) | Sin dependencias externas, portable |
| **Vectors** | sqlite-vss (MVP), pgvector (scale) | Embeddings sin infra adicional |
| **Cache** | SQLite en memoria + Redis (scale) | Políticas en caché |
| **Auth** | JWT + API Keys + SSO (SAML/OIDC) | Compatibilidad enterprise |
| **Policy Engine** | Rego (OPA) / Custom | Políticas declarativas |
| **MCP Server** | TypeScript | Compatibilidad con ecosistema Anthropic |
| **SDKs** | Python, TypeScript, Go | Developer experience |
| **Plugins (Cursor)** | TypeScript Extension API | Integración nativa |
| **Plugins (Copilot)** | GitHub Extension API | Integración nativa |
| **Deploy** | Docker + K8s | Portable, on-prem possible |

---

## 5. Integraciones Target (Evolutivas)

| Herramienta | Tipo | Prioridad | Esfuerzo |
|---|---|---|---|
| Claude Code | MCP + Hook | P0 | 2 semanas |
| Cursor | Plugin API | P0 | 2 semanas |
| GitHub Copilot | Extension API | P0 | 3 semanas |
| OpenCode | MCP | P0 | 1 semana |
| Cline / Roo Code | MCP | P1 | 1 semana |
| Windsurf | API | P1 | 2 semanas |
| Cualquier agente custom | REST API + SDKs | P1 | Documentación |

---

## 6. Seguridad

| Aspecto | Implementación |
|---|---|
| **Auth** | JWT + API Keys + SSO (SAML/OIDC) |
| **Encryption at rest** | AES-256 (SQLite Encryption Extension) |
| **Encryption in transit** | TLS 1.3 |
| **Audit integrity** | Hash chain (SHA-256) en append-only log |
| **Data isolation** | Multi-tenancy por project/organization |
| **PII redaction** | Regex patterns configurables + ML-based detection |

---

## 7. Deploy

### Local (desarrollo)
```bash
docker compose up
# NexusMind API en localhost:8080
# SQLite database en ./data/nexusmind.db
```

### Production (K8s)
```bash
helm install nexusmind ./charts/nexusmind
# Horizontal pod autoscaling
# PersistentVolumeClaim para SQLite
# Ingress con TLS termination
```

### On-prem (Enterprise)
```bash
# Single binary + SQLite file
./nexusmind --config /etc/nexusmind/config.yaml
# No dependencies. No external services.
```

---

## 8. Diagrama de Integración

```
                   ┌──────────────────────────────────┐
                   │      EMPRESA ACME                 │
                   │                                   │
                   │  ┌─────────┐  ┌─────────┐         │
                   │  │ Dev A   │  │ Dev B   │         │
                   │  │ (Cursor)│  │ (Claude)│         │
                   │  └────┬────┘  └────┬────┘         │
                   │       │            │              │
                   │       ▼            ▼              │
                   │  ┌──────────────────────────┐     │
                   │  │   NEXUSMIND INSTANCE      │     │
                   │  │   (Docker/K8s/Single-bin) │     │
                   │  └─────────────┬────────────┘     │
                   │                │                  │
                   │       ┌────────┴────────┐         │
                   │       │  SQLite (data)   │         │
                   │       └─────────────────┘         │
                   │                                   │
                   │  LLM Keys (traídas por ACME)      │
                   │       │          │                │
                   │       ▼          ▼                │
                   │  ┌────────┐ ┌────────┐           │
                   │  │ OpenAI │ │ Claude │           │
                   │  └────────┘ └────────┘           │
                   └──────────────────────────────────┘
```

---

*Fin de ARCHITECTURE.md v2.0*
