# NexusMind — MVP Scope Changelog

> **Documento**: 03-SCOPE_CHANGELOG.md
> **Versión**: 0.1.0
> **Fecha**: Mayo 2026
> **Propósito**: Registro de decisiones de scope para el MVP vs lo que prometen los documentos existentes.

---

## 0. Cambio de Stack

| Decisión | ADR | Estado |
|---|---|---|
| Lenguaje: Rust (no Go) | ADR-001 | ✅ Aceptado |
| Base de datos: SQLite (→ Postgres en enterprise) | ADR-002 | ✅ Aceptado |
| MCP Server: TypeScript (no Rust) | — | ✅ Decisión pragmática |

Este documento asume Rust como stack. Todos los documentos que referenciaban Go han sido actualizados.

---

## 1. Lo que QUEDA FUERA del MVP

### Policy Engine (completo con ABAC, Rego-like)
- **Documentado en**: PRD.md P0, ARCHITECTURE.md §2.1, ADR-001 §1.1 R7
- **Razón para excluir**: Policy engine requiere lógica custom significativa. Sin integraciones reales, no hay forma de validar que funcione. Además, el MVP necesita primero tener tools conectadas antes de regularlas.
- **Esfuerzo post-MVP**: Semana 5-6 (~40h) con implementación Rust sin Rego.

### Vector Search (sqlite-vec / Candle ONNX)
- **Documentado en**: PRD.md §3.1, ADR-001 §1.1 R2, ADR-002 §2.1
- **Razón para excluir**: FTS5 cubre el 80% de los casos de uso para memoria técnica. Vectors requieren un pipeline de embeddings (Candle/ONNX) que añade complejidad y tamaño binario. Mejor empezar con FTS5 y añadir vectors cuando la búsqueda semántica sea un bottleneck.
- **Esfuerzo post-MVP**: Semana 6-8 (~60h) con Candle + ONNX.

### SDKs Python y Go
- **Documentado en**: PRD.md, ROADMAP.md M4
- **Razón para excluir**: Con REST API + curl, cualquier lenguaje puede usar NexusMind. Los SDKs son QoL, no core.
- **Esfuerzo post-MVP**: Semana 5-6 (~20h cada uno).

### Multi-agent Orchestration
- **Documentado en**: PRD.md P1
- **Razón para excluir**: Sin tools conectadas, no hay agents que orquestar.
- **Esfuerzo post-MVP**: Fase 2 (meses 7-12).

### SSO / SAML / OIDC / SCIM
- **Documentado en**: ROADMAP.md §2, ADR-001 §1.1
- **Razón para excluir**: Complejidad alta para un MVP de 10 developers. API key es suficiente.
- **Esfuerzo post-MVP**: Fase 2, cuando haya clientes enterprise.

### Audit Trail Inmutable (Hash Chain + Merkle Tree + Ed25519)
- **Documentado en**: PRD.md §3.1, ADR-001 §1.1 R6, ADR-001 §3.2
- **Razón para excluir**: Append-only SQLite es suficientemente bueno para MVP. El Merkle tree y las firmas Ed25519 añaden ~200 líneas de código complejo que no se validan sin auditoría real. ADR-001 propone un crate `nexusmind-audit` entero — no para MVP.
- **Esfuerzo post-MVP**: Semana 7-8 (~40h).

### Plugins para Cursor, Copilot, Cline, etc.
- **Documentado en**: ROADMAP.md M1-M3
- **Razón para excluir**: Un plugin bien hecho para Claude Code vale más que 4 plugins a medias. El ecosistema MCP de Anthropic es el más documentado.
- **Esfuerzo post-MVP**: Semana 5+ (un plugin por semana).

### Terminal UI (Ratatui)
- **Documentado en**: ADR-001 §3.2 crate `nexusmind-tui`
- **Razón para excluir**: El ADR propone una TUI completa con screens, widgets, etc. Para MVP, CLI + REST API es suficiente. La TUI no aporta valor hasta que haya usuarios regulares.
- **Esfuerzo post-MVP**: Fase 2.

### Arquitectura Multi-Crate
- **Documentado en**: ADR-001 §3.3 (8+ crates: core, store, auth, audit, server, mcp, cli, tui, sync)
- **Razón para excluir**: 8 crates conllevan compilación incremental más lenta, mayor complejidad de workspace, y barreras de contribución altas. Un solo crate plano `src/` reduce la compilación de ~3 min a ~30s y mantiene el MVP manejable por 1 developer.
- **Post-MVP**: Dividir en crates cuando el código supere ~5000 líneas o cuando haya 2+ developers.

