# NexusMind — Engineering Process

> **Documento**: ENGINEERING_PROCESS.md
> **Versión**: 2.0
> **Fecha**: Mayo 2026
> **Propósito**: Procesos de ingeniería para construir el control plane — con foco en integraciones, protocolos abiertos y despliegue mínimo.

---

## 1. Principios de Ingeniería

1. **Integration-first** — Cada feature debe demostrar su valor integrándose con una herramienta real (Claude Code, Cursor, Copilot).
2. **API surface pequeña** — Menos endpoints, más flexibilidad. Preferir REST sobre GraphQL. Preferir MCP sobre API propia.
3. **SQLite como base** — Sin dependencias externas para el MVP. PostgreSQL como opción de scale.
4. **BYOM (Bring Your Own Model)** — Nunca dependemos de un proveedor de LLM. El core funciona sin LLMs.
5. **Exportabilidad** — Todo se exporta. Memoria, políticas, audit trails. Sin lock-in.
6. **Single binary deploy** — On-prem debe ser tan simple como `./nexusmind --config config.yaml`.

---

## 2. Stack

| Componente | Tecnología | Razón |
|---|---|---|
| **Backend** | Rust (ADR-001) | Performance determinista, sin GC, concurrencia real sobre SQLite, borrow checker |
| **Database** | SQLite (WAL + FTS5) | Sin dependencias, portable |
| **Vectors** | sqlite-vss (MVP) → pgvector (scale) | Embeddings sin infra adicional |
| **MCP Server** | TypeScript | Ecosistema Anthropic |
| **Admin UI** | React + Tailwind CSS | Solo admin console, no UI de usuario final |
| **SDKs** | Python, TypeScript, Rust (post-MVP) | Developer experience |
| **Plugins** | Extension API (Cursor), MCP (Claude), GitHub Ext (Copilot) | Nativo |

---

## 3. SDLC

### 3.1 Feature Flow

1. **RFC (Request for Comments)** — Documento corto: ¿qué herramienta integramos? ¿cómo? ¿API surface?
2. **Plugin First** — La feature se prueba como plugin de una herramienta existente antes de construir UI propia.
3. **API + SDK** — Si la feature requiere API, se escribe spec OpenAPI + SDK mínimo primero.
4. **Integration Test** — Prueba end-to-end con la herramienta real.
5. **Documentation** — README + ejemplo funcional + snippet de código.

### 3.2 Release Cadence

| Release | Frecuencia | Contenido |
|---|---|---|
| **Daily** | Diario | Bug fixes, small improvements |
| **Weekly** | Semanal | Nuevas features, plugins, mejoras API |
| **Monthly** | Mensual | Release mayor con changelog |

---

## 4. CI/CD

- **Lint**: cargo clippy, prettier
- **Test**: cargo test, vitest (SDKs), integration tests con herramientas reales
- **Build**: cargo build --release + cross-compile (cargo-zigbuild para ARM)
- **Release**: GitHub Actions → Docker image + single binary release
- **Deploy**: Docker compose (dev), Helm chart (prod)

---

## 5. Code Review

- Todos los PRs requieren aprobación de al menos 1 reviewer
- Los plugins open-source pueden ser revisados por la comunidad
- No se mergea sin integration test que pase
- Breaking changes requieren RFC + approval del CTO

---

## 6. Plugin Architecture

### 6.1 Plugin Types

| Type | Lenguaje | Ejemplo |
|---|---|---|
| **MCP Server** | TypeScript | Claude Code, Cline, Roo Code |
| **Extension API** | TypeScript | Cursor, VS Code |
| **GitHub Extension** | TypeScript | Copilot |
| **REST SDK** | Python, Rust (post-MVP) | Agentes custom |

### 6.2 Plugin Development Process

1. Fork del template de plugin
2. Implementar integración usando NexusMind API
3. Test end-to-end con la herramienta target
4. Publicar en el marketplace de la herramienta + NexusMind docs
5. Mantenimiento: actualizar cuando la herramienta cambie API

---

*Fin de ENGINEERING_PROCESS.md v2.0*
