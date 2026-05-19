# NexusMind — MVP Architecture (v1 — Single User)

> ⚠️ **Este documento es del plan v1 (developer-focused).**
> Para la arquitectura multi-tenant del plan enterprise, ver [06-ENTERPRISE_MVP.md](./06-ENTERPRISE_MVP.md).

> **Documento**: 02-ARCHITECTURE_MVP.md
> **Versión**: 0.1.0 (v1 — archivado)
> **Propósito**: Arquitectura reducida para el MVP. Basada en ADR-001 (Rust) y ADR-002 (SQLite). **Documento de referencia — no refleja el plan actual.**

---

## Corrección: Este documento está obsoleto

El MVP v1 asumía un modelo single-user. El MVP v2 (enterprise) requiere:

- **Multi-tenant**: datos aislados por organización, no por usuario individual
- **Admin panel enterprise**: dashboard, user management, audit log
- **API keys por usuario**, scoped a su organización
- **Memoria compartida**: toda la org ve la misma memoria (con filtros)

Ver [06-ENTERPRISE_MVP.md](./06-ENTERPRISE_MVP.md) para la arquitectura actualizada.

---

*El resto de este documento se mantiene como referencia técnica de bajo nivel para el stack Rust/SQLite/Axum.*
