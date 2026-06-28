# NexusMind — Feature Proposals from Data Governance Research

## Executive Summary

NexusMind has the right foundational primitives for an enterprise-grade AI memory platform: multi-tenant isolation, RBAC, audit trails, and project scoping. However, from a data governance perspective, three critical gaps stand out. First, memories are stored without classification or retention schedules — a direct GDPR Article 5 violation for any EU customer storing personal data. Second, the audit trail, while present, lacks the immutability, structural richness, and export capability required for SOC 2 Type II or EU AI Act compliance. Third, there is no lifecycle management layer — no TTL, no erasure workflow, no archival tier — making NexusMind legally unusable for regulated enterprises without significant custom scaffolding on their side.

Closing these gaps would move NexusMind from a capable developer tool to a genuinely enterprise-ready governed memory platform.

---

## New Features to Add

### 1. Data Classification System

**Priority**: High
**Governance driver**: GDPR Article 5 (storage limitation), data classification principle, need-to-know
**Problem**: All memories are currently treated equally regardless of their sensitivity. A memory containing a team convention ("use camelCase") and a memory containing a customer API key, a developer's personal email, or regulated system architecture are stored and retrieved under the same access policy. This makes it impossible to apply differential retention, access control, or audit requirements.
**Proposed solution**: Introduce a four-tier classification label on every memory object: `public`, `internal`, `confidential`, `restricted`. Classification can be set explicitly by the user at store time, or assigned automatically by a scanning heuristic at ingestion (PII detection, secret pattern matching, keyword rules). Classification is inherited: a memory derived from a `restricted` memory is `restricted`. Access policies and retention schedules are keyed to classification level.
**Effort estimate**: Medium

---

### 2. Memory Retention Policies and Lifecycle Management

**Priority**: High
**Governance driver**: GDPR Article 5(1)(e) (storage limitation), CCPA retention disclosure requirement, SOX 7-year minimum, HIPAA 6-year minimum
**Problem**: Memories have no expiry. They accumulate indefinitely. This is a direct regulatory risk for EU customers (personal data held longer than necessary violates GDPR) and a missing capability for regulated enterprises that need configurable retention schedules per data type.
**Proposed solution**: Add a retention policy system with three levels of configuration:
  1. **Organization-level defaults**: configured by org admin (e.g., "internal memories expire after 180 days, confidential after 90 days")
  2. **Project-level overrides**: per-project retention that overrides org defaults
  3. **Memory-level TTL**: individual memories can be given an explicit expiry at store time via the MCP API

Lifecycle engine runs as a background job: active → marked-for-deletion → soft-deleted → purged from storage. Each transition is logged in the audit trail. Legal hold (see below) suspends the purge step.

**Effort estimate**: Large

---

### 3. Right-to-Erasure Workflow (GDPR Article 17)

**Priority**: High
**Governance driver**: GDPR Article 17 (right to erasure), CCPA right to delete
**Problem**: There is no formal workflow for a data subject to request erasure of all their data, or for an administrator to execute such a request. Without this, any enterprise customer who uses NexusMind to process personal data of EU residents is non-compliant.
**Proposed solution**: A structured erasure workflow accessible from the admin panel:
  1. Admin submits an erasure request with: subject identifier (user ID, email), scope (full account, specific project, date range), and legal basis
  2. System identifies all memories, sessions, and audit log entries associated with the subject
  3. For memories: hard delete the memory content; create a tombstone record (records that a memory existed and was deleted, not what it contained)
  4. For audit logs: the fact of the interaction is retained (timestamp, action type, outcome) per EU AI Act Article 12 requirements; the content fields are nulled
  5. System generates an erasure completion certificate with: request timestamp, completion timestamp, count of records deleted, scope covered
  6. Erasure requests are themselves logged in a separate erasure audit trail

Completion SLA: 30 days (GDPR maximum). The system tracks time-to-fulfill and alerts admins approaching the deadline.

**Effort estimate**: Large

---

### 4. Immutable Audit Log with Structured Export

