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
export const catalog = {
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
  definitions: [definition],
  runs: [run],
  intakes: [],
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
        ],
      },
    }
  if (path.endsWith('/workflow-catalog')) return catalog
  if (path.endsWith('/review-hooks')) return { login: 'CesarRuizBotAgents', hooks: [] }
  if (path.includes('/workflow-runs/')) return run
  return []
}
