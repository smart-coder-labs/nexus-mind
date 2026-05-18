# NexusMind — MVP Docs

> Documentación del plan para sacar un MVP funcional en **4 semanas**.

---

## 📋 Documentos

| # | Documento | Qué contiene |
|---|---|---|
| 0 | [06-ENTERPRISE_MVP.md](./06-ENTERPRISE_MVP.md) | **Plan principal (v2)** — MVP enterprise con multi-usuario, admin panel y demo-ready |
| 1 | [02-ARCHITECTURE_MVP.md](./02-ARCHITECTURE_MVP.md) | Arquitectura (v1, archivada) |
| 2 | [03-SCOPE_CHANGELOG.md](./03-SCOPE_CHANGELOG.md) | Scope diff: v1 vs v2 |
| 3 | [04-MVP_PITCH.md](./04-MVP_PITCH.md) | Pitch ejecutivo (v1, archivado) |
| 4 | [05-TASK_BREAKDOWN.md](./05-TASK_BREAKDOWN.md) | Desglose granular v1 (archivado) |

---

## 🎯 Resumen (v2 — Enterprise)

**Stack**: Rust + SQLite + Axum (multi-tenant) + React admin panel.

**Meta**: Demo enterprise donde el CTO ve multi-usuario, control centralizado, audit trail y admin panel.

**MVP Scope**:
- ✅ Backend multi-tenant (organizaciones, usuarios, API keys scoped)
- ✅ Admin panel (Dashboard, Users, Memories, Audit Log)
- ✅ Script de demo con datos de ejemplo (3 orgs, 15 usuarios)
- ✅ Docker compose listo en 2 comandos

**Pospuesto a v0.3**:
- ❌ Plugin MCP para Claude Code
- ❌ Integración con Cursor/Copilot
- ❌ Vector search
- ❌ Policy engine

**Ver [06-ENTERPRISE_MVP.md](./06-ENTERPRISE_MVP.md) para el plan completo.**