**Priority**: High
**Governance driver**: SOC 2 CC7 (monitoring), EU AI Act Article 12 (logging), ISO 27001 A.8.15, GDPR accountability principle
**Problem**: NexusMind has an audit trail, but it likely writes to the same SQLite database as operational data. This means: (1) it is not immutable — a database compromise or admin error can alter historical records; (2) it lacks the structural richness required by SOC 2 auditors (missing fields: IP address, API key ID, data classification of resource acted upon, client tool/version); (3) there is no export capability for SIEM integration or compliance reporting.
**Proposed solution**:
  - Separate the audit log into an append-only log store (separate SQLite file or external service) with no update/delete operations exposed at any layer
  - Add cryptographic chaining: each log entry includes a hash of the previous entry; integrity can be verified by replaying the chain
  - Enrich log entries with missing fields: IP address, API key ID (masked), client tool identifier, data classification of the accessed resource, session ID
  - Add a structured export API: filter by date range, tenant, project, event type; export as newline-delimited JSON or CSV
  - Add a log integrity verification endpoint: admins can trigger a chain verification run; results are logged and surfaced in the admin panel

**Effort estimate**: Large

---

### 5. PII and Secret Detection at Ingestion

**Priority**: High
**Governance driver**: GDPR Article 25 (data protection by design), data minimization principle, security
**Problem**: Developers routinely paste sensitive content into AI tools without realizing it will be persisted. A memory store that accepts all content unconditionally will accumulate PII (names, emails, phone numbers), credentials (API keys, passwords, tokens), and regulated data (financial account numbers, health information) that it was never intended to hold.
**Proposed solution**: A configurable scanning layer that runs at memory ingestion before any content is persisted:
  - **PII detection**: regex + ML classifiers for common PII patterns (email, phone, SSN, credit card, passport numbers)
  - **Secret detection**: pattern matching for common secret formats (AWS keys, GitHub tokens, private keys, connection strings with passwords)
  - **Policy actions**: `warn` (store with a flag), `redact` (replace detected patterns with tokens before storage), `block` (reject the store operation with a clear error to the AI tool)
  - **Configurable per org**: orgs can set the default policy per detection category and per classification level
  - **Admin review queue**: blocked or flagged memories appear in an admin review queue before being stored or discarded

**Effort estimate**: Large

---

### 6. Legal Hold

**Priority**: Medium
**Governance driver**: Legal hold requirements, SOX, litigation response
**Problem**: When a customer is involved in litigation or a regulatory investigation, they may receive a legal hold notice that prohibits deletion of specific data. NexusMind's retention lifecycle would otherwise automatically delete data during an active hold, creating spoliation risk (destruction of evidence).
**Proposed solution**: A legal hold mechanism that:
  - Admins can place a hold on: a specific memory, a project, a user's memories, or all memories in a date range
  - Holds block the lifecycle engine from advancing past the archival stage (no purge)
  - Holds are recorded in the audit trail with the admin who placed them and the stated reason
  - Holds have an optional expiry date; the system alerts when a hold expires
  - Hold status is surfaced in the memory browser in the admin panel

**Effort estimate**: Small

---

### 7. Data Residency Controls

**Priority**: Medium
**Governance driver**: GDPR Chapter V (international transfers), EU AI Act, DORA, data sovereignty requirements
**Problem**: Enterprise customers in regulated jurisdictions (EU, UK, Australia, Canada) increasingly require that their data never leaves a specific geographic region. NexusMind has no mechanism for regional data isolation.
**Proposed solution**:
  - Organization-level data residency setting: `global`, `eu`, `us`, `apac`
  - When set, the storage and processing for that org is routed to a region-specific deployment
  - API keys are region-scoped; cross-region requests are rejected
  - Admin panel surfaces the data residency setting prominently and prevents accidental region changes
  - Data export includes region provenance metadata for compliance documentation

This is an infrastructure-level feature requiring multi-region deployment. For early implementation, a simpler version is a flag that controls which region's infrastructure handles the org, even if NexusMind is deployed in a single region per deployment.

**Effort estimate**: Large

---

### 8. SOC 2 Compliance Dashboard

