# NexusMind — Business Model & Strategy

> **Documento**: BUSINESS_MODEL.md  
> **Versión**: 1.0  
> **Fecha**: Mayo 2026  
> **Propósito**: Modelo de negocio, pricing, TAM/SAM/SOM, y estrategia go-to-market.

---

## 1. Product Vision

**NexusMind** es la plataforma AI unificada para empresas que combina:

```
┌─────────────────────────────────────────────────────────┐
│                    NEXUSMIND                             │
├─────────────────┬───────────────────────────────────────┤
│  For Developers │  For Everyone Else                    │
│                 │                                       │
│  • AI Coding    │  • Pre-built Agents                   │
│  • Memory Sys   │    (Support, Ops, Data)                │
│  • Sub-agents   │  • No-code Agent Builder              │
│  • Workflows    │  • Knowledge Base Integration         │
│  • CLI + API    │  • Dashboard + Reports                │
├─────────────────┴───────────────────────────────────────┤
│                 Enterprise Layer                         │
│  RBAC │ Audit │ SSO │ Compliance │ Billing │ On-prem    │
└─────────────────────────────────────────────────────────┘
```

---

## 2. Pricing Model

### 2.1 Tier Structure

| Feature | Developer ($29/mo) | Team ($49/seat/mo) | Enterprise (Custom) |
|---|---|---|---|
| **AI Coding Assistant** | ✅ | ✅ | ✅ |
| **Persistent Memory** | ✅ (5GB) | ✅ (50GB) | ✅ (ilimitado) |
| **Memory Search** | FTS only | Hybrid (FTS+Vector) | Hybrid + Custom |
| **Multi-model Gateway** | 3 models | 8 models | Todos + BYOM |
| **Sub-agent Spawn** | 5 concurrentes | 20 concurrentes | Ilimitados |
| **Workflow DAG** | ❌ | ✅ (10 workflows) | ✅ (ilimitados) |
| **Admin Console** | ❌ | ✅ | ✅ + Custom |
| **RBAC** | ❌ | Roles básicos | Roles custom |
| **SSO** | ❌ | ❌ | ✅ (SAML/OIDC/SCIM) |
| **Audit Trails** | ❌ | ❌ | ✅ (inmutables) |
| **On-prem Option** | ❌ | ❌ | ✅ |
| **SOC2 Compliance** | ❌ | ❌ | ✅ |
| **SLA** | 99.7% | 99.9% | 99.95% |
| **Support** | Community | Email (4h) | Dedicado (1h) |
| **Agent Usage Credits** | ❌ | 5,000/mo/seat | Custom pool |
| **Data Residency** | ❌ | ❌ | ✅ (EU, US, APAC) |

### 2.2 Agent Usage Credits

Las empresas necesitan que los no-desarrolladores ejecuten agentes sin consumir "seats" de developer:

```
Developer Seat  → $49/mo (incluye 5,000 agent runs/seat)
                    → $0.005/run extra

Agent Credit Pool:
  10,000 runs/mes  → $199/mo  (para equipos pequeños)
  50,000 runs/mes  → $799/mo  (departamental)
  200,000 runs/mes → $2,499/mo (organizacional)
  1,000,000+/mes   → Custom pricing
```

### 2.3 Add-ons

| Add-on | Developer | Team | Enterprise |
|---|---|---|---|
| **Extra Storage (50GB)** | $10/mo | $20/mo | $50/mo |
| **Model Gateway (extra model)** | $5/mo | $5/mo | Custom |
| **Knowledge Base Connector** | ❌ | $50/mo/connector | $20/mo/connector |
| **On-prem Deployment Fee** | ❌ | ❌ | $5,000/mo (min 12 meses) |
| **Custom Agent Template** | ❌ | $500 flat | Included |

### 2.4 Pricing Comparativa vs Competencia

```
Competitor         │ Per-Seat/Mo  │ Memory │ Orchest │ Enterprise │ Total/anual(100devs)
───────────────────┼──────────────┼────────┼─────────┼────────────┼────────────────────
GitHub Copilot     │ $10-39       │   ❌   │   ❌   │   Parcial  │ $12k-46.8k
Cursor             │ $20-40       │   ❌   │   ❌   │   Parcial  │ $24k-48k
Windsurf           │ $15-60       │   ❌   │   ❌   │   ❌       │ $18k-72k
Amazon Q           │ $19-39       │   ❌   │   ❌   │   ✅       │ $22.8k-46.8k
───────────────────┼──────────────┼────────┼─────────┼────────────┼────────────────────
NexusMind (Team)   │ $49          │   ✅   │   ✅   │   ✅       │ $58.8k
NexusMind (Dev)    │ $29          │   ✅   │   ❌   │   ❌       │ $34.8k
───────────────────┴──────────────┴────────┴─────────┴────────────┴────────────────────
Al combinarlo: Copilot $46.8k + CrewAI $12k + Mem0 $3k + Retool $24k = $85.8k
vs NexusMind = $58.8k → 31% de ahorro
```

