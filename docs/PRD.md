# NexusMind — Product Requirements Document (PRD)

> **Documento**: PRD.md
> **Versión**: 2.0
> **Fecha**: Mayo 2026
> **Propósito**: Definición del producto — un control plane universal para herramientas AI enterprise, no un reemplazo de ellas.

---

## 1. Product Vision

> **"El control plane que unifica todas las herramientas AI de tu empresa — sin importar qué agente, qué modelo o qué IDE use cada equipo. Un centro de gravedad para reglas, memoria, trazabilidad y gobierno."**

NexusMind **no compite** con GitHub Copilot, Claude Code, Cursor, ni ningún agente AI. Por el contrario, los **potencia** al proveer una capa de orquestación, memoria persistente y gobierno que cualquier herramienta puede consumir.

El problema real no es que falten buenas herramientas AI — sobran. El problema es que cada equipo elige la suya, no hay trazabilidad, el conocimiento se pierde cuando alguien se va, y nadie sabe qué datos sensibles están pasando por APIs de terceros.

**NexusMind no reemplaza herramientas. Las orquesta.**

```
[Claude Code]  ─┐
[Cursor]       ─┤
[GitHub Copilot]─┼──→  NexusMind Control Plane  ──→  Reglas empresa
[OpenCode]     ─┤       │                          ──→  Memoria persistente
[CrewAI]       ─┤       │                          ──→  Audit trails
[Agentes custom]─┘       │                          ──→  SSO / RBAC
                         ▼
              Cualquier LLM (BYOM)
              OpenAI · Anthropic · Google · Open-source
```

### Principios Fundamentales

1. **BYOT (Bring Your Own Tool)** — NexusMind no reemplaza Claude Code, Cursor, Copilot ni ningún agente. Al contrario: les da superpoderes (memoria, contexto, reglas).
2. **Zero Lock-in** — Si mañana dejas NexusMind, tu memoria se exporta, tus reglas se exportan, tus audit trails se exportan. No hay dependencia.
3. **Multi-agent Orchestration** — El CTO decide: "Quiero que cualquier agente que use mi equipo cumpla estas reglas". NexusMind las aplica sin importar qué herramienta esté usando el developer.
4. **Capa de gobierno, no de reemplazo** — El valor está en el centro, no en el borde.

---

## 2. Target Personas

### 2.1 VP Engineering / CTO — *Comprador*

| Atributo | Descripción |
|---|---|
| **Rol** | Toma decisiones de compra para herramientas de productividad del equipo |
| **Dolores actuales** | Cada equipo usa herramientas AI distintas; costos dispersos; sin visibilidad de uso; compliance es un dolor de cabeza; no tiene control sobre qué hacen los agentes |
| **Valor de NexusMind** | Control centralizado sobre cualquier herramienta AI que use su equipo. Sin importar si usan Claude Code, Cursor, Copilot o agentes custom |
| **Criterios de compra** | Integración con tools existentes (no reemplazo), seguridad enterprise, audit trails, exportabilidad de datos |
| **Frases típicas** | "No quiero decirle al equipo qué herramienta usar. Quiero que cualquier herramienta que usen cumpla con nuestras políticas." |

### 2.2 Developer — *Usuario Primario*

| Atributo | Descripción |
|---|---|
| **Rol** | Ingeniero que usa herramientas AI para escribir código |
| **Dolores actuales** | Pierde contexto entre sesiones, entre herramientas, entre equipos. Cada herramienta tiene su propia "memoria" |
| **Valor de NexusMind** | Memoria unificada cross-tool. Lo que aprendió con Claude Code está disponible cuando usa Cursor. El contexto del proyecto persiste sin importar el IDE |
| **Frases típicas** | "Quiero que mi AI me entienda sin importar qué herramienta esté usando" |

### 2.3 Product Manager / Analyst — *Usuario No-Técnico*

