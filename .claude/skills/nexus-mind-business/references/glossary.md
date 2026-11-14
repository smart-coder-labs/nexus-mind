# Glossary

## Product terms → code

| Term | In code |
|---|---|
| memory type | free text; protocol list in `CLAUDE.md`: architecture, bugfix, decision, discovery, pattern, config, preference, project, session_summary, feature, refactoring, feedback, manual |
| topic_key | org-wide upsert key for an evolving memory |
| scope | `org`, `client`, `project`, `personal` |
| SDD phase | `SddPhase`: explore, propose, spec, design, tasks, apply, verify, archive |
| SDD artifact kind | `SddArtifactKind`: exploration, proposal, spec, design, tasks, apply-progress, verify-report, archive-report, state |
| harness | versioned bundle of AI tooling with install approvals and config reviews (`api/harnesses.rs`) |
| convention | org/client rule for agents |
| session | an AI tool working session (`/v1/sessions`); **not** the web-session cookie key |
| agent | machine identity with keys (`/agents`) |
| autonomous agent template | qa, github_issue_resolver, github_pr_reviewer, lead_generation, judge, ai_content_manager, security_scan, security_dast |
| run status | queued, leased, running, succeeded, partial, failed, cancelled, blocked_policy, blocked_runtime, budget_exhausted, dead_letter |
| trigger | manual, schedule, github_webhook, reconcile |
| executor | `claude`, `codex`, `pi`, `nexus` (`automation/agent_runtime.rs`) |
| Pi executor | `pi` with ChatGPT OAuth (`docs/factory/PI_EXECUTOR.md`) |
| OpenShell | the `nexus` executor: Claude Code inside an external sandbox gateway (`docs/AUTONOMOUS_NEXUS_OPENSHELL.md`) |
| intake | external work items (Linear, Slack, Sentry, Gmail, transcripts) → task specs |
| finding | `autonomous_agent_findings` |
| evaluation | `factory_eval_runs` |
| shadow router / Jev | decision model run in shadow; verdict recorded, never acted on (`factory/jev.rs`) |
| workflow source | repos/branches fetched before a run (`api/workflow_sources.rs`) |
| runner, workspace | registered machine (token) and its repos/folders |
| gateway | model tier router: cheap, standard, frontier, with a daily frontier cap |
| lookback | recommendations linking repeated tasks to bugfix memories |
| soak | `factory_merge_soaks` |
| u2s | self-hosted single-org customer box (`deploy/u2s/`) |
| `only-context` / `only_context` | admin build profile (hyphen) / MCP tool profile (underscore) |
| superadmin vs `super_user` | env `SUPERUSER_KEY` holder in the backoffice vs an org role |

## Spanish (admin Factory UI, `docs/factory/`) → English → code

| Español | English | Code / route |
|---|---|---|
| Resumen | overview | `/factory` |
| Agentes | agents | `/factory/agents` |
| Entradas | intake | `/factory/intake` |
| Ejecuciones | runs | `/factory/runs` |
| Hallazgos | findings | `/factory/findings` |
| Pendientes | inbox / human digest | `/factory/inbox` |
| Laboratorio | evaluations + decision model | `/factory/laboratory` |
| Configuración | settings | `/factory/settings` |
| Plantillas | templates | `/factory/templates` |
| Origen | trigger origin | `trigger_origin` |
| Cola | queue | queued runs |
| Aprobación | approval | workflow approvals |
| Publicación | publication | `workflow_publication` |
| Entorno | environment | `docs/factory/AUTO_ENVIRONMENT.md` |
| Fuente | source | workflow source |
| Aislamiento | isolation | `pages/factory/isolation.ts`, sandbox |
| Reconciliación (sondeo) | reconcile polling | trigger `reconcile` |
| Temporizador | timer | trigger `schedule` |
| Superusuario | super user | role `super_user` |