---

## 3. TAM / SAM / SOM

### 3.1 Market Sizing

**TAM (Total Addressable Market)**: $14.21B (2024 AI Platform Market)

```
┌──────────────────────────────────────────────────────────────────┐
│                          TAM: $14.21B                             │
│  AI Platform Market (all segments)                                │
│                                                                   │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │                  SAM: $6.27B                                │  │
│  │  Agentic AI Orchestration & Memory Systems                  │  │
│  │                                                             │  │
│  │  ┌──────────────────────────────────────────────────────┐  │  │
│  │  │          SAM-targetable: ~$2.5B                      │  │  │
│  │  │  Unified platforms (our competitive set)              │  │  │
│  │  │                                                       │  │  │
│  │  │  ┌──────────────────────────────────────────────┐    │  │  │
│  │  │  │  SOM Year 1: $2.5M                            │    │  │  │
│  │  │  │  SOM Year 3: $50M  ← Target                   │    │  │  │
│  │  │  │  SOM Year 5: $200M                            │    │  │  │
│  │  │  └──────────────────────────────────────────────┘    │  │  │
│  │  └──────────────────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

### 3.2 Growth Trajectory

| Year | Market (TAM) | Our Revenue | Market Share |
|---|---|---|---|
| 2026 (launch) | $19.6B | $0.5-2.5M | <0.01% |
| 2027 | $27.1B | $10-25M | 0.04-0.09% |
| 2028 | $37.4B | $50-100M | 0.13-0.27% |
| 2029 | $51.6B | $150-300M | 0.29-0.58% |
| 2030 | $71.2B | $400-600M | 0.56-0.84% |

### 3.3 Revenue Model Assumptions

```
Year 1 (2026):
  - 20 beta customers (10-50 seats each)
  - 500 early adopter seats
  - $49 avg revenue per seat
  - 50/50 Developer vs Team plans
  = ~$300k MRR → ~$2.5M ARR

Year 2 (2027):
  - 200 customers (avg 50 seats)
  - 10,000 seats
  - $44 avg (mix shift to Enterprise)
  - +$200k Enterprise add-ons
  = ~$2.5M MRR → ~$25M ARR

Year 3 (2028):
  - 500 customers (avg 100 seats)
  - 50,000 seats
  - $38 avg (more Enterprise deals, lower per-seat but higher volume)
  - +$1.5M Enterprise add-ons + on-prem
  = ~$6.3M MRR → ~$75M ARR
```

---

## 4. Go-to-Market Strategy

### 4.1 Funnel

```
Top of Funnel (PLG)
├── Free tier for individual developers
│   ├── Basic AI coding assistant
│   ├── 100MB memory
│   ├── 3 models (GPT-4o-mini, Claude Haiku, Gemini Flash)
│   └── Community support
│
├── Content Marketing
│   ├── Blog: "Building persistent memory for AI agents"
│   ├── GitHub: Open-source tools (engram-inspired extracts)
│   ├── YouTube: Tutorials, architecture deep-dives
│   └── Twitter/X: Tips, comparisons, benchmarks
│
├── Developer Relations
│   ├── Conference talks (KubeCon, AI Engineer Summit)
│   ├── Hackathons
│   └── Discord community (2000+ active members target)
└──

Middle of Funnel (Product-led Growth)
├── Self-serve upgrade: Developer → Team ($29 → $49)
├── Team trials (14-day free, no credit card)
├── Usage-based upsell (memory, agent credits)
├── In-app onboarding with agent walkthrough
└──

Bottom of Funnel (Enterprise Sales)
├── BDR team (3 SDRs → 6 AEs → 10 AEs over 2 years)
├── Technical demos + POC (2-week pilot)
├── Security review package (SOC2, audit trails, data residency)
├── Executive sponsorship program (CTO → CTO)
├── Partner channel (SI partners: Deloitte, Accenture)
└──