### Cloud Sync (Git + Postgres replication)
- **Documentado en**: ADR-001 §3.2 crate `nexusmind-sync`, ADR-002 §4.4 sync engine
- **Razón para excluir**: Sin multi-usuario, no hay necesidad de sync.
- **Esfuerzo post-MVP**: Fase 2.

### On-prem Single Binary (cross-compile ARM)
- **Documentado en**: ADR-001 §1.1 R11, ROADMAP.md M8
- **Razón para excluir**: Docker compose cubre deploy. Single binary se añade post-MVP con `cargo build --release`.
- **Nota**: Rust ya produce single binary (`./target/release/nexusmind`), pero no lo empaquetamos para ARM hasta Fase 2.

---

## 2. Lo que se REDUCE del scope

| Feature | Scope Original | Scope MVP | Diferencia |
|---|---|---|---|
| **Memory API** | 3 tipos + vectors + hybrid search | 1 tipo (semantic) + FTS5 | -2 tipos, sin vectors |
| **Auth** | SAML + OIDC + MFA + Device FP + JWT + API Key | Solo API Key simple | -5 mecanismos |
| **Audit Trail** | Merkle tree + Ed25519 + hash chain + export | Append-only simple | -hash chain, -firmas, -export |
| **MCP Server** | 19+ tools + resources completos | 3 tools (store, search, context) | -16 tools |
| **Admin UI** | Dashboard + analytics + reports + team mgmt | CRUD memorias + audit log | -analytics, -teams, -reports |
| **Plugins** | Claude + Cursor + Copilot + Cline + OpenCode | Solo Claude Code (MCP) | -4 plugins |
| **Estructura** | 8+ crates separados (workspace) | 1 crate plano `src/` | -7 crates |
| **Deploy** | K8s + Helm + single binary + Docker compose | Solo Docker compose | -K8s, -Helm |

---

## 3. Lo que se MANTIENE igual

| Feature | Status |
|---|---|
| Rust como lenguaje backend | ✅ ADR-001 mantenido |
| SQLite como base de datos | ✅ ADR-002 mantenido |
| FTS5 para búsqueda textual | ✅ Sin cambios |
| REST API como interfaz principal | ✅ Sin cambios |
| BYOM (Bring Your Own Model) | ✅ Sin cambios — no tocamos LLMs |
| Exportabilidad | ✅ Datos exportables (SQLite db es un archivo) |
| Open-source core | ✅ Sin cambios |
| Sin lock-in | ✅ Store Abstraction Trait (ADR-002 §6.3) diferido, pero el schema es SQL estándar |

---

## 4. Timeline Ajustado

```
ADR-001 Fase 1 (4 semanas para Drop-in replacement de Engram):
  - nexusmind-core, nexusmind-store (SQLite + FTS5), nexusmind-mcp (mem_save/search/context)
  - ~3000 líneas Rust, 8+ crates, TUI

MVP Propuesto (4 semanas para producto funcional):
  Sem1: Backend Rust + SQLite + Auth + Memory API
  Sem2: MCP Server para Claude Code
  Sem3: Cross-tool Memory + Admin UI mínima
  Sem4: Polish + Deploy + Release v0.1.0
  └── ~2300 líneas Rust + ~500 TS + ~700 React
  └── ~70% menos scope que la Fase 1 del ADR-001
```

---

## 5. Post-MVP Roadmap (Recomendado)

```
Sem 1-4:   MVP  ──  Memory API + MCP Server (Claude Code)
Sem 5-6:   v0.2 ──  Vector search (Candle ONNX) + Policy engine básico
Sem 7-8:   v0.3 ──  Plugin Cursor + Audit trail con hash chain
Sem 9-10:  v0.4 ──  Plugin Copilot + Store Abstraction Trait
Sem 11-12: v0.5 ──  Arquitectura multi-crate + Admin console completa
Sem 13-16: v0.6 ──  TUI (Ratatui) + Sync engine + On-prem ARM
```

Cada release de 2 semanas debe validarse con usuarios reales antes de pasar al siguiente.

---

*Fin de 03-SCOPE_CHANGELOG.md*
