# NexusMind — MVP Pitch

> **Documento**: 04-MVP_PITCH.md
> **Versión**: 0.1.0
> **Propósito**: Explicación ejecutiva de por qué este MVP — no el de 6 meses del roadmap original.

---

## 1. El Problema (Actual)

El repo tiene **13 documentos de concepto** y **cero líneas de código del producto real**.

Lo que existe:
- `docs/PRD.md` — Producto soñado para 24 meses
- `docs/ARCHITECTURE.md` — Sistema que requiere 3 equipos
- `docs/ROADMAP.md` — 6 meses de Fase 1 con 5+ plugins
- `apps/bootcamp-tracker/` — Proyecto legacy no relacionado
- `apps/landing/` — Landing page con waitlist

Lo que NO existe:
- Código del backend
- Base de datos
- API funcional
- Plugin para ninguna herramienta
- Ningún usuario usando el producto

**El riesgo**: Seguir documentando en vez de construir.

---

## 2. La Tesis a Validar

> **"Un sistema de memoria cross-tool, accesible vía API REST y MCP, permite a developers mantener contexto entre sesiones y entre herramientas AI."**

Esta tesis se puede validar con:
- **1 backend Go**
- **1 plugin MCP** para Claude Code
- **4 semanas**

Si funciona, la expansión a policy engine, más plugins, SDKs, etc. tiene sentido. Si no, mejor saberlo pronto.

---

## 3. ¿Por qué este MVP y no el del Roadmap?

| Roadmap Original (6 meses) | MVP Propuesto (4 semanas) |
|---|---|
| 5 plugins | 1 plugin (Claude Code) |
| Policy engine completo | Sin policy engine |
| SDKs en 3 lenguajes | Sin SDKs (REST API sola) |
| Audit trail inmutable | Audit trail simple |
| Admin console completa | Admin mínima |
| SSO + RBAC | API key sola |
| Vector search | FTS5 |
| **$0 revenue hasta M6** | **Validación en M1** |

### Matemática Simple

```
Roadmap original:
  6 meses × N developers × $X/mes = $6NX antes de validar
  Riesgo: construir features que nadie usa

MVP propuesto:
  1 mes × 1 developer = 1 developer-month antes de validar
  Riesgo: solo la memoria cross-tool
```

---

## 4. El MVP en una Frase

> **Un backend Go que guarda memos con texto y tags en SQLite, y un plugin de Claude Code para leerlos y escribirlos. Eso es todo. El resto se construye si alguien lo pide.**

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

Para escalar a Fase 1 del Roadmap (6 meses, equipo completo):
- 2-3 developers
- Infra K8s
- ~$120,000 - $180,000

**El MVP cuesta ~5% de lo que costaría la Fase 1 completa.**

---

## 7. Qué Hacer Después del MVP

### Si el MVP funciona (métricas verdes)
```
Sem 5-6:  Vector search + Policy engine básico
Sem 7-8:  Plugin Cursor
Sem 9-12: SDK TypeScript + Admin console mejorada
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