Post-Sale (Expansion)
├── Seat expansion (land 50 → grow to 500)
├── Product expansion (Coding → Workflows → Non-dev agents)
├── Cross-sell: Agent credit pools
├── Annual renewal with auto-upsell
└──
```

### 4.2 Target Customer Profiles

**ICP 1: Mid-market Tech** (50-500 employees)
- Headcount: 50-500
- Revenue: $10-200M
- Pain point: Multiple AI tools, no governance
- Budget: $20-50k/ano en herramientas AI
- Decision cycle: 4-8 weeks
- Deal size: $15-60k ARR

**ICP 2: Enterprise** (500-5000+ employees)
- Headcount: 500-5000+
- Revenue: $200M-10B+
- Pain point: AI sprawl, compliance, shadow IT
- Budget: $50-200k/ano en AI tools
- Decision cycle: 12-24 weeks
- Deal size: $50-500k ARR

**ICP 3: Government / Regulated** (200-2000 employees)
- Headcount: 200-2000
- Revenue: $50M-5B
- Pain point: Data sovereignty, compliance, security
- Budget: $20-100k/ano (harder to capture)
- Decision cycle: 20-40 weeks
- Deal size: $50-200k ARR

### 4.3 Geographic Focus

**Phase 1 (Year 1)**: US + France
- **US**: Largest market (40% of global AI spend)
- **France**: Home market advantage, EU regulations, strong data residency requirements

**Phase 2 (Year 2)**: UK, Germany, Canada
**Phase 3 (Year 3)**: APAC (Japan, Singapore, Australia)

---

## 5. Business Model Canvas

| Segment | Detail |
|---|---|
| **Value Proposition** | Plataforma AI unificada con memoria persistente, orquestación de agentes, y gobierno enterprise |
| **Customer Segments** | VP Engineering, Developers, Compliance Officers, IT Ops |
| **Channels** | PLG (self-serve), Enterprise Sales, Partner Channel |
| **Customer Relationships** | Community (free), Email/Ticket (Team), Dedicated CSM (Enterprise) |
| **Revenue Streams** | Subscriptions (per-seat), Agent Credits (usage), Add-ons (storage, connectors), On-prem fees |
| **Key Resources** | Go core team, LLM partnerships, Reference customers |
| **Key Activities** | Platform development, Enterprise sales, Community building |
| **Key Partners** | LLM providers (OpenAI, Anthropic, Google), Cloud providers (AWS, GCP, Azure), SI partners |
| **Cost Structure** | R&D (40%), G&A (15%), S&M (35%), Infra (10%) |
| **Cost Drivers** | Salarios ($1.2M-2.5M/ano), Cloud infra ($50-500k/mo escaling), LLM API costs ($0.01-0.10/run) |

---

## 6. Unit Economics

### 6.1 Cost per Seat (Enterprise)

```
Revenue per seat:     $49/mo
COGS:
  - LLM API costs:    $8.50/mo (avg 3,000 runs/seat/mo)
  - Infrastructure:   $3.20/mo (storage, compute, bandwidth)
  - Support:           $1.80/mo
Total COGS:           $13.50/mo
Gross Margin:           72.4%

Contribution margin:  $35.50/seat/mo
```

### 6.2 Customer Acquisition Cost

```
Enterprise deal:
  - Sales cycle: 16 weeks
  - Cost per AE: $150k/year all-in
  - Deals closed per AE per year: 12
  - CAC per deal: $12,500 ($150k / 12)

  - Including SDR ($80k), marketing ($50k attribution):
  - Blended CAC: ~$15,000/deal

  - Average deal size (year 1): $40k ARR
  - Payback period: 4.5 months ($15k / $3.3k monthly margin)

PLG deal:
  - Self-serve, no sales cost
  - Marketing attributed: $75/CAC
  - Payback: 2 months
```

### 6.3 LTV Projection

```
Enterprise:
  - Average deal: $40,000 ARR
  - Gross margin: 72%
  - Churn rate: 5% annual (enterprise)
  - LTV: $40k * 0.72 / 0.05 = $576,000
  - LTV/CAC: $576k / $15k = 38x ✅

Team (self-serve):
  - Average deal: $5,880 ARR (10 seats * $49)
  - Gross margin: 72%
  - Churn rate: 15% annual
  - LTV: $5.88k * 0.72 / 0.15 = $28,224
  - LTV/CAC: $28k / $100 = 280x ✅