| Atributo | Descripción |
|---|---|
| **Rol** | Define features, analiza datos, coordina equipos |
| **Dolores actuales** | No sabe programar; los agentes AI que usan los devs están fuera de su alcance |
| **Valor de NexusMind** | Agentes pre-construidos que se integran con la memoria de la empresa. Con gobernanza, trazabilidad y sin depender de developers |
| **Frases típicas** | "Necesito que los agentes que usa el equipo también puedan ayudarme a mí" |

### 2.4 Compliance / Security Officer — *Stakeholder*

| Atributo | Descripción |
|---|---|
| **Rol** | Garantiza compliance en herramientas de productividad |
| **Dolores actuales** | Herramientas AI son "cajas negras"; sin audit trails; datos pueden filtrarse a modelos externos |
| **Valor de NexusMind** | Audit trails inmutables de *todas* las interacciones AI de la empresa, sin importar la herramienta. PII redaction automática. Políticas centralizadas |
| **Frases típicas** | "Necesito saber qué datos está viendo cada agente, en todo momento" |

---

## 3. Core Features

### 3.1 Priorización

| Prioridad | Feature | Esfuerzo | Dependencias |
|---|---|---|---|
| **P0** | Memory System (unificado, cross-tool) | 6 sem | SQLite + embeddings |
| **P0** | Policy Engine (reglas centralizadas) | 4 sem | Backend API |
| **P0** | Audit Trail (todas las interacciones) | 4 sem | Backend API |
| **P0** | Tool Integrations API (plugins para Claude, Cursor, etc.) | 6 sem | Memory System |
| **P1** | Multi-agent Orchestration | 8 sem | Agent Runtime |
| **P1** | Enterprise Admin Console | 6 sem | Backend API |
| **P1** | MCP / Open-Context Plugins | 4 sem | Tool Integrations |
| **P2** | Non-developer Agents | 6 sem | UI, Agent Runtime |
| **P2** | Analytics & Cost Control | 4 sem | Audit Trail |
| **P2** | Custom Agent Builder | 8 sem | UI, Agent Runtime |

---

### P0: Memory System (Cross-Tool)

**Descripción**: Sistema de memoria persistente accesible por cualquier herramienta AI. Un developer usando Claude Code puede acceder al mismo contexto que cuando usaba Cursor la semana pasada. La memoria es del **proyecto/equipo**, no de la herramienta.

**Arquitectura**:

```
[Claude Code] ──┐
[Cursor]       ─┤
[Copilot]      ─┼──→ NexusMind Memory API ──→ SQLite FTS5 + Vectors
[OpenCode]     ─┤                                │
[CrewAI]       ─┘                                ▼
                                           Embeddings → semantic search
```

**Acceptance Criteria**:
- [ ] APIs REST para write/read/search memory desde cualquier tool
- [ ] Plugins/extensiones para Claude Code, Cursor, Copilot, OpenCode
- [ ] Búsqueda híbrida FTS + semántica
- [ ] <100ms FTS search, <200ms vector search
- [ ] Memoria cross-session y cross-tool verificable

---

### P0: Policy Engine

**Descripción**: Motor de reglas centralizado que define qué puede y qué no puede hacer cada agente. Las reglas se aplican sin importar qué herramienta esté usando el usuario.

**Ejemplos de políticas**:
- "Ningún agente puede enviar datos a modelos externos sin aprobación"
- "Código con PII debe ser redactado antes de enviarse a cualquier LLM"
- "Los audit trails de interacciones con Claude Code deben retenerse 90 días"
- "Solo el equipo de backend puede ejecutar agentes contra producción"

**Arquitectura**:
```
[Tool] → NexusMind Policy Gateway → ¿Permitido?
   │           │                          │
   │           ├── Check RBAC ────────────┤
   │           ├── Check Data Rules ──────┤
   │           ├── Check Model Policy ────┤
   │           └── Log to Audit Trail ────┤
   ▼                                      ▼
 Ejecuta o Rechaza                  Siempre auditoría
```

