# NexusMind — Roadmap

> **Documento**: ROADMAP.md  
> **Versión**: 1.0  
> **Fecha**: Mayo 2026  
> **Propósito**: Timeline de desarrollo, milestones, y entregables quarter por quarter.

---

## 1. Timeline Visual

```
2025         2026                                   2027
Q3    Q4    Q1         Q2         Q3         Q4         Q1         Q2
│     │     │          │          │          │          │          │
│     │     ├──────────┤          │          │          │          │
│     │     │  F0      │          │          │          │          │
│     │     │  Research│          │          │          │          │
│     │     │  & Valid │          │          │          │          │
│     │     ├──────────┼──────────┤          │          │          │
│     │     │           │  F1     │          │          │          │
│     │     │           │  Core   │          │          │          │
│     │     │           │  Found. │          │          │          │
│     │     │           ├──────────┼──────────┤          │          │
│     │     │           │          │  F2      │          │          │
│     │     │           │          │  Memory  │          │          │
│     │     │           │          │  System  │          │          │
│     │     │           │          ├──────────┼──────────┤          │
│     │     │           │          │          │  F3      │          │
│     │     │           │          │          │  Agent   │          │
│     │     │           │          │          │  Runtime │          │
│     │     │           │          │          ├──────────┼──────────┤
│     │     │           │          │          │          │  F4      │
│     │     │           │          │          │          │  Orchest │
│     │     │           │          │          │          │  -ration │
│     │     │           │          │          │          ├──────────┤
│     │     │           │          │          │          │          │  F5
│     │     │           │          │          │          │          │  Enterp.
│     │     │           │          │          │          │          │
├─────┴─────┴─────┬─────┴─────┬───┴─────┬─────┴─────┬───┴─────┬─────┤
│                 │           │         │           │         │     │
│   Pre-seed      │  Seed     │         │           │ Series A│     │
│   $500k         │  $3-5M    │         │           │ $10-15M │     │
│                 │           │         │           │         │     │
│   MVP           │  Beta     │         │ Public    │         │     │
│   Prototype     │  Closed   │         │ Launch    │         │     │
└─────────────────┴───────────┴─────────┴───────────┴─────────┴─────┘
```

---

## 2. Phase Detail: Q1 2026 (Research & Validation)

**Goal**: Validar oportunidad de mercado y asegurar seed funding.

### Milestones

| Milestone | Date | Status |
|---|---|---|
| Market research complete | Week 2 | ✅ |
| 20 customer interviews | Week 4 | ✅ |
| PRD v1 complete | Week 4 | ✅ |
| Architecture v1 complete | Week 5 | ✅ |
| Engineering process documented | Week 5 | ✅ |
| Business model + pricing defined | Week 6 | ✅ |
| Seed pitch deck ready | Week 8 | 🔄 |

### Key Deliverables
```
┌──────────────────────────────────────────────┐
│               Q1 2026 OUTPUTS                 │
├──────────────────────────────────────────────┤
│  1. MARKET_RESEARCH.md   ← This document     │
│  2. PRD.md               ← You are here      │
│  3. ARCHITECTURE.md      ← System design     │
│  4. API_SPEC.md          ← API reference     │
│  5. ENGINEERING_PROCESS.md ← Build plan      │
│  6. BUSINESS_MODEL.md    ← Pricing + GTM     │
│  7. COMPETITIVE_MATRIX.md ← Competitors      │
│  8. RISK_ANALYSIS.md     ← Risk assessment   │
│  9. ROADMAP.md           ← This document     │
└──────────────────────────────────────────────┘
```

### External Events
- Pre-seed raise ($500k from angels/VCs)
- Hire first 3 engineers (Go backend, React frontend, ML/AI)
- Setup legal entity (France — SAS)
- Open-source Engram-inspired core as community builder

---

## 3. Phase Detail: Q2 2026 (Core Foundation)

**Goal**: Single binary funcional con CLI, HTTP API, MCP server, SQLite.

### Milestones

| Milestone | Date | Dependencies |
|---|---|---|
| Project scaffolding + directory structure | Week 10 | Engineers hired |
| CLI skeleton (cobra commands) | Week 11 | — |
| HTTP server with Chi router + middleware | Week 12 | — |
| SQLite schema + migrations | Week 13 | DB design from ARCHITECTURE.md |
| User auth (JWT + API keys) | Week 14 | Schema ready |
| MCP server with base tools | Week 15 | — |
| React frontend scaffold | Week 16 | — |
| Login/signup flow (frontend) | Week 17 | Auth ready |
| Core integration tests | Week 18 | All above |
| **MVP Demo: `nexusmind start` → server running** | **Week 18** | **—** |