```

---

## 7. Competitive Positioning

### 7.1 Positioning Statement

> **For enterprises tired of managing 5 different AI tools, NexusMind is the unified platform that combines coding assistance, persistent memory, and agent orchestration with enterprise-grade governance — so you get one bill, one dashboard, and one compliance framework instead of a fragmented mess.**

### 7.2 Competitive Moats

1. **Data Network Effect**: Mientras más usan NexusMind los equipos, mejor es la memoria compartida → más valioso el producto → más difícil de dejar
2. **Enterprise Integration Depth**: Conectores profundos con el stack enterprise (SAML, SCIM, audit trails) crean switching costs
3. **Multi-agent Memory**: La memoria no es por agente, es compartida a nivel organización, lo que ningún competidor hace bien
4. **Dual Persona**: Única plataforma que sirve tanto a devs como a no-devs, reduciendo tool sprawl
5. **On-prem + Cloud**: Las empresas reguladas necesitan on-prem; nadie en este espacio lo ofrece bien

---

## 8. Funding Strategy

### 8.1 Funding Roadmap

| Round | Amount | Timeline | Milestones |
|---|---|---|---|
| Pre-seed | $500k | Q3 2025 | Research, MVP, market validation |
| Seed | $3-5M | Q1 2026 | Core + Memory + 10 beta customers |
| Series A | $10-15M | Q4 2026 | Agents + Orchestration + $1M ARR |
| Series B | $30-50M | Q3 2027 | Enterprise sales + $10M ARR |

### 8.2 Use of Funds (Seed Round: $4M)

```
R&D (50%): $2M
  - 4 engineers: $600k
  - 2 AI/ML engineers: $400k 
  - 1 frontend: $150k
  - 1 DevOps: $150k
  - Cloud infra: $200k
  - LLM API credits: $200k
  - Software + tools: $100k

G&A (15%): $600k
  - Legal + accounting: $100k
  - Office: $200k
  - Compliance (SOC2 prep): $200k
  - Insurance: $100k

S&M (35%): $1.4M
  - 1 Head of Sales: $200k
  - 2 SDRs: $160k
  - Marketing (content, events): $300k
  - Developer relations: $100k
  - Paid acquisition: $640k
```

---

## 9. Revenue Scenarios

### Scenario 1: Conservative (PLG-heavy, slow enterprise)

```
Year 1: $1.5M ARR (500 seats, $49 avg)
Year 2: $10M ARR (5,000 seats, $40 avg)
Year 3: $40M ARR (25,000 seats, $35 avg)
Year 4: $100M ARR (70,000 seats, $32 avg)
Year 5: $200M ARR (150,000 seats, $30 avg)
Valuation (5x ARR): $1B
```

### Scenario 2: Base Case (balanced PLG + Enterprise)

```
Year 1: $2.5M ARR (800 seats, $49 avg)
Year 2: $25M ARR (12,500 seats, $42 avg)
Year 3: $75M ARR (40,000 seats, $38 avg)
Year 4: $200M ARR (110,000 seats, $35 avg)
Year 5: $400M ARR (250,000 seats, $32 avg)
Valuation (8x ARR): $3.2B
```

### Scenario 3: Aggressive (enterprise-heavy)

```
Year 1: $5M ARR (1,500 seats, $49 avg)
Year 2: $50M ARR (20,000 seats, $45 avg)
Year 3: $150M ARR (75,000 seats, $40 avg)
Year 4: $400M ARR (200,000 seats, $36 avg)
Year 5: $800M ARR (500,000 seats, $33 avg)
Valuation (10x ARR): $8B
```

---

## 10. Key Metrics Dashboard

### 10.1 North Star Metric

**Weekly Active Developer Sessions (WADS)**: Una sesión = un developer interactuando con NexusMind (chat, agent run, workflow execute).

### 10.2 Leading Indicators

| Metric | Target |
|---|---|
| Daily Active Users (DAU) | >40% of total seats |
| Actions per session | >5 (significa que están explorando features) |
| Memory saves per user/day | >3 (buena adopción) |
| Agent runs per week | >10/dev, >5/non-dev |
| Time to first agent run | <5 minutes desde signup |
| NPS (Enterprise) | >40 |

### 10.3 Lagging Indicators

| Metric | Target |
|---|---|
| Monthly ARR growth | >15% MoM (year 1), >10% (year 2) |
| Net Revenue Retention (NRR) | >120% |
| Gross Margin | >70% |
| Customer Acquisition Cost | <$15k (Enterprise), <$100 (PLG) |
| LTV/CAC | >10x Enterprise, >50x PLG |
| Churn | <5% Enterprise, <15% Team |

---

*Fin de BUSINESS_MODEL.md*