**Acceptance Criteria**:
- [ ] Políticas configurables vía YAML/JSON
- [ ] Evaluación <50ms por política
- [ ] Rechazo con mensaje claro (ej: "Política P-042: no puedes enviar PII a modelos externos")
- [ ] Versionado de políticas (git-ops friendly)
- [ ] Dry-run mode para testear políticas sin aplicarlas

---

### P0: Audit Trail

**Descripción**: Registro inmutable de todas las interacciones AI de la empresa, sin importar qué herramienta las originó. Cada query, cada respuesta, cada decisión del policy engine queda registrada.

**Schema de cada registro**:
```
{
  "timestamp": "2026-05-10T19:00:00Z",
  "user": "ana@acme.com",
  "tool": "claude-code",
  "model": "claude-opus-4",
  "action": "chat",
  "prompt_hash": "abc123...",
  "prompt_preview": "Escribe una función que...",
  "response_hash": "def456...",
  "tokens_in": 142,
  "tokens_out": 89,
  "policy_decisions": ["P-042: passed", "P-015: redacted PII"],
  "cost": 0.0034,
  "status": "allowed"
}
```

**Acceptance Criteria**:
- [ ] Append-only (immutable por diseño)
- [ ] Búsqueda por usuario, tool, modelo, acción, fecha
- [ ] Export CSV/JSON/PDF
- [ ] Retention configurable por política
- [ ] Hash chain para verificar integridad

---

### P0: Tool Integrations API

**Descripción**: Capa de integración que permite a cualquier herramienta AI conectarse a NexusMind. En lugar de construir UI propia, NexusMind provee APIs y plugins que las herramientas existentes consumen.

**Integraciones target**:

| Herramienta | Tipo de integración | Release |
|---|---|---|
| Claude Code | MCP server / Custom extension | V1 |
| Cursor | Plugin / API | V1 |
| GitHub Copilot | Extension API | V1 |
| OpenCode | MCP server | V1 |
| Cline / Roo Code | MCP server | V1 |
| Cualquier agente | REST API genérica | Siempre |

**API Surface**:
```
POST /v1/memory/search      ← Búsqueda semántica
POST /v1/memory/store       ← Guardar en memoria
POST /v1/policy/check       ← Validar contra políticas
POST /v1/audit/log          ← Registrar interacción
GET  /v1/context/project    ← Obtener contexto del proyecto
```

**Acceptance Criteria**:
- [ ] SDK en Python, TypeScript, Go
- [ ] OpenAPI 3.0 spec completa
- [ ] Rate limiting por API key
- [ ] Plugins publicados para Claude Code, Cursor, Copilot
- [ ] Documentación de integración para herramientas custom

---

### P1: Multi-agent Orchestration

**Descripción**: Orquestación de agentes que permite coordinar múltiples herramientas AI trabajando en un mismo objetivo. No importa si un agente es Claude Code, otro es un CrewAI pipeline y otro es un script custom — NexusMind los coordina.

**Patrones soportados**:
- **Chain**: Tool A → Tool B → Tool C (secuencial)
- **Fan-out**: Tool A → Tool B + Tool C + Tool D (paralelo)
- **Voting**: Múltiples agentes proponen, uno decide
- **Handoff**: Agente A delega a Agente B con contexto

---

### P1: Enterprise Admin Console

**Descripción**: Panel de administración para configurar políticas, ver audit trails, monitorear costos y gestionar integraciones.

---

### P1: MCP / Open-Context Plugins

**Descripción**: Plugins estándar del protocolo MCP (Model Context Protocol) y formatos abiertos para que cualquier herramienta compatible pueda integrarse sin código custom.

---

## 4. Non-functional Requirements

