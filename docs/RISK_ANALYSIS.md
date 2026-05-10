# NexusMind — Risk Analysis

> **Documento**: RISK_ANALYSIS.md
> **Versión**: 2.0
> **Fecha**: Mayo 2026
> **Propósito**: Análisis de riesgos actualizado. El mayor riesgo ya no es el costo de LLM (lo eliminamos con BYOM), sino la adopción de un control plane en un mercado fragmentado.

---

## 1. Risk Matrix

| # | Riesgo | Probabilidad | Impacto | Score | Mitigación |
|---|---|---|---|---|---|
| **R1** | Baja adopción del control plane (mercado no maduro) | Alta | Alto | 🔴 | Open-source core para adopción orgánica; plugins para tools existentes; contenido educativo |
| **R2** | Herramientas AI añaden su propio gobierno | Media | Alto | 🔴 | Diferencial cross-tool (ninguna tool cubre todas); velocidad de integración |
| **R3** | Fragilidad del modelo BYOM (CLOs no quieren gestionar LLMs) | Media | Medio | 🟡 | Auto-provisioning de modelos; managed LLM como add-on enterprise |
| **R4** | Dependencia de APIs de terceros (Cursor, Copilot) | Alta | Bajo | 🟡 | APIs públicas estables; MCP como protocolo abierto; fallback modes |
| **R5** | Competidores indirectos (LangFuse, Guardrails) pivotan | Media | Medio | 🟡 | Ventaja de 12+ meses; memoria cross-tool es compleja de implementar |
| **R6** | Compliance retrasa ventas enterprise | Alta | Medio | 🟡 | SOC2 desde el día 1; documentación compliance incluida; on-prem disponible |
| **R7** | Single binary SQLite no escala para grandes clientes | Media | Bajo | 🟢 | pgvector + PostgreSQL como opción de scale documentada desde MVP |
| **R8** | Churn en Team plan | Media | Medio | 🟡 | Open-source core reduce barrera de salida; exportabilidad total; feedback loops |
| **R9** | Dependencia de un CTO champion para venta | Alta | Alto | 🔴 | Contenido para VP Eng, Compliance, IT; venta multi-stakeholder |

---

## 2. Análisis de Riesgos Clave

### R1: Baja adopción del control plane

**Por qué es el #1**: El concepto de "control plane para AI tools" es nuevo. Los CTOs pueden no ver el problema hasta que es demasiado tarde (data leak, auditoría fallida, fuga de talento con contexto).

**Indicadores tempranos**:
- Waitlist signups <50 en primeros 3 meses
- Plugins descargados <100 en primeros 6 meses
- Conversión trial → pago <5%

**Plan de contingencia**:
- Pivotar a feature específica (solo memoria cross-tool como entry point)
- Reducir scope: memory-only como producto independiente
- Partner con consultoras que vendan AI governance

### R2: Herramientas añaden su propio gobierno

**Por qué es riesgo**: Si Claude Code añade "Claude Enterprise Governance" o Cursor añade "Cursor Policies", reduce el valor percibido de NexusMind.

**Defensa**: 
- Cross-tool será siempre el diferencial. Ninguna herramienta va a gobernar *a sus competidores*.
- Velocidad de integración. Si salen 10 tools nuevas al año, NexusMind debe integrarlas antes que nadie.
- Protocolo abierto (MCP). Cuantas más tools adopten MCP, más fácil es integrarlas.

### R3: Fragilidad del BYOM

**Por qué es riesgo**: Algunos CTOs quieren "que funcione sin configurar nada". Pedirles que traigan sus propias keys de OpenAI + Anthropic + Google es fricción.

**Mitigaciones**:
1. Default: Auto-provisioning con modelo open-source (Llama 3) para que funcione out-of-box
2. Upgrade path: "Bring your own key" para desbloquear modelos premium
3. Enterprise: Managed LLM (NexusMind compra al por mayor, revende con markup opcional)

---

## 3. Riesgos Mitigados (v1.0 → v2.0)

| Riesgo (v1.0) | Estado v2.0 | Explicación |
|---|---|---|
| Costos de LLM impredecibles | ✅ Eliminado | BYOM desde el día 1. El cliente paga sus LLMs. |
| Dependencia de un solo proveedor LLM | ✅ Eliminado | BYOM es multi-modelo por diseño. |
| Pricing per-token complejo | ✅ Simplificado | Pricing per-seat fijo. Sin sorpresas. |
| Competencia directa con Copilot/Claude | ✅ Reformulado | No competimos. Los integramos. |

---

## 4. Compliance Roadmap

| Certificación | Target | Prioridad |
|---|---|---|
| **SOC2 Type I** | Month 6 | Alta (ventas enterprise) |
| **SOC2 Type II** | Month 12 | Alta (contratos grandes) |
| **GDPR compliance** | Day 1 | Alta (built-in) |
| **EU AI Act readiness** | Month 6 | Alta (mercado europeo) |
| **HIPAA** | Month 12 | Media (sector salud) |
| **ISO 27001** | Year 2 | Media (enterprise global) |

---

## 5. Plan de Continuidad

### Si el mercado no adopta control planes...
1. **Pivot a Memory-as-a-Service**: Solo el componente de memoria cross-tool como producto independiente.
2. **Pivot a policy-only**: Solo policy engine como middleware para empresas reguladas.
3. **Adquiridos**: Engineering team + tecnología embedded en plataforma existente.

### Si una Big Tech lanza control plane...
1. **Diferenciación**: Multi-tool vs. single-vendor. Seguir siendo el agnóstico.
2. **Open-source**: Comunidad existente es difícil de replicar.
3. **Velocidad**: Integrar tools nuevas más rápido que cualquier competidor.

---

*Fin de RISK_ANALYSIS.md v2.0*
