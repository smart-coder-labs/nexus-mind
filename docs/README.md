# NexusMind — Documentation

Technical documentation covering product, architecture, API, and engineering process.

## Index

| # | Document | Description |
|---|----------|-------------|
| 1 | [ARCHITECTURE.md](./ARCHITECTURE.md) | System architecture, data flow, component diagrams, deployment |
| 2 | [API_SPEC.md](./API_SPEC.md) | Full API specification with endpoints, schemas, auth |
| 3 | [AUTH_SPEC.md](./AUTH_SPEC.md) | Authentication, authorization (roles 100% custom), memory isolation, SSO, RBAC, ABAC |
| 4 | [ENGINEERING_PROCESS.md](./ENGINEERING_PROCESS.md) | Engineering workflow, CI/CD, code review, SDLC |
| 5 | [RUNNING.md](./RUNNING.md) | Full local + Docker setup guide |
| 6 | [CLAUDE_CODE_PLUGIN.md](./CLAUDE_CODE_PLUGIN.md) | Claude Code plugin integration |
| 7 | [CURSOR_PLUGIN.md](./CURSOR_PLUGIN.md) | Cursor integration |
| 8 | [CODEX_PLUGIN.md](./CODEX_PLUGIN.md) | Codex integration |
| 9 | [REPOSITORY_CONFIG.md](./REPOSITORY_CONFIG.md) | `.nexusmind.yaml` reference: project routing, agent profiles, resolution rules, examples |
| 10 | [SENTRY.md](./SENTRY.md) | Sentry integration and deployment variables |

## Factory

- [Workspaces and durable workflows](./factory/WORKSPACES_WORKFLOWS.md) — runner setup, local folders, manual environment references, Linear intake, implemented preparation flow and explicit delivery gaps.
- [Mandatory issue-resolver verification](./factory/ISSUE_RESOLVER_VERIFICATION.md) — final-SHA tests/build gates, CI requirements and safe checkpoints.
- [GitHub requested-review webhooks](./factory/GITHUB_REVIEW_WEBHOOKS.md) — backend `gh` identity, signed subscriptions and setup.
- [Factory plan](./factory/PLAN.md) — existing execution, verification and autonomy policies.

## Architecture Decision Records

→ [docs/adr/](./adr/)

| # | Título | Estado |
|---|--------|--------|
| [ADR-001](./adr/ADR-001.md) | Lenguaje de Implementación del Core de Memoria | Aceptado |
| [ADR-002](./adr/ADR-002.md) | Gestor de Base de Datos para el Core de Memoria | Aceptado (v2) |

## Quick Links

- [Landing Page](../apps/landing/)