**Priority**: Medium
**Governance driver**: SOC 2 Type II, enterprise sales qualification
**Problem**: Enterprise customers and their security teams need ongoing evidence that NexusMind's controls are operating. Currently, they would need to manually extract audit log data, access review records, and incident records and compile them externally.
**Proposed solution**: A compliance dashboard in the admin panel that aggregates and visualizes:
  - **Access reviews**: list of all users with access, last review date, next review due
  - **MFA adoption rate**: percentage of human users with MFA enabled
  - **Retention policy coverage**: percentage of memories with an applicable retention schedule
  - **Audit log integrity**: last integrity check result and timestamp
  - **Erasure requests**: open requests, average time-to-fulfill
  - **Privileged operations**: count of admin operations in the last 30 days
  - **Exportable evidence packet**: one-click export of all compliance metrics for a specified period (for auditor submission)

**Effort estimate**: Medium

---

### 9. Memory Dependency Graph (Data Lineage)

**Priority**: Medium
**Governance driver**: Data lineage principle, EU AI Act Article 12 (traceability), GDPR accountability
**Problem**: When a user requests erasure of their memories, or when a memory is found to contain incorrect information, there is no way to identify which other memories were derived from or influenced by it. This makes complete erasure impossible and makes quality root-cause analysis manual.
**Proposed solution**: Track at store time which source memories (if any) contributed to the creation of a new memory. When the MCP server synthesizes a new memory from retrieved context, log the source memory IDs as `lineage_sources` on the new memory. The admin panel can visualize the dependency graph for any memory, showing upstream sources and downstream dependents. Deletion of a memory flags its dependents for review.

**Effort estimate**: Medium

---

### 10. GDPR Data Processing Agreement Management

**Priority**: Medium
**Governance driver**: GDPR Article 28 (processor obligations), enterprise compliance
**Problem**: NexusMind acts as a data processor for enterprise customers who are data controllers. GDPR requires a signed Data Processing Agreement (DPA) to exist before any personal data is processed. There is no current mechanism to manage, store, or evidence DPA status.
**Proposed solution**: A lightweight DPA management feature in the admin panel:
  - Standard DPA template with configurable sub-processor schedule
  - E-signature workflow (or manual upload for paper DPAs)
  - DPA status visible on the org settings page (signed / pending / expired)
  - Automated reminder 60 days before DPA expiry
  - DPA record stored and retrievable for audit

**Effort estimate**: Small

---

## Features to Improve / Modify

### Audit Trail (Existing)

**Current state**: Logs store, search, and delete events with user, tool, and timestamp.
**Gap**: Missing fields (IP address, API key ID, data classification of resource, session ID); no immutability guarantee (same SQLite database as operational data); no export capability; no integrity verification.
**Proposed change**: See "Immutable Audit Log with Structured Export" above. Additionally, add classification-level filtering to the admin audit log viewer so security teams can quickly isolate events involving Confidential or Restricted data.

---

### RBAC and Access Policies (Existing)

**Current state**: Role-based access control with access policies at the organization level.
**Gap**: No classification-gated access (any member can access any memory regardless of classification); no project-level Viewer role (read-only project access without write); no time-based or context-based access policies (ABAC extensions).
**Proposed change**: Extend RBAC with:
  - Project-level Viewer role (read memories, cannot store or delete)
  - Classification-gated operations: only Admins can access `restricted` memories; `confidential` memories require explicit project membership
  - Per-role access review requirement: Admin role requires quarterly access certification

---

### API Keys (Existing)

**Current state**: API keys authenticate AI tools and MCP integrations.
**Gap**: Keys appear to be organization-scoped with no expiry or rotation capability. A compromised key grants broad access indefinitely. No per-project or per-capability scoping.
**Proposed change**:
  - Add mandatory expiry on API keys (max 1 year, configurable to shorter)
  - Add project-scoping: a key can be restricted to specific projects
  - Add capability scoping: a key can be restricted to specific operations (read-only, store-only, no-delete)
  - Add a key rotation workflow: new key issued, old key deprecated with a grace period, then revoked
  - All key operations (create, rotate, revoke) logged in the audit trail

---

### Sessions (Existing)

**Current state**: Session context is tracked for AI tools.
**Gap**: Sessions may accumulate indefinitely without cleanup, creating unbounded personal data retention. Session data likely has no retention schedule separate from memory retention.
**Proposed change**: Add session lifecycle management:
  - Sessions have a configurable inactivity timeout (default: 24 hours)
  - Session data has its own retention schedule (default: 30 days post-session-close)
  - Session closure triggers a cleanup check: does any session-only data (ephemeral context not promoted to a memory) need to be deleted?
  - Session events are included in the audit trail

