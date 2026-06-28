# Vendor and Sub-processor Risk Management for AI Memory Systems

> Research track: data governance round 1 — round 1, 2026-06-27.
> Frameworks cited: GDPR Art. 28/30, ISO/IEC 27001:2022 A.5.19–A.5.23, ISO/IEC 27018, NIST SP 800-161r1, SOC 2 CC9, EU AI Act Art. 25/53, DAMA DMBOK3 Ch. 10/11.

## 1. The problem in plain language

When an engineering team adopts an AI memory platform, the data they store does not stay between them and the platform. Every prompt, every retrieved memory, every prompt-log entry, every embedded vector, and every audit row flows through a chain of third parties: the LLM provider, the embedding model, hosting infrastructure, the observability vendor, the support team, and the engineering team that operates the platform. Under data-protection law, each link in that chain is a **sub-processor** of the customer, and the platform is the **processor** of personal data on the customer's behalf.

A mature governance program answers four questions about that chain:

1. **Who** is in it (registry)?
2. **What** data they can see, and under which legal terms (flow-down contract)?
3. **How** customers are told when it changes (change notification)?
4. **What** happens when a sub-processor is breached (incident correlation and forensic trail)?

The AI memory category has made these questions harder, not easier, because the sub-processor graph is now an *executable* graph: a single AI action can hit a vector store, a redaction policy, an LLM, a content-safety classifier, a tokenization service, and a logging endpoint, often in milliseconds. Auditors have noticed. The 2025 European Data Protection Board's enforcement sweep of EU-US AI tool providers cited sub-processor disclosure failures in 38% of cases reviewed.

## 2. What the frameworks say

**GDPR Art. 28(2)–(4)** requires the processor (NexusMind) to obtain **prior specific or general written authorization** from the controller (the customer) before engaging any sub-processor, to flow down the same data-protection obligations by contract, and to remain fully liable for the sub-processor's acts. Art. 30 demands the controller keep a **record of processing activities** naming the categories of recipients, including sub-processors. Forgetting either clause is a finding on its own.

**ISO/IEC 27001:2022 Annex A** elevates this from "legal" to "certifiable." A.5.19 (supplier relationships), A.5.20 (supplier agreements), A.5.21 (ICT supply-chain security), and A.5.23 (cloud services) require a documented supplier lifecycle: due diligence, contractual security clauses, ongoing monitoring, and exit. The 2022 update explicitly added "ICT supply chain" as a control, reflecting years of attacks (SolarWinds, Log4j, 3CX) that landed through third parties.

**NIST SP 800-161r1** prescribes a **C-SCRM** (Cybersecurity Supply Chain Risk Management) program with component provenance — an SBOM-equivalent for AI models. For AI, that means model provenance: which weights, which training-data cutoff, which fine-tunes are running on a customer's prompts.

**ISO/IEC 27018** layers PII-specific rules on top of ISO 27001 for any cloud service acting as a PII processor. It is the de-facto expectation for EU enterprise SaaS that processes personal data.

**SOC 2 CC9** (Risk Mitigation) requires the service organization to identify, select, and monitor vendor and business-partner risks. Auditors will ask for a vendor inventory, a tiering rubric, and evidence of review.

**EU AI Act Art. 25** (provider obligations) and **Art. 53** (GPAI provider obligations) impose transparency duties that flow *upward* through the chain: providers must document downstream sub-processors, and GPAI providers must keep technical documentation for 10 years. A memory platform that brokers LLM calls inherits part of that duty.

**DAMA DMBOK3 Chapter 10** (Data Security) names supplier and partner risk management as a first-class concern; Chapter 11 (Integration & Interoperability) argues that integration contracts must encode data-handling terms, not just API contracts.

## 3. What this looks like in the real world

**Example A — Financial services pre-sales (anonymized).** A mid-market lender evaluating an AI memory platform in Q1 2026 was six weeks from procurement. Their CISO's team asked for: (1) the list of every sub-processor the platform routes data through, by region; (2) signed flow-down terms that mirror the customer's own data-processing agreement; (3) 30-day prior notice for any sub-processor change; (4) a sub-processor exit plan; and (5) quarterly SOC 2 Type II reports for every sub-processor that handles PII. The platform had to answer "no" to four of the five; the deal slipped a quarter and required custom contracting.

**Example B — Healthcare pilot (anonymized).** A regional hospital network trying to use an AI coding assistant to document encounter notes learned, three weeks into a pilot, that the LLM sub-processor retained prompts for 30 days for abuse monitoring, contradicting their BAA. The hospital terminated the pilot and required that any future tool expose a sub-processor telemetry panel showing, per request, which sub-processors handled each prompt, with per-sub-processor data-handling configuration.

