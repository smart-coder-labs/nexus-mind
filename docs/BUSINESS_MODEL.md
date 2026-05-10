# NexusMind — Business Model

> **Documento**: BUSINESS_MODEL.md
> **Versión**: 2.0
> **Fecha**: Mayo 2026
> **Propósito**: Modelo de negocio actualizado — sin costos de LLM propios, ingresos por SaaS + Enterprise.

---

## 1. Principio Fundamental

**NexusMind NO asume costos de LLM.** El cliente trae sus propias API keys (BYOM — Bring Your Own Model). Esto:

1. Elimina nuestro riesgo de costos de inferencia
2. Nos permite escalar sin preocuparnos por márgenes de LLM
3. Da al cliente control total sobre qué modelos usa y cuánto gasta
4. Nos posiciona como capa de valor, no como commodity

**NexusMind cobra por el control plane, no por los tokens.**

---

## 2. Modelo de Ingresos

| Fuente | Descripción | % Esperado (Year 3) |
|---|---|---|
| **SaaS (Team)** | Suscripción mensual/anual | 40% |
| **Enterprise (Self-hosted)** | Licencia anual + soporte | 45% |
| **Professional Services** | Onboarding, integraciones custom | 10% |
| **Marketplace (Year 2)** | Plugins de terceros, revenue share | 5% |

---

## 3. Pricing Tiers

### Open Source
**$0/mes**
- Memory API (limitado: 100MB/proyecto)
- Policy engine básico (hasta 3 políticas)
- Audit trail local
- Plugins comunitarios (MCP, Claude Code, Cursor)
- Sin soporte oficial
- Licencia Apache 2.0

Propósito: Adopción viral, comunidad, feedback, plugins.

### Team
**$49/mes** (hasta 10 usuarios)
**$99/mes** (hasta 25 usuarios)
**$199/mes** (hasta 50 usuarios)

- Memoria ilimitada por proyecto
- Policy engine completo (políticas ilimitadas)
- Audit trail con 90 días de retención
- Admin console (dashboard, analytics)
- SSO (Google, GitHub, Microsoft)
- Soporte email (48h SLA)
- Todos los plugins oficiales

### Enterprise
**Desde $499/mes** (por 50+ usuarios)

- **Self-hosted** (Docker/K8s/single binary)
- Memoria ilimitada, proyectos ilimitados
- Audit trail con retención configurable (hasta 7 años)
- On-prem: datos nunca salen de tu infraestructura
- SOC2 compliance
- SLA 99.99%
- Soporte dedicado (chat + video)
- Onboarding asistido (48h)
- Políticas custom avanzadas
- SSO enterprise (SAML, OIDC, Azure AD, Okta)

### Enterprise Plus (Custom)
**Desde $1,999/mes**
Todo lo de Enterprise, más:
- Integraciones custom con herramientas propietarias
- Consultoría de AI governance
- Data residency específica (EU, APAC)
- HIPAA compliance
- Audit trails inmutables con hash chain verificable

---

## 4. Unidades Económicas

### SaaS (Team)

| Métrica | Valor |
|---|---|
| **Precio promedio** | $99/mes (~20 users) |
| **CAC** | $1,200 |
| **COGS** | $5/mes (infra + soporte) |
| **Margen bruto** | 95% |
| **Churn esperado** | 3-5% mensual |
| **LTV** | ~$2,900 |
| **Payback** | ~12 meses |
| **LTV/CAC** | ~2.4x |

### Enterprise (Self-hosted)

| Métrica | Valor |
|---|---|
| **Precio promedio** | $500/mes |
| **CAC** | $2,500 |
| **COGS** | ~$50/mes (soporte dedicado) |
| **Margen bruto** | 90% |
| **Churn esperado** | <1% mensual |
| **LTV** | ~$49,500 |
| **Payback** | ~5 meses |
| **LTV/CAC** | ~19.8x |

---

## 5. Proyecciones Financieras

### Year 1: Seed / Early Traction (2027)

| Mes | Clientes | MRR |
|---|---|---|
| M1 (Launch) | 5 | $500 |
| M3 | 25 | $3,500 |
| M6 | 75 | $12,000 |
| M12 | 200 | $45,000 |

**ARR Year 1: ~$200K** (considerando crecimiento)
**Total raised needed**: $500K (Seed)

### Year 2: Growth

**ARR Target**: $1.2M
**Crew**: 8-12 personas
**Series A target**: $4M

### Year 3: Scale

**ARR Target**: $4.5M
**Crew**: 20-25 personas
**Profitability**: Q4 Year 3

---

## 6. Estrategia de Crecimiento

### Fase 1: Open-source (0-6 meses)
- Core open-source bajo Apache 2.0
- Plugins para Claude Code, Cursor, Copilot
- Comunidad en Discord/GitHub
- Waitlist para Team/Enterprise

### Fase 2: Self-serve SaaS (6-12 meses)
- Team plan con auto-activación
- Free tier con funcionalidad limitada
- Upgrade path a Enterprise

### Fase 3: Enterprise Sales (12-24 meses)
- Equipo de ventas enterprise
- SOC2 + compliance
- On-prem deployment

---

## 7. Competitive Moat

| Moat | Descripción | Sostenibilidad |
|---|---|---|
| **Integraciones** | Plugins para cada herramienta popular | Alta (efecto red) |
| **Datos de usuario** | Memoria + policies + audit trails | Muy alta (switching cost) |
| **Open-source core** | Confianza, transparencia | Media (code es copiable) |
| **Protocolo abierto** | MCP estándar, no vendor lock-in | Media (pero requiere implementación) |

---

*Fin de BUSINESS_MODEL.md v2.0*
