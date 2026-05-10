# NexusMind — Roadmap

> **Documento**: ROADMAP.md
> **Versión**: 2.0
> **Fecha**: Mayo 2026
> **Propósito**: Roadmap actualizado. Prioridad #1: integraciones con herramientas existentes y memoria cross-tool. No construir UI propia hasta que el control plane funcione.

---

## 1. Filosofía del Roadmap

1. **Primero integraciones, después features.** El valor está en conectar herramientas existentes, no en construir las nuestras.
2. **Pulgas vs. Elefantes.** Un plugin para Claude Code vale más que 100 líneas de UI propia.
3. **BYOM primero.** No tocamos costos de LLM. El cliente trae sus keys.
4. **Open-source como strategy.** Core abierto, enterprise features cerradas.

---

## 2. Fase 1: MVP — "The Integrator" (Meses 1-6)

**Objetivo**: Demostrar que NexusMind puede integrarse con las herramientas AI más populares y proveer valor inmediato.

### Hitos

| Mes | Hito | Métrica de Éxito |
|---|---|---|
| M1 | Plugin Claude Code (MCP) + Memory API | 10 developers probándolo |
| M2 | Plugin Cursor + Policy Engine básico | 50 developers, 3 empresas piloto |
| M3 | Plugin GitHub Copilot + Audit Trail | 100 developers, 10 empresas |
| M4 | SDK Python + TypeScript + OpenSpec API | 3 plugins comunitarios |
| M5 | Open-source release (GitHub) | 100 stars, 20 forks |
| M6 | Admin Console (dashboard + analytics) | 20 empresas activas, $2K MRR |

### Features

- **Identidad y Auth:** — P0, crítica para seguridad
  - SSO (SAML 2.0 + OIDC) con Okta, Azure AD, Google
  - SCIM sync para users/groups automático
  - API Keys con scopes y rate limiting
  - Device fingerprint para CLI/IDE
  - JWT session management con refresh
- **Integraciones:** Claude Code (MCP), Cursor (Plugin), Copilot (Extension), OpenCode (MCP)
- **Memory API:** Store, search, semantic query. SQLite FTS5 + sqlite-vss.
- **Memory Isolation:** Por proyecto + nivel de sensibilidad
- **Policy Engine:** 5 políticas predefinidas + API para custom policies en YAML
- **Audit Trail:** Append-only log con búsqueda básica
- **RBAC:** Roles Super Admin, Security Officer, Developer, Viewer
- **SDKs:** Python + TypeScript + Go
- **Admin Console:** Session dashboard en tiempo real
- **Deploy:** Docker compose + single binary
- **Open-source:** Core bajo Apache 2.0 en GitHub

### Arquitectura Target

```
[Claude Code] ─┐
[Cursor]      ─┤
[Copilot]     ─┼── NexusMind API ── SQLite (memoria + policies + audit)
[OpenCode]    ─┤
[Cline]       ─┘
                    │
                    └── BYOM: OpenAI, Claude, Google, Local LLM (cliente trae keys)
```

---

## 3. Fase 2: Crecimiento — "The Control Plane" (Meses 7-12)

**Objetivo**: Consolidar NexusMind como el control plane estándar para equipos que usan AI.

### Hitos

| Mes | Hito | Métrica de Éxito |
|---|---|---|
| M7 | Team plan lanzado (SaaS auto-serve) | 50 equipos pagando |
| M8 | Enterprise plan (self-hosted) | 5 clientes enterprise |
| M9 | Multi-agent orchestration (Chain, Fan-out, Handoff) | 100+ agentes orquestados/día |
| M10 | SOC2 Type I + GDPR compliance docs | 10+ demos enterprise/mes |
| M11 | 15+ tools integradas | Ecosistema crítico |
| M12 | $45K MRR, 200 clientes, 10 enterprise | Sostenibilidad |

### Features

- **Multi-agent Orchestration:** Chain, Fan-out, Voting, Handoff patterns
- **Enterprise Admin Console:** RBAC granular, custom roles, SSO
- **Team Plan:** Self-serve signup, team management, billing
- **Enterprise Plan:** Self-hosted (Docker/K8s/single binary), SLA, soporte dedicado
- **SOC2:** Type I completed, Type II iniciado
- **GDPR:** Documentation, DPA, data residency choices
- **Integraciones nuevas:** Windsurf, Cline, Roo Code, agentes custom (SDK)

---

## 4. Fase 3: Escalamiento — "The Platform" (Meses 13-24)

**Objetivo**: Escalar a $1.2M ARR con foco enterprise y ecosistema de integraciones.

### Hitos

| Mes | Hito | Métrica de Éxito |
|---|---|---|
| M15 | $100K MRR | Cash flow positive ops |
| M18 | SOC2 Type II + HIPAA readiness | Enterprise pipeline >$500K |
| M20 | Marketplace de plugins (20+ plugins de comunidad) | 5% revenue de marketplace |
| M24 | $1.2M ARR, 500+ clientes | Series A ready |

### Features

- **Marketplace:** Plugins de terceros, revenue share 80/20
- **Custom Agent Builder:** Visual workflow + code editor
- **HIPAA compliance:** BAA, PHI handling, audit enhancements
- **Advanced Analytics:** Cost optimization recommendations, usage patterns
- **API v2:** GraphQL, webhooks, real-time events

---

## 5. Fase 4: Dominio — "The Standard" (Año 3+)

**Objetivo**: Ser el estándar de facto para control plane AI en empresas.

### Hitos

| Mes | Hito |
|---|---|
| M30 | $3M ARR |
| M36 | $4.5M ARR, profitable |
| M48 | 2,000+ clientes, 50+ enterprise >$50K ARR |

### Features

- **ISO 27001**
- **AI Governance Suite:** Automated compliance reporting
- **Private Cloud:** AWS/GCP/Azure marketplace deployments
- **Global:** Data residency APAC, LATAM, Middle East

---

## 6. Lo que NO construimos (a propósito)

| No construimos | Razón |
|---|---|
| Un chat AI propio | Claude/ChatGPT ya lo hacen mejor. Preferimos integrarlos |
| Un IDE | Cursor/VSCode ya existen. Preferimos plugins |
| Un hosting de LLMs | BYOM evita costos y lock-in |
| Un competidor de Copilot | Mejor potenciar Copilot que reemplazarlo |
| Una UI para developers | Los plugins se integran en su flujo actual |

---

## 7. Dependencias Externas

| Dependencia | Riesgo | Alternativa |
|---|---|---|
| APIs de Cursor, Copilot, etc. | Cambios breaking en APIs | MCP como protocolo universal, fallback REST |
| sqlite-vss (vectors) | Mantenimiento comunitario | pgvector (PostgreSQL) |
| Modelos open-source (Llama, Mistral) | Calidad inferior a GPT/Claude | BYOM permite elegir cualquier modelo |

---

## 8. Hitos de Revenue

```
$0     M1 ── Lanzamiento open-source
$2K    M6 ── Primeros clientes Team plan
$45K   M12 ─ Team + Enterprise
$200K  M18 ─ Enterprise growth
$500K  M24 ─ Series A ready
$1.2M  M30 ─ Growth stage
$4.5M  M36 ─ Profitable
```

---

*Fin de ROADMAP.md v2.0*