---

### Collections (Existing)

**Current state**: Group memories into named collections.
**Gap**: Collections have no classification attribute. Adding a `restricted` memory to a `public` collection effectively downgrades the memory's protection. No classification inheritance policy.
**Proposed change**: Collections inherit the highest classification of any member memory. Admins can also explicitly set a collection's minimum classification floor. Access to the collection is governed by the collection's effective classification level.

---

### Admin Panel (Existing)

**Current state**: Browse memories, manage users, view audit logs.
**Gap**: No retention schedule management UI, no erasure request tracking, no classification filtering in the memory browser, no compliance dashboard.
**Proposed change**: Add to the admin panel:
  - Retention policy configuration (org-level defaults, project overrides)
  - Erasure request queue and workflow tracker
  - Classification filter in the memory browser
  - Compliance dashboard (see New Features above)
  - Legal hold management UI

---

## Features to Consider Removing or Deprecating

### Conventions Feature (Review Recommended)

**Current concern**: Team coding conventions stored in NexusMind are likely to contain sensitive architectural decisions, proprietary patterns, and business logic. If conventions are stored as a distinct, less-controlled object type (potentially with different access policies or retention behavior than regular memories), they could create a governance gap — sensitive data in a category that may not have the same lifecycle, classification, or audit controls as memories.
**Recommendation**: Do not deprecate conventions as a concept, but ensure they are stored as classified memories with the same governance controls. Any special-casing in the storage layer that gives conventions looser policies should be removed. Convention objects should be `confidential` by default.
**Alternative**: None needed — this is a consolidation, not a removal.

---

## Compliance Gaps

Based on the research, NexusMind currently has the following specific regulatory gaps that block enterprise adoption in regulated industries:

### GDPR (EU General Data Protection Regulation)
- **No retention schedules**: personal data stored in memories has no defined expiry — direct violation of Article 5(1)(e) storage limitation principle
- **No right-to-erasure workflow**: Article 17 compliance requires a formal, auditable deletion process with a 30-day fulfillment SLA
- **No data minimization at ingestion**: Article 25 (data protection by design) requires technical measures to minimize personal data collection — currently all content is accepted unconditionally
- **No DPA management**: Article 28 requires a signed DPA with each enterprise customer before processing their data
- **No DPIA tooling**: high-risk AI processing requires a Data Protection Impact Assessment; no support for documenting one

### SOC 2 Type II
- **Audit log immutability unverified**: no cryptographic chaining or WORM storage documented
- **Missing audit log fields**: SOC 2 auditors require IP address, API key, and resource classification in log entries
- **No access review workflow**: SOC 2 CC6 requires documented periodic access reviews with evidence
- **No export capability**: auditors need structured log exports for their own analysis

### EU AI Act
- **No minimum 6-month immutable log guarantee**: AI Act Article 12 requires tamper-resistant logs retained for at least 6 months; current implementation may not guarantee this
- **No human oversight controls**: high-risk classification would require admin ability to inspect and correct AI-stored memories — partially addressed by the admin panel but not formally documented as a control

### SOX (if financial customers are targeted)
- **No 7-year retention tier**: SOX requires 7-year retention for financial records; NexusMind has no long-retention archival tier

### HIPAA (if healthcare customers are targeted)
- **No PHI handling controls**: HIPAA requires specific safeguards for Protected Health Information; NexusMind has no PHI-specific classification, handling policy, or Business Associate Agreement (BAA) process

### General Enterprise Requirements
- **No data residency controls**: customers in EU, UK, Australia cannot guarantee their data stays within their jurisdiction
- **No legal hold mechanism**: organizations subject to litigation or regulatory investigation cannot prevent retention policy from auto-deleting data under hold
- **No SOC 2 compliance dashboard**: customers have no self-serve evidence of control effectiveness

---

## Round 1 (2026-06-27) — Vendor & Sub-processor Risk Management

