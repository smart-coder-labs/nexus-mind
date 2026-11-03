import type { RuntimeState } from './runtime'
export type Repository = { path: string; remote: string; head_sha: string; writable: boolean }
export type Runner = {
  id: string
  name: string
  revoked: boolean
  capabilities: string[]
  last_seen: string | null
}
export type Workspace = {
  id: string
  runner_id: string
  name: string
  root_path: string
  discovered: Repository[]
  repositories: Repository[]
  inventory_revision: number
  confirmed_revision: number | null
  runtime?: RuntimeState
}
export type WorkflowStep = {
  id: string
  name: string
  kind: 'prepare' | 'agent' | 'check' | 'gate' | 'publish' | 'review' | 'notify'
  depends_on: string[]
  adapter: string
  capabilities: string[]
  write_repositories: string[]
  max_attempts: number
  timeout_seconds: number
  options?: Record<string, unknown>
}
export type WorkflowDefinition = {
  name: string
  workspace_id: string
  steps: WorkflowStep[]
  repair_step?: string | null
  max_repairs?: number
  max_cost_usd?: number
}
export type StoredWorkflow = { id: string; definition: WorkflowDefinition }
export type WorkflowRunSummary = {
  id: string
  status: string
  source_key: string
  definition_id: string
  created_at: string
}
export type WorkflowRun = WorkflowRunSummary & {
  task: unknown
  snapshot: { definition: WorkflowDefinition; workspace: Workspace }
  execution?: {
    heads: Record<string, string>
    revision: number
    repairs: number
    cost_usd: number
    publications: {
      path: string
      head_sha: string
      state: string
      result: { url?: string; number?: number }
    }[]
  }
  artifacts?: {
    id: string
    step_id: string
    attempt: number
    name: string
    mime: string
    digest: string
    size: number
  }[]
  steps: { id: string; status: string; attempt: number; result: unknown }[]
  events: {
    seq: number
    step_id: string | null
    kind: string
    payload: unknown
    created_at: string
  }[]
}
export type WorkflowIntake = {
  id: string
  definition_id: string
  kind: string
  connector_id: string
  config: unknown
  enabled: boolean
  next_poll: number
  last_error: string | null
}
export type WorkflowPending = {
  id: string
  source_key: string
  created_at: string
  title: string | null
  definition_name: string | null
  workspace_name: string | null
  last_event: string | null
}
export type WorkflowCatalog = {
  factory_enabled?: boolean
  findings?: {
    run_id: string
    step_id: string
    run_status: string
    definition_name: string | null
    revision: number
    title: string
    severity: string
    summary: string
  }[]
  pending_runs?: WorkflowPending[]
  runners: Runner[]
  workspaces: Workspace[]
  definitions: StoredWorkflow[]
  runs: WorkflowRunSummary[]
  intakes?: WorkflowIntake[]
}

/** Minimal preparation-only workflow. */
export function preparationWorkflow(workspace: Workspace): WorkflowDefinition {
  return {
    name: `${workspace.name} · preparación`,
    workspace_id: workspace.id,
    steps: [
      {
        id: 'prepare',
        name: 'Preparar copias aisladas de los commits',
        kind: 'prepare',
        depends_on: [],
        adapter: 'prepare-workspace',
        capabilities: ['workspace:prepare'],
        write_repositories: [],
        max_attempts: 2,
        timeout_seconds: 900,
      },
    ],
  }
}

/** Delivery graph with isolated sessions, parallel QA and bounded repair. */
export function deliveryWorkflow(workspace: Workspace): WorkflowDefinition {
  const step = (
    id: string,
    name: string,
    kind: WorkflowStep['kind'],
    deps: string[],
    adapter: string,
    writes: string[] = [],
  ): WorkflowStep => ({
    id,
    name,
    kind,
    depends_on: deps,
    adapter,
    capabilities: [],
    write_repositories: writes,
    max_attempts: 3,
    timeout_seconds: 1800,
    options: kind === 'check' ? { required_checks: [id] } : {},
  })
  return {
    name: `${workspace.name} · tarea a draft PR revisado`,
    workspace_id: workspace.id,
    repair_step: 'implement',
    max_repairs: 2,
    max_cost_usd: 20,
    steps: [
      { ...preparationWorkflow(workspace).steps[0] },
      step('investigate', 'Investigar y planificar', 'agent', ['prepare'], 'isolated-investigator'),
      step(
        'tests',
        'Escribir tests de regresión',
        'agent',
        ['investigate'],
        'isolated-test-author',
        workspace.repositories.filter((r) => r.writable).map((r) => r.path),
      ),
      step(
        'implement',
        'Implementar el cambio completo',
        'agent',
        ['tests'],
        'isolated-implementer',
        workspace.repositories.filter((r) => r.writable).map((r) => r.path),
      ),
      {
        ...step(
          'verify',
          'Ejecutar tests, build y verificaciones',
          'check',
          ['implement'],
          'isolated-verification',
        ),
        options: { required_checks: ['tests', 'build'] },
      },
      step(
        'environment',
        'Levantar el entorno integrado',
        'check',
        ['verify'],
        'integrated-environment',
      ),
      step(
        'functional',
        'QA funcional y de integración',
        'check',
        ['environment'],
        'isolated-functional-qa',
      ),
      step(
        'design',
        'QA de diseño y accesibilidad',
        'check',
        ['environment'],
        'isolated-design-qa',
      ),
      step(
        'security',
        'QA de seguridad autorizado',
        'check',
        ['environment'],
        'isolated-security-qa',
      ),
      step(
        'gate',
        'Verificar evidencias del commit final',
        'gate',
        ['functional', 'design', 'security'],
        'evidence-gate',
      ),
      step(
        'publish',
        'Abrir draft PRs según las políticas',
        'publish',
        ['gate'],
        'policy-publisher',
      ),
      step(
        'review',
        'Revisión independiente con contexto limpio',
        'review',
        ['publish'],
        'isolated-reviewer',
      ),
    ],
  }
}
