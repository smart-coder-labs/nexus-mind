# NexusMind — MVP Docs

> Documentación del plan para sacar un MVP funcional en **4 semanas**.

---

## 📋 Documentos

| # | Documento | Qué contiene |
|---|---|---|
| 1 | [01-MVP_PLAN.md](./01-MVP_PLAN.md) | Plan general: filosofía, sprint breakdown, feature mapping, riesgos |
| 2 | [02-ARCHITECTURE_MVP.md](./02-ARCHITECTURE_MVP.md) | Arquitectura reducida, data model, endpoints, diff con la target |
| 3 | [03-SCOPE_CHANGELOG.md](./03-SCOPE_CHANGELOG.md) | Qué se excluye, qué se reduce, timeline ajustado |
| 4 | [04-MVP_PITCH.md](./04-MVP_PITCH.md) | Pitch ejecutivo: por qué este MVP y no el de 6 meses |
| 5 | [05-TASK_BREAKDOWN.md](./05-TASK_BREAKDOWN.md) | Desglose granular día por día para las 4 semanas |

---

## 🎯 Resumen

**Meta**: Un backend Go + SQLite + Plugin MCP para Claude Code que permita memoria cross-tool funcional.

**Scope**: Memory API (store/search/delete), auth por API key, admin web mínima, MCP server.

**Excluido**: Policy engine, vector search, SDKs, SSO, multi-agent, Cursor/Copilot plugins.

**Costo**: ~1 developer × 4 semanas (~$8,500).

**Riesgo principal**: Scope creep. La regla es "hard cutoff en semana 4".
