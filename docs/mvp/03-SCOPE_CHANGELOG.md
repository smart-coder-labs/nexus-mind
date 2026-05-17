# NexusMind — MVP Scope Changelog

> **Documento**: 03-SCOPE_CHANGELOG.md
> **Versión**: 0.1.0
> **Fecha**: Mayo 2026
> **Propósito**: Registro de decisiones de scope para el MVP vs lo que prometen los documentos existentes.

---

## 1. Lo que QUEDA FUERA del MVP

### Policy Engine
- **Documentado en**: PRD.md P0, ARCHITECTURE.md §2.1
- **Razón para excluir**: Policy engine requiere Rego/OPA o lógica custom significativa. Sin integraciones reales, no hay forma de validar que funcione. Además, el MVP necesita primero tener tools conectadas antes de regularlas.
- **Posible post-MVP**: Semana 5-6, cuando haya 2+ tools conectadas.

### Vector Search (sqlite-vss / pgvector)
- **Documentado en**: PRD.md §3.1, ARCHITECTURE.md §2.2
- **Razón para excluir**: FTS5 cubre el 80% de los casos de uso para memoria técnica. Vectors requieren un pipeline de embeddings (API de OpenAI/Anthropic) que añade complejidad y costo. Mejor empezar con FTS5 y añadir vectors cuando veamos que la búsqueda semántica es un bottleneck.
- **Posible post-MVP**: Semana 6-8, como feature opt-in.

### SDKs Python y Go
- **Documentado en**: PRD.md, ROADMAP.md M4
- **Razón para excluir**: Con REST API + curl, cualquier lenguaje puede usar NexusMind. Los SDKs son QoL, no core. Además, TypeScript SDK viene implícito en el MCP server.
- **Posible post-MVP**: Semana 5-6, cuando haya demanda.

### Multi-agent Orchestration
- **Documentado en**: PRD.md P1
- **Razón para excluir**: Sin tools conectadas, no hay agents que orquestar. Esto no tiene sentido hasta Fase 2.
- **Posible post-MVP**: Fase 2 (meses 7-12).

### SSO / SAML / OIDC / SCIM
- **Documentado en**: ROADMAP.md §2
- **Razón para excluir**: Complejidad alta para un MVP que probablemente usen 10 developers. API key es más que suficiente.
- **Posible post-MVP**: Fase 2, cuando haya clientes enterprise.

### Audit Trail Inmutable (Hash Chain)
- **Documentado en**: PRD.md §3.1, ARCHITECTURE.md §2.3
- **Razón para excluir**: Append-only SQLite ya es suficientemente bueno para MVP. Hash chain para probar inmutabilidad es over-engineering en esta etapa.
- **Posible post-MVP**: Semana 6-8.

### Plugins para Cursor, Copilot, Cline, etc.
- **Documentado en**: ROADMAP.md M1-M3
- **Razón para excluir**: Un plugin bien hecho para Claude Code vale más que 4 plugins a medias. Además, el ecosistema MCP de Anthropic es el más maduro y documentado. Cursor y Copilot requieren APIs de extensión propietarias.
- **Posible post-MVP**: Semana 5+ (un plugin por semana).

### Enterprise Admin Console
- **Documentado en**: PRD.md P1
- **Razón para excluir**: Admin web mínima es suficiente. No necesitamos dashboards, analytics, ni gestión de equipos.
- **Posible post-MVP**: Fase 2.

---

## 2. Lo que se REDUCE del scope

| Feature | Scope Original | Scope MVP | Diferencia |
|---|---|---|---|
| **Memory API** | 3 tipos (episodic, semantic, procedural) + vectors | 1 tipo (semantic) + FTS5 | -2 tipos, sin vectors |
| **Auth** | JWT + API Key + SSO + Device Fingerprint | Solo API Key simple | -3 mecanismos |
| **Audit Trail** | Inmutable + hash chain + export | Append-only simple, sin export | -hash chain, -export |
| **MCP Server** | Todos los recursos y tools | Solo memory/search + memory/store | -policy, -context resources |
| **Admin UI** | Dashboard + analytics + team management | CRUD memorias + audit log | -analytics, -teams |
| **SDKs** | Python + TypeScript + Go | Solo REST API (sin SDK) | -3 SDKs |
| **Plugins** | Claude + Cursor + Copilot + Cline + OpenCode | Solo Claude Code (MCP) | -4 plugins |
| **Deploy** | K8s + Helm + single binary + Docker compose | Solo Docker compose | -K8s, -Helm, -single binary |

---

## 3. Lo que se MANTIENE igual

| Feature | Status |
|---|---|
| SQLite como base de datos | ✅ Sin cambios |
| FTS5 para búsqueda textual | ✅ Sin cambios |
| REST API como interfaz principal | ✅ Sin cambios |
| BYOM (Bring Your Own Model) | ✅ Sin cambios — no tocamos LLMs |
| Exportabilidad | ✅ Datos exportables (SQLite db es un archivo) |
| Go como backend | ✅ Sin cambios |
| Open-source core | ✅ Sin cambios |
| Sin lock-in | ✅ Sin cambios |

---

## 4. Timeline Ajustado

```
Documentos actuales (ROADMAP.md):
M1-M6: Claude Code + Cursor + Copilot + Policy + Audit + SDKs + Admin

Realidad MVP:
Sem1: Backend Go + SQLite + Auth + Memory API
Sem2: MCP Server para Claude Code
Sem3: Cross-tool Memory + Admin UI mínima
Sem4: Polish + Deploy + Release v0.1.0
      └── ~70% menos scope que la Fase 1 original
```

### Justificación del Timeline

Los documentos actuales estiman 6 meses para la Fase 1 con:
- 5 plugins (Claude, Cursor, Copilot, OpenCode, Cline)
- Policy engine completo
- Audit trail con hash chain
- SDKs en 3 lenguajes
- Admin console completa

Realistamente, construir todo eso toma 6 meses de **equipo full-time**. Para un MVP que valide la tesis central ("cross-tool memory"), 4 semanas con **1 developer** es suficiente si recortamos scope agresivamente.

---

## 5. Post-MVP Roadmap (Recomendado)

```
Sem 1-4:  MVP  ──  Memory API + MCP Server (Claude Code)
Sem 5-6:  V0.2 ──  Vector search + Policy engine básico
Sem 7-8:  V0.3 ──  Plugin Cursor + Audit trail mejorado
Sem 9-10: V0.4 ──  Plugin Copilot + SDK TypeScript
Sem 11-12:V0.5 ──  Admin console + Team plan
```

Cada release de 2 semanas debe validarse con usuarios reales antes de pasar al siguiente.

---

*Fin de 03-SCOPE_CHANGELOG.md*
