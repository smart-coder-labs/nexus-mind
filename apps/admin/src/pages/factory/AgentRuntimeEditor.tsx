import { useEffect, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { createClient } from '../../api/client'
import { Button } from '../../components/ui/Button'
import { Input } from '../../components/ui/Input'
import type { AgentRuntimeComponent, AgentRuntimeConfig, HarnessVersion } from '../../types'

export const runtimeKinds: Record<string, AgentRuntimeComponent['kind'][]> = {
  claude: ['instructions', 'skill', 'script', 'plugin', 'prompt_template', 'mcp'],
  pi: ['instructions', 'skill', 'script', 'extension', 'prompt_template', 'mcp'],
  codex: ['instructions', 'skill', 'script', 'mcp'],
  nexus: ['instructions', 'mcp'],
}
const control = 'mt-1 w-full rounded-md border border-input bg-background p-2 text-sm'

function ComponentJson({ value, onChange, onValidityChange }: { value: AgentRuntimeComponent[]; onChange: (value: AgentRuntimeComponent[]) => void; onValidityChange?: (valid: boolean) => void }) {
  const serialized = JSON.stringify(value, null, 2)
  const [draft, setDraft] = useState(serialized)
  const [error, setError] = useState('')
  useEffect(() => { setDraft(serialized); setError(''); onValidityChange?.(true) }, [serialized]) // external additions/removals
  return <label className="block text-xs">Component configuration JSON
    <textarea className={`${control} min-h-40 font-mono text-xs`} value={draft} onChange={e => {
      setDraft(e.target.value)
      try {
        const parsed: unknown = JSON.parse(e.target.value)
        if (!Array.isArray(parsed) || parsed.some(c => !c || typeof c !== 'object' || typeof c.id !== 'string' || typeof c.kind !== 'string')) throw new Error('Expected an array of components with id and kind')
        e.target.setCustomValidity(''); setError(''); onValidityChange?.(true); onChange(parsed as AgentRuntimeComponent[])
      } catch {
        const message = 'Invalid component JSON: use an array of objects with id and kind.'
        e.target.setCustomValidity(message); setError(message); onValidityChange?.(false)
      }
    }} />
    {error && <span role="alert">{error}</span>}
  </label>
}

function portableMarkdown(manifest?: Record<string, unknown>): boolean {
  if (!manifest || !['', 'agent', 'skill', 'command', 'output_style'].includes(String(manifest.format ?? ''))) return false
  const security = manifest.security as { executable?: unknown } | undefined
  if (security?.executable !== undefined && security.executable !== false) return false
  const file = (value: unknown) => {
    const entry = value as Record<string, unknown> | null
    return entry?.kind === 'file' && typeof entry.path === 'string' && entry.path.toLowerCase().endsWith('.md')
  }
  return Array.isArray(manifest.components) && manifest.components.length > 0 && manifest.components.every(value => {
    const component = value as Record<string, unknown> | null
    return component?.kind === 'folder' ? Array.isArray(component.entries) && component.entries.length > 0 && component.entries.every(file) : file(value)
  })
}

/** Shared by agent revisions and workflow step overrides. No global installs. */
export function AgentRuntimeEditor({ value, executor, onChange, onValidityChange }: { value: AgentRuntimeConfig; executor: string; onChange: (value: AgentRuntimeConfig) => void; onValidityChange?: (valid: boolean) => void }) {
  const client = createClient()
  const harnesses = useQuery({ queryKey: ['runtime-harnesses'], queryFn: () => client.listHarnesses() })
  const [candidate, setCandidate] = useState<HarnessVersion | null>(null)
  const [selectedPaths, setSelectedPaths] = useState<string[]>([])
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [kind, setKind] = useState<AgentRuntimeComponent['kind']>('mcp')
  const [id, setId] = useState('')
  const supported = runtimeKinds[executor] ?? []
  const incompatible = (value.components ?? []).filter(c => c.enabled !== false && !supported.includes(c.kind))
  const candidateComponents = Array.isArray(candidate?.manifest.components) ? candidate.manifest.components as { path: string }[] : []
  const catalogKind = ({ skill: 'skill', command: 'prompt_template', hook: 'script' } as Record<string, AgentRuntimeComponent['kind']>)[String(candidate?.manifest.format)]
  const candidateCompatible = portableMarkdown(candidate?.manifest) || Boolean(catalogKind && supported.includes(catalogKind) && candidate?.targets?.includes(executor))
  const update = (change: Partial<AgentRuntimeConfig>) => onChange({ ...value, ...change })
  useEffect(() => { setCandidate(null); setSelectedPaths([]) }, [executor])
  return <fieldset className="space-y-3 rounded-md border border-border-primary p-3">
    <legend className="px-1 text-sm font-medium">Agent runtime</legend>
    <label className="block text-xs">NexusMind profile (always connected)
      <select className={control} value={value.nexusmind_profile ?? 'essential'} onChange={e => update({ nexusmind_profile: e.target.value })}>
        {['essential', 'only_context', 'reduced_readonly', 'factory_operator', 'legacy'].map(p => <option key={p}>{p}</option>)}
      </select>
    </label>
    <p className="text-xs text-text-tertiary">Only assigned resources load. No automatic package downloads. Scripts are available to the agent, not executed on startup. Secrets use environment references such as {'${DOCS_TOKEN}'}, provisioned by the operator.</p>
    {(error || harnesses.isError) && <p role="alert" className="text-xs">{error || 'Cannot load the harness catalog. Check harness permissions.'}</p>}
    <label className="block text-xs">Add a registered harness
      <select className={control} value="" disabled={busy} onChange={async e => {
        const harness = harnesses.data?.find(h => h.id === e.target.value)
        if (!harness?.latest_version) return
        setBusy(true); setError('')
        try { const version = await client.getHarnessVersion(harness.id, harness.latest_version.version); setCandidate(version); setSelectedPaths([]) }
        catch (e) { setError(e instanceof Error ? e.message : 'Could not load harness') }
        finally { setBusy(false) }
      }}><option value="">Choose a harness</option>{harnesses.data?.filter(h => h.status === 'published' && h.latest_version && !value.harnesses?.some(v => v.id === h.id)).map(h => <option key={h.id} value={h.id}>{h.name} · {h.latest_version?.version}</option>)}</select>
    </label>
    <p className="text-xs text-text-tertiary">Markdown-only harnesses are available to every executor, regardless of their declared targets. Scripts, plugins and mixed bundles retain compatibility checks.</p>
    {candidate && <div className="space-y-2 rounded-md border border-border-primary p-2">
      {!candidateCompatible && <p role="alert" className="text-xs">This catalog format cannot be loaded by {executor}. Remote marketplace declarations are not executable bundles; import reviewed, self-contained plugin files as additional components.</p>}
      <p className="break-all text-xs">Pinned version {candidate.version} · hash {candidate.manifest_hash}</p>
      <details><summary className="cursor-pointer text-xs">Review files before approving this runtime installation</summary><pre className="max-h-64 overflow-auto whitespace-pre-wrap text-xs">{JSON.stringify(candidate.manifest, null, 2)}</pre></details>
      {candidateComponents.map(c => <label key={c.path} className="flex items-center gap-2 text-xs"><input type="checkbox" checked={selectedPaths.includes(c.path)} onChange={e => setSelectedPaths(e.target.checked ? [...selectedPaths, c.path] : selectedPaths.filter(p => p !== c.path))} />{c.path}</label>)}
      <Button size="sm" disabled={busy || !selectedPaths.length || !candidateCompatible} onClick={async () => {
        setBusy(true); setError('')
        try {
          await client.approveHarnessInstall(candidate.harness_id, candidate.version, { target_tool: executor, target_scope: 'agent-runtime', manifest_hash: candidate.manifest_hash, metadata: { warning_acknowledged: value.acknowledge_executable === true } })
          update({ harnesses: [...(value.harnesses ?? []), { id: candidate.harness_id, version: candidate.version, manifest_hash: candidate.manifest_hash, components: selectedPaths }] }); setCandidate(null)
        } catch (e) { setError(e instanceof Error ? e.message : 'Approval failed') }
        finally { setBusy(false) }
      }}>Approve selected files for this runtime</Button>
    </div>}
    {value.harnesses?.map(h => <div key={h.id} className="flex items-center justify-between gap-2 text-xs"><span>{h.id} · {h.version} · {h.components.join(', ')}</span><Button size="sm" variant="ghost" onClick={() => update({ harnesses: value.harnesses?.filter(v => v.id !== h.id) })}>Remove harness</Button></div>)}
    <div className="flex flex-wrap items-end gap-2">
      <label className="text-xs">Component ID<Input value={id} onChange={e => setId(e.target.value)} placeholder="backend-review" /></label>
      <label className="text-xs">Component type<select className={control} value={supported.includes(kind) ? kind : supported[0]} onChange={e => setKind(e.target.value as AgentRuntimeComponent['kind'])}>{supported.map(k => <option key={k}>{k}</option>)}</select></label>
      <Button size="sm" disabled={!/^[a-zA-Z0-9_-]+$/.test(id) || value.components?.some(c => c.id === id)} onClick={() => {
        const selected = supported.includes(kind) ? kind : supported[0]
        if (!selected) return
        const component: AgentRuntimeComponent = selected === 'mcp' ? { id, kind: selected, server: { command: 'your-mcp-server', args: [], env: {} } } : { id, kind: selected, files: selected === 'skill' ? { 'SKILL.md': `---\nname: ${id.toLowerCase().replace(/_/g, '-')}\ndescription: Describe when to use this skill\n---\n` } : selected === 'extension' ? { 'index.ts': '' } : selected === 'plugin' ? { 'plugin.json': JSON.stringify({ name: id.toLowerCase().replace(/_/g, '-'), version: '1.0.0' }, null, 2) } : { [selected === 'script' ? 'run.sh' : 'prompt.md']: '' } }
        update({ components: [...(value.components ?? []), component] }); setId('')
      }}>Add component</Button>
    </div>
    {incompatible.length > 0 && <p role="alert" className="text-xs">Unsupported by {executor}: {incompatible.map(c => c.id).join(', ')}. Remove or replace these components before saving.</p>}
    {value.components?.map(c => <label key={c.id} className="flex items-center gap-2 text-xs"><input type="checkbox" checked={c.enabled !== false} onChange={e => update({ components: value.components?.map(v => v.id === c.id ? { ...v, enabled: e.target.checked } : v) })} />{c.id} · {c.kind}</label>)}
    <ComponentJson value={value.components ?? []} onChange={components => update({ components })} onValidityChange={onValidityChange} />
    <label className="flex items-start gap-2 text-xs"><input type="checkbox" checked={value.acknowledge_executable === true} onChange={e => update({ acknowledge_executable: e.target.checked })} />I reviewed and approve executable resources, plugins and MCP servers. They execute with the permissions of the selected runtime.</label>
  </fieldset>
}