### Team Needed
- 1 Go backend engineer
- 1 React frontend engineer
- 1 ML/AI engineer (part-time, preparing for memory phase)

### Key Risks
- Hiring delays → compress scope (drop some CLI features)
- Auth complexity → use Auth0 as middleware initially (replace later)

---

## 4. Phase Detail: Q3 2026 (Memory System)

**Goal**: Sistema de memoria persistente con hybrid search.

### Milestones

| Milestone | Date | Dependencies |
|---|---|---|
| Episodic memory CRUD + FTS5 | Week 19 | SQLite schema |
| Full-text search with BM25 ranking | Week 21 | FTS5 index |
| Embedding service (all-MiniLM-L6-v2) | Week 22 | ML engineer |
| Vector search with pgvector | Week 23 | PostgreSQL migration |
| Hybrid search (FTS + vector weighted) | Week 24 | Both search types |
| Auto-summarization pipeline | Week 26 | Context window mgr |
| Context window manager (sliding/relevance) | Week 27 | Summarizer |
| Memory consolidation (auto-summarize sessions) | Week 28 | Summarizer |
| Memory UI (search, timeline, stats) | Week 30 | Frontend |
| **Memory System Alpha** | **Week 30** | **—** |

### Technical Decisions
- SQLite FTS5 for local/on-prem deployments
- pgvector for cloud deployments
- Embeddings computed async (queue-based)
- Summarizer runs on session end (non-blocking)

### Key Risks
- Embedding model performance → benchmark before committing
- Memory delta size → implement sliding window from day 1
- User data sensitivity → encryption + RBAC on memory entries

---

## 5. Phase Detail: Q4 2026 (Agent Runtime)

**Goal**: Agentes multi-modelo con sandbox y tool execution.

### Milestones

| Milestone | Date | Dependencies |
|---|---|---|
| Model gateway (OpenAI, Anthropic, Google) | Week 31 | — |
| Smart routing (cost, latency, fallback) | Week 32 | Multi-model support |
| LLM cache (LRU + semantic dedup) | Week 33 | Cache layer |
| Sandbox container (gVisor) | Week 35 | DevOps infra |
| Tool definitions (JSON Schema) | Week 36 | Sandbox ready |
| Tool executor (Python, JS, Go runtimes) | Week 38 | Tool definitions |
| Chat interface with SSE streaming | Week 39 | Frontend |
| Agent selector + model selector UI | Week 40 | Chat interface |
| Context panel (memory visible, editable) | Week 42 | — |
| Session management UI | Week 44 | Agent runtime |
| **Agent Runtime Beta (closed)** | **Week 44** | **—** |

### Team Expansion
- +1 ML/AI engineer (model gateways, cache, routing)
- +1 DevOps engineer (sandbox, containerization, CI/CD)

### Key Risks
- Model provider API changes → abstracted behind gateway
- Sandbox security → gVisor + capability drop + timeouts
- Streaming latency → <500ms first token target

---

## 6. Phase Detail: Q1 2027 (Orchestration)

**Goal**: Orquestación de sub-agentes con DAG workflows.

### Milestones

| Milestone | Date | Dependencies |
|---|---|---|
| Agent CRUD + spec definitions | Week 45 | Agent runtime |
| Agent lifecycle state machine | Week 46 | Agent CRUD |
| Health checks + auto-recovery | Week 47 | State machine |
| DAG definition language (YAML) | Week 48 | — |
| DAG executor with parallel nodes | Week 49 | DAG parser |
| Error handling strategies (fail, skip, retry) | Week 50 | DAG executor |
| Sub-agent handoff protocol | Week 51 | — |
| Result aggregation + conflict resolution | Week 52 | Handoff |
| Task scheduler (cron-like) | Week 53 | Workflow engine |
| DAG visualizer (frontend) | Week 56 | — |
| **Public Launch v1.0** | **Week 56** | **Everything above** |

### Key Differentiator
DAG workflows con sub-agents jerárquicos + memoria compartida es la feature que ningún competidor tiene.

---

## 7. Phase Detail: Q2 2027 (Enterprise + GTM)

**Goal**: Características enterprise completas y lanzamiento comercial.

### Milestones