This round's deep-dive lives at [`docs/research/data-governance-001-vendor-subprocessor-risk.md`](research/data-governance-001-vendor-subprocessor-risk.md). The headline finding: NexusMind is structurally a sub-processor broker — every prompt crosses LLM, embedding, hosting, and observability sub-processors — but the product has no first-class sub-processor object, no flow-down policy language, no change-notification workflow, no per-request sub-processor attribution in audit events, and no model provenance record. The prior DPA-management feature (#10) covers the *contract* artifact, but not the *operational* surface regulators actually inspect.

Frameworks grounding this round: **GDPR Art. 28(2)–(4) and Art. 30**, **ISO/IEC 27001:2022 Annex A.5.19–A.5.23**, **ISO/IEC 27018**, **NIST SP 800-161r1 (C-SCRM)**, **SOC 2 CC9**, **EU AI Act Art. 25 and Art. 53**, **DAMA DMBOK3 Ch. 10/11**. See deep-dive §2 for citations and §3 for two anonymized pre-sales / pilot examples.

### [round-1] R1.1 — Sub-processor Registry (First-class Resource)

**Problem**: NexusMind has no first-class `Sub-processor` resource. The audit log records `model` per request, but customers have no way to enumerate the parties handling their data, their regions, their retention regimes, or their certifications, and no machine-readable answer for an Art. 30 record-of-processing query.
**Solution sketch**: Introduce `GET/POST /v1/subprocessors` as a versioned resource. Each record carries `legal_name`, `processing_role` (LLM / embedding / hosting / observability / support), `data_categories` (text, embeddings, prompts-with-content, etc.), `region`, `retention_regime`, `certifications` (SOC2 Type II, ISO 27001, ISO 27018), `contract_reference`, `status` (`active` | `change-pending` | `sunset`), and `effective_from` / `effective_to`. The registry is fed by the platform (LLM providers in production use) and by tenant admins (their own sub-processors that NexusMind does not directly operate). Expose a `GET /v1/subprocessors?project=acme-payments` filter so a customer can answer "what handled my data last quarter?" without joining spreadsheets.
**Reference**: deep-dive §4 ("No sub-processor registry") and §5 item 1.
**effort: M** | **impact: L**

### [round-1] R1.2 — Sub-processor-aware Policy Language

**Problem**: The current Rego policy engine matches on tool and model names (AUTH_SPEC §4 example 1) but not on sub-processor attributes. A customer cannot write "this project is forbidden from any US-resident sub-processor" or "only SOC2-certified sub-processors may process sensitive data" without manual spreadsheet maintenance.
**Solution sketch**: Extend the policy schema so `match` blocks can reference sub-processor attributes resolved from the registry. Add new attribute keys: `subprocessor.region`, `subprocessor.certifications`, `subprocessor.retention_days`, `subprocessor.processing_role`. The Policy Gateway resolves sub-processors per request (via the registry + the LLM/embedding/hosting call graph) before evaluating rules. The same rule that today blocks `gpt-4` for a project can now block "any non-EU sub-processor" for the same project — without code changes.
**Reference**: deep-dive §4 ("No customer-controlled allow-list / deny-list of sub-processors") and §5 item 2.
**effort: L** | **impact: L**

### [round-1] R1.3 — Sub-processor Change-Notification Workflow

**Problem**: GDPR Art. 28(2) requires prior authorization before engaging a new sub-processor; ISO 27001 A.5.20 requires contractual change-control. Today, when the platform adds a new LLM provider, the customer has no API to subscribe to and no admin-console banner — they find out at audit time.
**Solution sketch**: Add `POST /v1/subscriptions` (webhooks + email) with event types `subprocessor.added`, `subprocessor.changed` (region, certification, retention), `subprocessor.deprecated`, `subprocessor.incident`. Default notice window: 30 days for additions, immediate for incidents. Admin console shows a persistent banner until acknowledged. Admins can set per-org default windows and per-processor overrides.
**Reference**: deep-dive §4 ("No sub-processor change notification") and §5 item 3.
**effort: S** | **impact: M**

### [round-1] R1.4 — Per-Request Sub-processor Attribution in Audit Events

