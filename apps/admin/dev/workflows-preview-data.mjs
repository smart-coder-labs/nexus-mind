// Illustrated, read-only fixtures. Never substitute production credentials here.
const names = [
  'kasymir-app-ui',
  'kasymir-graphql-service',
  'kasymir-mobile-app',
  'kasymir-iam-service',
  'kasymir-products-service',
  'kasymir-sales-service',
  'kasymir-common-domain',
  ...Array.from({ length: 42 }, (_, i) => `kasymir-servicio-${i + 1}`),
]
const repos = names.map((path) => ({
  path,
  remote: `https://github.com/kasymir/${path}.git`,
  head_sha: 'a'.repeat(40),
  writable: true,
}))
export const workspace = {
  id: 'workspace',
  runner_id: 'runner',
  name: 'Kasymir · Oracle',
  root_path: '/home/ubuntu/nexusmind-workspaces/kasymir',
  discovered: repos,
  repositories: repos,
  inventory_revision: 1,
  confirmed_revision: 1,
  runtime: {
    revision: 0,
    config: { agents: [], checks: [], services: [] },
    report: {
      remote_enabled: false,
      applied_revision: 0,
      images: [],
      networks: [],
      model_connections: [],
      environments: [],
      harnesses: [],
    },
    ready: false,
    missing: [
      'agent:investigation',
      'agent:test_author',
      'agent:implementation',
      'agent:reviewer',
      'check:tests',
      'check:build',
      'check:functional',
      'check:design',
      'check:security',
      'services',
      'remote_setup_disabled',
      'runner_apply',
    ],
  },
}
export const definition = {
  id: 'def',
  definition: {
    name: 'Preparar Kasymir · ejemplo',
    workspace_id: workspace.id,
    steps: [
      {
        id: 'prepare',
        name: 'Preparar repositorios',
        kind: 'prepare',
        adapter: 'prepare-workspace',
        depends_on: [],
        max_attempts: 2,
        timeout_seconds: 900,
        options: {},
        capabilities: [],
        write_repositories: [],
      },
    ],
  },
}
const graph = [
  ['prepare', 'Preparar clones', 'prepare', 'prepare-workspace', []],
  ['investigate', 'Investigar el problema', 'agent', 'isolated-investigator', ['prepare']],
  ['tests', 'Escribir tests de regresión', 'agent', 'isolated-test-author', ['investigate']],
  ['implement', 'Implementar el cambio', 'agent', 'isolated-implementer', ['tests']],
  ['verify', 'Tests y build', 'check', 'isolated-verification', ['implement']],
  ['environment', 'Levantar servicios', 'check', 'integrated-environment', ['verify']],
  ['functional', 'QA funcional', 'check', 'isolated-functional-qa', ['environment']],
  ['design', 'Diseño y accesibilidad', 'check', 'isolated-design-qa', ['environment']],
  ['security', 'QA de seguridad', 'check', 'isolated-security-qa', ['environment']],
  ['gate', 'Verificar evidencias', 'gate', 'evidence-gate', ['functional', 'design', 'security']],
  ['publish', 'Publicar draft PR', 'publish', 'policy-publisher', ['gate']],
  ['review', 'Revisión independiente', 'review', 'isolated-reviewer', ['publish']],
]
const delivery = {
  id: 'def-blocked',
  definition: {
    name: 'Corrección con QA · ejemplo',
    workspace_id: workspace.id,
    max_cost_usd: 20,
    max_repairs: 2,
    steps: graph.map(([id, name, kind, adapter, depends_on]) => ({
      id,
      name,
      kind,
      adapter,
      depends_on,
      max_attempts: 3,
      timeout_seconds: 1800,
      capabilities: [],
      write_repositories: [],
      options: {},
    })),
  },
}
export const run = {
  id: 'preview-run',
  status: 'succeeded',
  task: { title: 'Ejemplo ilustrativo, no ejecutado por esta vista previa' },
  source_key: 'manual:ejemplo-ilustrativo',
  definition_id: 'def',
  created_at: new Date().toISOString(),
  snapshot: { workspace, definition: definition.definition },
  execution: { revision: 0, repairs: 0, cost_usd: 0, heads: {}, publications: [] },
  steps: [{ id: 'prepare', status: 'succeeded', attempt: 1, result: { illustrative: true } }],
  events: [],
  artifacts: [],
}
const blocked = {
  ...run,
  id: 'preview-blocked',
  status: 'blocked',
  definition_id: delivery.id,
  source_key: 'manual:ejemplo-bloqueo',
  task: { title: 'Corregir el checkout · ejemplo' },
  snapshot: { workspace, definition: delivery.definition },
  steps: delivery.definition.steps.map((s) => ({
    id: s.id,
    status: s.id === 'prepare' ? 'succeeded' : s.id === 'investigate' ? 'blocked' : 'pending',
    attempt: s.id === 'prepare' ? 1 : 0,
    result: s.id === 'investigate' ? { hold: true, error: 'adapter_unavailable' } : null,
  })),
}
export const catalog = {
  factory_enabled: true,
  runners: [
    {
      id: 'runner',
      name: 'Oracle Kasymir',
      revoked: false,
      capabilities: ['workspace:prepare', 'adapter:prepare-workspace'],
      last_seen: new Date().toISOString(),
    },
  ],
  workspaces: [workspace],
  definitions: [definition, delivery],
  runs: [blocked, run],
  intakes: [],
  pending_runs: [
    {
      id: blocked.id,
      title: blocked.task.title,
      source_key: blocked.source_key,
      created_at: blocked.created_at,
      workspace_name: workspace.name,
      definition_name: delivery.definition.name,
      last_event: 'step.adapter_unavailable',
    },
  ],
  findings: [],
}
const agents = [
  ['preview-resolver', 'Resolver de Kasymir · ejemplo', 'github_issue_resolver', 'sonnet'],
  ['preview-reviewer', 'Reviewer independiente · ejemplo', 'github_pr_reviewer', 'opus'],
].map(([id, name, template_key, model]) => ({
  id,
  name,
  template_key,
  model,
  org_id: 'org',
  description: 'Perfil ilustrativo, sin credenciales ni ejecución real',
  template_version: 1,
  status: 'enabled',
  current_revision: 1,
  created_by: 'user',
  created_at: run.created_at,
  updated_at: run.created_at,
  validation_status: 'valid',
}))
const emptyDigest = {
  held_merges: [],
  approved_merges: [],
  blocked_runs: [],
  factory_tasks: [],
  unlabeled_shadow: 0,
  unlabeled_shadow_allows: 0,
}
export function fixture(path) {
  if (path.endsWith('/auth/me'))
    return {
      org: { id: 'org', name: 'Vista previa local', slug: 'preview' },
      user: {
        id: 'user',
        org_id: 'org',
        email: 'preview@example.invalid',
        name: 'Operador de prueba',
        role: 'super_user',
        status: 'active',
        permissions: [
          'factory_policy:read',
          'factory_policy:write',
          'autonomous_agent:read',
          'autonomous_agent:manage_connectors',
          'autonomous_agent:create',
          'autonomous_agent:update',
          'autonomous_agent:enable',
          'autonomous_agent:run',
        ],
      },
    }
  if (path.endsWith('/workflow-catalog'))
    return {
      ...catalog,
      runners: catalog.runners.map((r) => ({ ...r, last_seen: new Date().toISOString() })),
    }
  if (path.endsWith('/review-hooks')) return { login: 'CesarRuizBotAgents', hooks: [] }
  if (path.includes('/workflow-runs/')) return path.endsWith('preview-blocked') ? blocked : run
  if (path === '/v1/autonomous-agents') return agents
  const agent = agents.find(
    (a) =>
      path === `/v1/autonomous-agents/${a.id}` ||
      path === `/v1/factory/workflow-agent-profiles/${a.id}`,
  )
  if (agent)
    return {
      ...agent,
      revision: {
        id: `${agent.id}-revision-1`,
        definition_id: agent.id,
        revision: 1,
        config_hash: 'a'.repeat(64),
        config: {
          executor: 'claude',
          model: agent.model,
          model_source: 'agent',
          repository: 'kasymir/kasymir-app-ui',
          github_auth: 'server_gh_cli',
          custom_instructions: 'Ejemplo de perfil reutilizable. Respetar criterios de aceptación.',
        },
        capabilities: [],
        budgets: { max_cost_usd: 5 },
        policy_generation: 0,
        validation_status: 'valid',
        validation: {},
        validated_at: run.created_at,
        created_by: 'user',
        created_at: run.created_at,
      },
    }
  if (path.endsWith('/autonomous-agents/templates'))
    return agents.map((a) => ({
      key: a.template_key,
      version: 1,
      name:
        a.template_key === 'github_issue_resolver'
          ? 'Resolver de issues'
          : 'Reviewer de pull requests',
      description: 'Plantilla ilustrativa del catálogo',
      capabilities: [],
      default_budgets: {},
      config_schema: {},
      workflow: [],
    }))
  if (path.endsWith('/autonomous-agents/runtime'))
    return {
      status: 'unavailable',
      reason_code: 'preview_not_connected',
      checked_at: run.created_at,
      claude_version: null,
      last_success_at: null,
      last_failure_at: null,
    }
  if (path.endsWith('/autonomous-agents/pi-runtime'))
    return { status: 'blocked', reason_code: 'preview_not_connected' }
  if (path.endsWith('/autonomous-agents/settings')) return { enabled: true, retention_days: 90 }
  if (path.endsWith('/autonomous-agents/metrics'))
    return {
      queued: 0,
      running: 0,
      blocked: 0,
      open_findings: 0,
      failed_deliveries: 0,
      dead_letters: 0,
      estimated_cost_usd: 0,
    }
  if (path.endsWith('/factory/bot')) return { bot: null }
  if (path.endsWith('/factory/digest')) return emptyDigest
  if (path.endsWith('/factory/watchdog')) return null
  if (path.endsWith('/factory/economics'))
    return {
      days: 30,
      runs: 0,
      cost_usd: 0,
      by_model: [],
      tier_choices: [],
      frontier_avoidance: null,
      proposed_changes: 0,
      cost_per_proposed_change: null,
      accepted_changes_tracked: false,
    }
  if (path.endsWith('/factory/shadow/report'))
    return { classes: [], min_settled_allows: 100, max_false_low_rate: 0.01 }
  if (path.endsWith('/factory/evaluations')) return { sets: [], runs: [] }
  return []
}
