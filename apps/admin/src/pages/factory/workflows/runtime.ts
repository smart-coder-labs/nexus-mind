import type { AutonomousAgentDefinition } from '../../../types'
export type WorkflowAgentProfile = Pick<
  AutonomousAgentDefinition,
  'id' | 'name' | 'template_key' | 'status' | 'validation_status'
> & { revision: { revision: number; config_hash: string; config: Record<string, unknown> } }
export type AgentRole = 'investigation' | 'test_author' | 'implementation' | 'reviewer'
export type RuntimeAgent = {
  role: AgentRole
  harness?: string
  binding?: { id: string; revision: number; name: string; config_hash: string } | null
  inherit_model?: boolean
  instructions?: string
  image: string
  model: string
  model_connection: string
  network: string
}
export type RuntimeCheck = {
  name: string
  image: string
  cwd: string
  command: string[]
  environment_refs: string[]
}
export type RuntimeService = Omit<RuntimeCheck, 'name'> & { name: string; healthcheck: string[] }
export type RuntimeConfig = {
  agents: RuntimeAgent[]
  checks: RuntimeCheck[]
  services: RuntimeService[]
}
export type RuntimeState = {
  revision: number
  config: RuntimeConfig
  ready: boolean
  missing: string[]
  reported_at?: string | null
  report: {
    remote_enabled: boolean
    applied_revision: number
    images: string[]
    networks: string[]
    model_connections: string[]
    environments: string[]
    harnesses?: string[]
    managed_images?: Record<string, string>
    error?: string | null
  }
}
export const emptyRuntime = (): RuntimeState => ({
  revision: 0,
  config: { agents: [], checks: [], services: [] },
  ready: false,
  missing: [
    'remote_setup_disabled',
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
    'runner_apply',
  ],
  report: {
    remote_enabled: false,
    applied_revision: 0,
    images: [],
    networks: [],
    model_connections: [],
    environments: [],
  },
})
export const agentRoles: { id: AgentRole; label: string; description: string; model: string }[] = [
  {
    id: 'investigation',
    label: 'Investigación',
    description: 'Entiende el problema y los contratos entre repositorios. Solo lectura.',
    model: 'haiku',
  },
  {
    id: 'test_author',
    label: 'Tests de regresión',
    description: 'Escribe pruebas que reproduzcan el fallo antes de implementar.',
    model: 'haiku',
  },
  {
    id: 'implementation',
    label: 'Implementación',
    description: 'Modifica únicamente los repositorios autorizados.',
    model: 'sonnet',
  },
  {
    id: 'reviewer',
    label: 'Revisión independiente',
    description: 'Sesión nueva; recibe código y evidencias, no la conversación del implementador.',
    model: 'opus',
  },
]
export const checkTypes = [
  { id: 'tests', label: 'Tests' },
  { id: 'build', label: 'Build' },
  { id: 'lint', label: 'Lint' },
  { id: 'typecheck', label: 'Tipos' },
  { id: 'functional', label: 'QA funcional' },
  { id: 'design', label: 'Diseño y accesibilidad' },
  { id: 'security', label: 'Seguridad' },
]
export function blockerLabel(code: string): string {
  if (code === 'harness:pi' || code === 'model_connection:backend')
    return 'Verificar el SDK Pi automático, la autorización y el OAuth del backend'
  if (code === 'image:') return 'Esperar la preparación automática del SDK del runner'
  if (code.startsWith('model:')) return `Seleccionar modelo explícito para ${code.slice(6)}`
  if (code.startsWith('harness:'))
    return `Instalar y autorizar el adaptador ${code.slice(8)} en el runner`
  if (code.startsWith('agent_binding:'))
    return 'Revisar el agente vinculado: debe estar habilitado y validado'
  if (code.startsWith('agent:'))
    return `Configurar ${agentRoles.find((r) => r.id === code.slice(6))?.label.toLowerCase() ?? code.slice(6)}`
  if (code.startsWith('check:'))
    return `Añadir ${checkTypes.find((r) => r.id === code.slice(6))?.label.toLowerCase() ?? code.slice(6)}`
  if (code.startsWith('image:'))
    return 'Verificar la preparación del SDK o entorno autorizado en el runner'
  if (code.startsWith('model_connection:'))
    return `Conectar el proxy de modelo «${code.slice(17)}» en el runner`
  if (code.startsWith('environment:'))
    return `Registrar el entorno privado «${code.slice(12)}» en el runner`
  if (code.startsWith('network:')) return `Autorizar la red interna «${code.slice(8)}»`
  return (
    (
      {
        remote_setup_disabled: 'Permitir configuración remota en el runner',
        services: 'Configurar servicios de pruebas',
        runner_apply: 'Esperar a que el runner aplique la revisión',
        runner_configuration_error: 'Revisar el rechazo de configuración del runner',
      } as Record<string, string>
    )[code] ?? code
  )
}
/** Shell-style tokenization only. These argv are never executed by a host shell. */
export function parseCommand(line: string): string[] {
  const args: string[] = []
  let word = '',
    quote = '',
    started = false
  for (let i = 0; i < line.length; i++) {
    const c = line[i]
    if (quote === "'") {
      if (c === "'") quote = ''
      else word += c
      continue
    }
    if (c === '\\' && quote !== "'") {
      const next = line[++i]
      if (next === undefined) throw new Error('La barra final necesita un carácter.')
      word += quote === '"' && !['"', '\\', '$', '`'].includes(next) ? '\\' + next : next
      started = true
      continue
    }
    if (quote === '"') {
      if (c === '"') quote = ''
      else word += c
      continue
    }
    if (c === '"' || c === "'") {
      quote = c
      started = true
      continue
    }
    if (/\s/.test(c)) {
      if (started) {
        args.push(word)
        word = ''
        started = false
      }
      continue
    }
    word += c
    started = true
  }
  if (quote) throw new Error('Cierra las comillas del comando.')
  if (started) args.push(word)
  if (!args.length || args.some((a) => ['&&', '||', ';', '|', '>', '<'].includes(a)))
    throw new Error('Usa un solo comando por comprobación, sin operadores de shell.')
  return args
}
export const formatCommand = (argv: string[]) =>
  argv
    .map((v) => (/^[a-zA-Z0-9_./:@=,+-]+$/.test(v) ? v : "'" + v.split("'").join("'\\''") + "'"))
    .join(' ')
