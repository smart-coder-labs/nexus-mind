# NexusMind — MVP Pitch (v1 — Archivado)

> ⚠️ **Este pitch corresponde al plan v1 (developer-focused).**
> El plan actual es enterprise. Ver [06-ENTERPRISE_MVP.md](./06-ENTERPRISE_MVP.md) para el pitch actualizado.

> **Documento**: 04-MVP_PITCH.md
> **Versión**: 0.1.0 (archivado)
> **Propósito**: Explicación ejecutiva de por qué este MVP — no el de 16 semanas del ADR-001.

---

## 1. El Problema (Actual)

El repo tiene **13 documentos de concepto, 2 ADRs aceptados y una landing page**, pero **cero líneas de código del producto real**.

Lo que existe:
- `docs/PRD.md` — Producto soñado para 24 meses
- `docs/ARCHITECTURE.md` — Sistema que requiere 3 equipos
- `docs/ROADMAP.md` — 6 meses de Fase 1
- `docs/adr/ADR-001.md` — Arquitectura Rust con 8+ crates (16 semanas para Fase 1)
- `docs/adr/ADR-002.md` — Store Abstraction Trait con SQLite → Postgres
- `apps/landing/` — Landing page con waitlist

Lo que NO existe:
- Código del backend (Rust o cualquier otro)
- Base de datos
- API funcional
- Plugin para ninguna herramienta
- Ningún usuario usando el producto

**El riesgo**: Seguir documentando y diseñando arquitectura en vez de construir.

---

## 2. La Tesis a Validar

> **"Un sistema de memoria cross-tool, accesible vía API REST y MCP, permite a developers mantener contexto entre sesiones y entre herramientas AI."**

Esta tesis se puede validar con:
- **1 backend Rust** (~1200 líneas)
- **1 plugin MCP** para Claude Code (~500 líneas TS)
- **4 semanas**

Si funciona, la expansión a la arquitectura completa de ADR-001 (8+ crates, TUI, Merkle audit trail, policy engine, vectores, etc.) tiene sentido. Si no, mejor saberlo pronto.

---

## 3. ¿Por qué este MVP y no el del ADR-001?

| ADR-001 Fase 1 (16 semanas) | MVP Propuesto (4 semanas) |
|---|---|
| 8+ crates separados (core, store, auth, audit, server, mcp, cli, tui) | 1 crate plano `src/` |
| TUI completa con Ratatui | Sin TUI |
| Merkle audit trail + Ed25519 | Audit trail append-only simple |
| Policy engine ABAC (<50μs por regla) | Sin policy engine |
| CLI con 5+ subcomandos | 2 subcomandos (serve, keygen) |
| Arquitectura multi-crate lista para escalar | Arquitectura plana, refactorizable |
| **Validación en semana 16** | **Validación en semana 4** |

### Matemática Simple

```
ADR-001 Fase 1:
  16 semanas × 1 developer = 16 developer-weeks antes de validar
  Riesgo: construir features que nadie usa

MVP propuesto:
  4 semanas × 1 developer = 4 developer-weeks antes de validar
  Riesgo: solo la memoria cross-tool
```

---

## 4. El MVP en una Frase

> **Un backend Rust que guarda memos con texto y tags en SQLite (FTS5), y un plugin de Claude Code para leerlos y escribirlos. Eso es todo. El resto (TUI, vectores, policy engine, Merkle audit, 8 crates) se construye si alguien lo pide.**

---

## 5. Métricas de Éxito del MVP

| Métrica | Target | Cómo se mide |
|---|---|---|
| Developers usando el plugin | 10 | GitHub releases descargados |
| Memorias almacenadas | 500+ | Query a la DB |
| Búsquedas realizadas | 1000+ | Audit trail |
| Tiempo de setup | <5 min | README seguido por dev externo |
| Issues/feedback | 5+ issues meaningful | GitHub Issues |
| Uso semanal recurrente | 5+ developers >1 semana | Unique users en audit trail |

Si alguna de estas métricas no se alcanza en 4 semanas, pivoteamos en lugar de escalar.

---

## 6. Costo del MVP

| Recurso | Costo |
|---|---|
| Developer (1, 4 semanas) | ~$8,000 - $12,000 |
| Infra (Docker host básico) | ~$20/mes |
| API keys de LLM (BYOM) | $0 (cliente trae las suyas) |
| Domain + DNS | ~$15/año |
| **Total** | **~$8,500** |

Para escalar a Fase 1 del ADR-001 (16 semanas, equipo completo):
- 2-3 developers
- Infra K8s
- ~$120,000 - $180,000

**El MVP cuesta ~5-7% de lo que costaría la Fase 1 completa del ADR-001.**

---

## 7. Qué Hacer Después del MVP

### Si el MVP funciona (métricas verdes)
```
Sem 5-6:   Vector search (Candle ONNX) + Policy engine básico
Sem 7-8:   Plugin Cursor + Merkle audit trail
Sem 9-12:  SDK TypeScript + Admin console mejorada
Sem 13-16: Multi-crate refactor + TUI (Ratatui)
     →   Contratar dev #2, buscar early customers
```

### Si el MVP no funciona (métricas rojas)
```
Pivotar:
- ¿El problema no es memoria sino gobernanza? → Policy engine first
- ¿Claude Code no es la tool correcta? → Probar con Cursor
- ¿El mercado no quiere esto? → Cancelar
```

---

*Fin de 04-MVP_PITCH.md*
