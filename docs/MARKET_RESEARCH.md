# NexusMind — Market Research

> **Documento**: MARKET_RESEARCH.md
> **Versión**: 2.0
> **Fecha**: Mayo 2026
> **Propósito**: Análisis del mercado de control planes AI enterprise — un espacio emergente donde no se compite con herramientas AI, sino que se les da gobierno, memoria y orquestación.

---

## 1. El Problema de Mercado (Reformulado)

El mercado de herramientas AI para developers está **saturado y fragmentado**. Hay decenas de herramientas excelentes: Claude Code, Cursor, Copilot, OpenCode, Cline, Windsurf, CodeGPT, y muchas más. Cada una es buena en lo suyo.

**El problema no es que falten herramientas. Es que sobran, están aisladas, y nadie las gobierna.**

### Síntomas del Cliente

| Síntoma | Frecuencia | Dolor |
|---|---|---|
| Equipos usando 3+ herramientas AI distintas | Muy alta | Costos duplicados, sin visibilidad |
| Conocimiento perdido entre sesiones/herramientas | Alta | Developers repiten contexto |
| Sin audit trails de interacciones AI | Muy alta | Riesgo compliance |
| Datos sensibles pasando por LLMs sin control | Alta | Riesgo legal |
| CTO sin visibilidad de qué hacen los agentes | Muy alta | Falta de control |

### El Insight Clave

Las empresas **no quieren** que les digan qué herramienta usar. Quieren que cualquier herramienta que elijan **cumpla con sus reglas**.

---

## 2. TAM / SAM / SOM

### TAM (Total Addressable Market)
**$12.4B USD para 2028**

Mercado global de herramientas AI enterprise para developers + agentes + gobernanza.
*Fuente: Gartner "Market Guide for AI Code Assistants", McKinsey "The State of AI in 2025"*

### SAM (Serviceable Addressable Market)
**$3.1B USD**

Empresas con 50-500 empleados que usan herramientas AI y necesitan control/gobierno.
- Empresas target: 120,000 globalmente
- Gasto promedio anual en tools AI: $26,000
- Segmento: CTOs conscientes de gobernanza AI (estimado 40%)

### SOM (Serviceable Obtainable Market)
**$120M USD (Year 3 target)**

Empresas early adopter que priorizan control y están dispuestas a pagar por un control plane.
- Empresas obtenibles: 2,000 (Year 3)
- ARPU promedio: $60,000/año

---

## 3. Competencia (Reformulada)

**NexusMind no compite con herramientas AI. Compite con el *vacío* — la ausencia de un control plane.**

### Competidores Indirectos (No compiten directamente, pero podrían)

| Compañía | Qué ofrecen | Por qué no resuelven el problema |
|---|---|---|
| **LangSmith / LangFuse** | Observabilidad de LLM calls | Solo tracing, no políticas ni memoria cross-tool |
| **Guardrails AI** | Validación de outputs de LLM | Solo validación, no orquestación ni memoria |
| **Weights & Biases** | ML experiment tracking | No es para producción, no es cross-tool |
| **Helicone** | Proxy de LLM con logging | Solo proxy, sin policies engine ni memoria |
| **Portkey** | Gateway de LLM | Sin memoria persistente ni cross-tool |

### Competidores Potenciales (Podrían pivotar)

| Compañía | Riesgo | Por qué |
|---|---|---|
| **Anthropic (Claude)** | Medio | Podrían añadir capa enterprise sobre Claude Code |
| **Microsoft (Copilot)** | Medio | Podrían integrar governance en GitHub |
| **Cursor** | Bajo | Están enfocados en IDE, no en control plane |

### Nuestro Diferencial Clave

1. **Tool-agnostic**: No importa qué herramienta use tu equipo. NexusMind funciona con todas.
2. **BYOM (Bring Your Own Model)**: No asumimos costos de LLM. El cliente trae sus keys.
3. **Exportabilidad total**: Sin lock-in. Tu memoria, policies y audit trails se exportan.
4. **Open-source core**: Transparencia. El core es público y auditable.
5. **MCP Protocol**: Compatibilidad estándar con el ecosistema.

---

## 4. Tendencias de Mercado

### 4.1 Multi-agent es el presente
Empresas ya no usan un solo agente AI. Usan múltiples. El problema de gobernanza multi-agente crece exponencialmente.

### 4.2 Fragmentación de herramientas
Cada mes sale una nueva herramienta AI. Los CTOs no pueden mantener el ritmo. Necesitan una capa que abstraiga la complejidad.

### 4.3 Regulación AI (EU AI Act)
La regulación europea exige trazabilidad de decisiones de AI. Las empresas necesitan audit trails.

### 4.4 Costos de LLM como preocupación #1
Empresas gastan $50k-$500k/año en APIs de LLM. Quieren control, no otro costo.

### 4.5 MCP como estándar emergente
El Model Context Protocol de Anthropic se está convirtiendo en estándar para integración de tools AI. NexusMind lo adopta nativamente.

---

## 5. Segmentos de Clientes

| Segmento | Tamaño | Dolor | Disposición a pagar |
|---|---|---|---|
| **Startups (10-50)** | 50,000+ | Usan 1-2 tools, poco dolor aún | Baja |
| **Scaleups (50-200)** | 30,000+ | Usan 3-5 tools, dolor alto | Alta |
| **Mid-market (200-1000)** | 15,000+ | Compliance obligatorio, dolor muy alto | Muy alta |
| **Enterprise (1000+)** | 5,000+ | Governance mandatorio | Muy alta (proceso largo) |

**Target primario**: Scaleups (50-200 empleados) — suficiente dolor, suficiente budget, proceso de decisión ágil.

---

## 6. Pricing Validation

| Plan | Precio Mensual | ARPU Anual | Margen |
|---|---|---|---|
| Open Source | $0 | $0 | N/A (lead gen) |
| Team (hasta 50 seats) | $49/mes | $588 | 80%+ (SaaS) |
| Enterprise (>50 seats) | Custom (~$199-499/mes) | $3,000-6,000 | 85%+ (self-hosted) |

**CAC estimado**: $1,200 (SaaS), $2,500 (Enterprise)
**LTV estimado**: $8,800 (SaaS), $45,000 (Enterprise)
**Payback period**: 6-8 meses

---

## 7. Estrategia de Go-to-Market

1. **Open-source core** → Adopción orgánica por developers
2. **Plugins para Claude Code, Cursor, Copilot** → Distribución en marketplaces existentes
3. **Landing page con waitlist** → Captura de leads calificados
4. **Content marketing** → "Cómo gobernar AI agents en tu empresa"
5. **Outbound a CTOs** → Dolor validado, solución clara

---

*Fin de MARKET_RESEARCH.md v2.0*