Both examples repeat a pattern: the platform becomes a liability precisely where it should be a strength. The customer cannot answer regulator questions about sub-processor behavior because the platform does not surface the answer — and procurement slows.

## 4. Why this is a gap for NexusMind

NexusMind is **architecturally positioned** for sub-processor governance. The Policy Gateway already inspects every request before it leaves the control plane (PRD §3.2, ARCHITECTURE §2.1); the audit log already records `model` per interaction (API_SPEC §4.3); and the project's roadmap calls out BYOM (bring-your-own-model) as a first-class capability. The product is a broker in front of LLM providers, embedding providers, and the customer's own infrastructure — the right place to enforce sub-processor policy.

But the current specs and feature set do not close the loop:

- **No sub-processor registry.** Neither `docs/PRD.md` nor `docs/API_SPEC.md` describes a first-class `Sub-processor` resource. The closest artifact is a passing "sub-processor schedule" in the DPA-management feature proposal in `researchfeatures.md`, which is reactive (a contract artifact), not operational (a runtime artifact).
- **No flow-down control.** A customer can write a policy that blocks `gpt-4` for a given project (AUTH_SPEC §4 example 1) but cannot assert a flow-down obligation (e.g., "OpenAI must not retain this prompt for >24h") because the contract and the policy are two different systems with no link.
- **No sub-processor change notification.** ARCHITECTURE §6 lists audit integrity and redaction but no supply-chain event. The customer has no API to subscribe to and no admin-console banner when the platform adds a new LLM provider.
- **No per-request sub-processor attribution in the API contract.** `GET /v1/audit` returns the model that processed a request, but not the data-residency region, retention regime, or contract terms of that sub-processor. Compliance teams join the audit row to an external spreadsheet.
- **No customer-controlled allow-list / deny-list of sub-processors.** A customer's policy can match a model name but cannot say "this project is forbidden from using any US-resident sub-processor" because the policy engine does not model sub-processor attributes.
- **No model provenance record.** EU AI Act Art. 53 expects GPAI providers to disclose training-data summaries and version history. NexusMind can record `model_version` in the audit row but does not surface a structured "model provenance" record that downstream customers can show their auditors.
- **No sub-processor exit plan tooling.** The on-prem Enterprise plan offers the strongest control, but there is no documented exit-package generator (data export, key destruction, attestation). ISO 27001 A.5.23 and DORA both expect one.

The business-model implications are non-trivial. The BUSINESS_MODEL.md Enterprise tier promises SOC 2 and "data residency" but does not promise sub-processor governance, even though it is the next question every EU enterprise CISO asks.

## 5. What this implies for NexusMind

The platform has a defensible opportunity to make sub-processor governance a **productized capability**, not a custom-contract deliverable. Concrete capabilities that fall out of the gaps above:

1. A **sub-processor registry** as a first-class resource (`/v1/subprocessors`) with per-sub-processor metadata: legal name, processing role, data categories, region, retention regime, certifications, contract reference, status (active, change-pending, sunset).
2. A **policy-engine extension** that lets Rego rules match on sub-processor attributes (region, certification, retention) — not only tool and model names — so customers can write "EU-only" or "SOC2-only" as policy, not as a spreadsheet.
3. A **change-notification workflow**: 30-day prior notice for additions, immediate notice for security incidents, with `/v1/subscriptions` webhooks and an admin-console banner.
4. **Per-request sub-processor attribution in audit events**: the audit row joins to the sub-processor record, so compliance teams can answer "what sub-processors saw data from project X last quarter?" in one query.
5. A **model-provenance record** for every LLM call, compatible with EU AI Act Art. 53: provider, model name, version, training-data cutoff, fine-tune lineage, served region.
6. A **sub-processor exit attestation generator** that produces a signed evidence bundle suitable for ISO 27001 A.5.23 and DORA exit-strategy reviews.
7. A **customer-facing sub-processor page** (public, per tenant) that lists the customer's data flows in plain language for non-technical stakeholders — the same audience the PRD's Compliance persona targets.

These turn a compliance liability into a sales asset: NexusMind becomes the platform that *answers* the CISO's five questions, rather than deflecting them. They align with the existing roadmap (Phase 2 lists SOC 2 + GDPR docs; Phase 3 lists HIPAA) and are preconditions for the EU AI Act and ISO 27001 certifications on the long-term plan (Phase 4).