| Requisito | Especificación |
|---|---|
| **Uptime** | 99.9% (Control Plane), 99.99% Enterprise |
| **Latency** | <50ms policy check, <200ms memory search |
| **Security** | SOC2 Type II, encryption at rest/transit, AuthN/AuthZ granular |
| **Identity** | SSO (SAML/OIDC/SCIM), device fingerprint, tool identity |
| **Authorization** | RBAC híbrido + ABAC, Rego policies, permisos granulares |
| **Memory Isolation** | Proyecto + sensibilidad + herramienta + rol |
| **Data Residency** | US, EU, APAC; on-prem option |
| **Audit Trails** | Inmutables, retention configurable |
| **Scalability** | 10k+ concurrent tools por instancia |
| **Exportabilidad** | Memoria, policies, audit trails exportables (no lock-in) |
| **Compliance** | GDPR, SOC2, EU AI Act (Year 1) |

---

## 5. User Stories

### CTO Journey
```
Como: CTO
Quiero: Definir una política "ningún código con PII sale a modelos externos"
Para: Que cualquier herramienta que mi equipo use respete esa regla
Criterios:
- Política se aplica a Claude Code, Cursor, Copilot y agentes custom
- Audit trail muestra cada vez que se intentó violar
- Reporte semanal de cumplimiento
```

### Developer Journey
```
Como: Developer
Quiero: Que mi memoria de contexto persista entre Cursor y Claude Code
Para: No tener que re-explicar el proyecto cada vez que cambio de herramienta
Criterios:
- Memoria guardada desde Cursor aparece en Claude Code
- Búsqueda semántica funciona igual desde cualquier tool
- No hay duplicación de contexto
```

### Compliance Journey
```
Como: Compliance Officer
Quiero: Un audit trail de todas las interacciones AI de la empresa
Para: Demostrar en auditoría que controlamos qué datos ven los LLMs
Criterios:
- Exportable a PDF/CSV
- Hash chain que verifique inmutabilidad
- Filtro por fecha, usuario, herramienta, modelo
```

---

## 6. Non-Goals (lo que NexusMind NO es)

| ❌ No es | ✅ Es |
|---|---|
| Un reemplazo de Claude Code | Un complemento que le da memoria y gobierno |
| Un reemplazo de Cursor | Un plugin que le da contexto cross-tool |
| Un hosting de LLMs propios | Un BYOM gateway (trae tu propio modelo) |
| Un IDE | Un control plane que funciona con cualquier IDE |
| Un competidor de Copilot | Un integrador que potencia Copilot |
| Otro agente AI | El orquestador de todos los agentes |

---

## 7. Business Model

NexusMind no cobra por LLM usage (el cliente trae sus propias keys). Cobramos por:

| Plan | Precio | Incluye |
|---|---|---|
| **Open Source** | Gratis | Memory API, policy engine básico, audit trail local |
| **Team** | $49/mes | + Admin console, SSO, 5 policies, 30d retention |
| **Enterprise** | Custom | + On-prem, SOC2, retention ilimitada, SLA, soporte dedicado |

No hay costos de infra LLM para NexusMind. El cliente paga sus LLMs directamente. Esto nos permite escalar sin asumir riesgo de costos de inferencia.

---

## 8. Metrics & Success Criteria

| Métrica | Target |
|---|---|
| Tools integradas en el ecosistema | 10+ en 12 meses |
| Políticas activas por empresa | >15 |
| Tasa de cumplimiento de políticas | >99% |
| Latencia p95 de policy check | <50ms |
| NPS (Developers) | >60 |
| NPS (CTOs) | >70 |
| Tiempo de integración de nueva tool | <1 día (con SDK) |

---

## 9. Open Questions / Decisiones Pendientes

| Pregunta | Decisión Tentativa |
|---|---|
| Open-source core o proprietary? | Core abierto (memory + policy engine), enterprise features cerradas |
| MCP protocol o API propia? | Ambos: MCP para compatibilidad estándar, API para features avanzadas |
| Cómo monetizar sin cobrar LLM? | SaaS (per-seat) + Enterprise (self-hosted) |
| Plugins mantenidos por NexusMind o comunidad? | Core mantenido por equipo, community plugins con review |
| Base de datos vectorial? | sqlite-vss para MVP, pgvector para scale |

---

*Fin de PRD.md v2.0*