**Problem**: `GET /v1/audit` returns the model that processed a request, but not the data-residency region, retention regime, or contract terms of that sub-processor. Compliance teams have to join the audit row to a vendor spreadsheet to answer "which sub-processors saw data from project X last quarter?" — an operation that fails closed under audit pressure.
**Solution sketch**: Enrich the audit-row schema with a `subprocessor` block joining to the registry: `{ subprocessor_id, region, retention_days, certifications_at_request_time, contract_version }`. The audit log carries a snapshot, not just a reference, so a sub-processor whose certification later expires still appears in historical reports as it was at the time. Adds a new query: `GET /v1/audit?subprocessor=openai-llm&from=...` for compliance reviews.
**Reference**: deep-dive §4 ("No per-request sub-processor attribution in the API contract") and §5 item 4. Complements feature #4 (immutable audit log) above.
**effort: M** | **impact: L**

### [round-1] R1.5 — Model Provenance Record (EU AI Act Art. 53)

**Problem**: The EU AI Act requires GPAI-adjacent providers to disclose training-data summaries and version history. NexusMind can record `model_version` in the audit row, but it does not surface a structured "model provenance" record that a customer's auditor can consume.
**Solution sketch**: Add a `Model` resource (`GET /v1/models/:id`) carrying `provider`, `model_name`, `version`, `training_data_cutoff`, `fine_tune_lineage`, `served_region`, `content_policy_url`, and `provider_dpa_url`. The audit row references the model record; the model record is the canonical artifact. Customers can render a per-project "models used" page for their own Art. 53 documentation. Self-hosted Enterprise customers can register their own models (including self-hosted open-source) and inherit the same attribution.
**Reference**: deep-dive §4 ("No model provenance record") and §5 item 5.
**effort: M** | **impact: M**

### [round-1] R1.6 — Sub-processor Exit Attestation Generator

**Problem**: ISO 27001 A.5.23 and DORA both expect a documented exit strategy. The on-prem Enterprise plan offers the strongest control, but there is no documented exit-package generator (data export, key destruction, attestation). Customers negotiating renewals repeatedly ask for one; today it is hand-rolled.
**Solution sketch**: Add `POST /v1/exit-package` that produces a signed ZIP/PDF bundle: (a) full data export of the tenant's memories, sessions, and audit events in portable formats (existing PRD commitment), (b) signed attestation that all sub-processor data has been deleted (per the registry), (c) a key-destruction record (or "no customer-held key" if not BYOK), (d) a timeline of when the tenant's data was last seen by each sub-processor. The bundle is hash-signed and stored in the audit trail itself, so the exit is itself auditable.
**Reference**: deep-dive §4 ("No sub-processor exit plan tooling") and §5 item 6.
**effort: M** | **impact: M**

### [round-1] R1.7 — Customer-facing Sub-processor Page (Compliance Audience)

**Problem**: The PRD identifies the Compliance / Security Officer persona, but the admin console is not designed for non-technical compliance readers. CISOs and DPOs need a plain-language, shareable view of "where does our data go?" without learning the Rego policy syntax.
**Solution sketch**: Add a public, per-tenant `/compliance/subprocessors` page (no auth required, signed URL with TTL) that renders the customer's sub-processor registry, data categories per sub-processor, regions, certifications, retention regimes, and the date of the last change notice. It is generated from the registry (R1.1) and the model record (R1.5) — no separate artifact. PDF export for inclusion in customer DPA packages.
**Reference**: deep-dive §5 item 7. Aligns with the PRD's Compliance persona at §2.4.
**effort: S** | **impact: M**

### Retirements / re-evaluations from this round

- **Feature #10 (DPA Management)** — **not retired**, but its "configurable sub-processor schedule" sub-bullet is now clearly an *input* to R1.1 (Sub-processor Registry) rather than the operational source of truth. Recommend feature #10 stay focused on the contract artifact (signatures, expiry, reminders) and that R1.1 owns the runtime registry. No code retired; ownership boundary clarified.
- **Feature #1 (Data Classification)** — no change. Classification is orthogonal to sub-processor attribution; both are needed.
- **Feature #7 (Data Residency)** — no change. R1.2 (sub-processor-aware policy) is a strict superset for *policy-level* residency enforcement, but #7's infrastructure-level routing still owns the data-plane guarantee.