| Milestone | Date | Dependencies |
|---|---|---|
| Admin dashboard with KPIs | Week 57 | — |
| User management + invite flow | Week 58 | Auth |
| Team management | Week 59 | User management |
| API Keys management | Week 60 | — |
| RBAC with custom roles | Week 62 | — |
| Policy engine (Casbin) | Week 63 | RBAC |
| SSO (SAML 2.0, OIDC) | Week 64 | — |
| SCIM provisioning | Week 65 | SSO |
| Immutable audit trails (HMAC chain) | Week 66 | — |
| Billing system (Stripe) | Week 67 | — |
| Quota management | Week 68 | Billing |
| Non-developer agent templates (Support, Data, Ops) | Week 72 | Agent runtime |
| Agent marketplace | Week 74 | Templates |
| Custom Agent Builder (drag & drop) | Week 76 | Marketplace |
| SOC2 Type I audit | Week 78 | All enterprise features |
| Beta program (20 companies) | Week 80 | SOC2 |
| **v1.0 General Availability** | **Week 88** | **Beta feedback** |

### Sales Targets
- 20 beta customers by Week 80
- 500 paid seats by Week 88
- $2.5M ARR by end of Q2 2027

---

## 8. Phase Timeline: Q3 2027+ (Scale)

**Goal**: Escalar a $10M+ ARR y preparar Series B.

| Quarter | Focus | Target Metric |
|---|---|---|
| Q3 2027 | Enterprise sales push | 100 customers, $5M ARR |
| Q4 2027 | International expansion (UK, Germany) | $8M ARR |
| Q1 2028 | Partner channel (SI partners) | $12M ARR |
| Q2 2028 | APAC expansion (Japan, Singapore) | $20M ARR |
| Q3 2028 | Enterprise features v2 (advanced compliance) | $30M ARR |
| Q4 2028 | AI Agent Marketplace launch | $50M ARR |

---

## 9. Dependency Graph

```
F0: Market Research
  └── F1: Core Foundation
        └── F2: Memory System
              └── F3: Agent Runtime
                    └── F4: Orchestration
                          └── F5: Enterprise Layer
                                └── F6: Non-dev Agents + Marketplace
                                      └── F7: Beta + SOC2 + Launch
```

**Parallel work allowed**:
- F1 Frontend (login, scaffold) puede empezar mientras F1 Backend termina
- F5 Audit trails puede empezar cuando F1 Auth esté listo (no necesita esperar F4)
- F6 Agent templates puede empezar cuando F3 Agent Runtime esté estable

---

## 10. Team Growth

```
Phase       │ Total   │ Backend │ Frontend │ ML/AI   │ DevOps │ S&M    │ PM
────────────┼─────────┼─────────┼──────────┼─────────┼────────┼────────┼────
Pre-seed    │ 3       │ 1       │ 1        │ 0.5     │ 0.5    │ 0      │ 0
Seed (Q1)   │ 6       │ 2       │ 1        │ 1       │ 1      │ 0.5    │ 0.5
Core (Q2)   │ 8       │ 2       │ 2        │ 1       │ 1      │ 1      │ 1
Memory (Q3) │ 10      │ 2       │ 2        │ 2       │ 1      │ 1.5    │ 1
Agent (Q4)  │ 14      │ 3       │ 2        │ 3       │ 1      │ 3      │ 1
Orch (Q1)   │ 18      │ 3       │ 3        │ 3       │ 2      │ 5      │ 1
Enterp (Q2) │ 25+     │ 4       │ 3        │ 3       │ 2      │ 10     │ 2
│
│ Legend:
│   F/T: Full-time / Part-time
│   S&M: Sales & Marketing
│   PM: Product Management
```

---

## Appendix A: Risk-Adjusted Timeline

Las fechas asumen un equipo completo y sin interrupciones. Ajuste realista:

| Risk | Impact on Timeline | Buffer Applied |
|---|---|---|
| Hiring delays (+2 months) | +2 months to all milestones | Milestones padded 2 weeks each |
| Technical debt from speed | +1 month by F4 | Code review buffer |
| Pivot from customer feedback | +2 months | (not in plan — contingency) |
| Competitor surprise launch | +0 (reprioritize) | — |
| Total buffer | ~4 months | GA: Q2 2027 → Q3-Q4 2027 realistic |

---

## Appendix B: Quick Reference

```
Key Dates:
  Q1 2026 → Research & Validation (we are here)
  Q2 2026 → Core Foundation (MVP: single binary)
  Q3 2026 → Memory System (hybrid search alpha)
  Q4 2026 → Agent Runtime (closed beta)
  Q1 2027 → Orchestration (public launch v1.0)
  Q2 2027 → Enterprise + GTM (GA + SOC2)
  Q3 2027 → Scale (Series B target)

Funding:
  Pre-seed ($500k) → Seed ($3-5M) → Series A ($10-15M) → Series B ($30-50M)

Team:
  3 → 6 → 10 → 14 → 18 → 25+ (over 2 years)
```

---

*Fin de ROADMAP.md*
