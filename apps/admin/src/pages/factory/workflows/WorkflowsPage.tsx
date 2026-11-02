import { useState, type FormEvent } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Button } from '../../../components/ui/Button'
import { Badge } from '../../../components/ui/Badge'
import { Input } from '../../../components/ui/Input'
import { InlineError, PageHeader, RawJson, SectionTitle, useFactory, errorMessage } from '../shared/ui'
import { WorkflowEditor } from './WorkflowEditor'
import { GithubReviewHooks } from './GithubReviewHooks'
import { deliveryWorkflow, preparationWorkflow, type Repository, type Workspace, type WorkflowDefinition, type WorkflowRun } from './types'

const control = 'w-full rounded-md border border-input bg-background px-3 py-2 text-sm text-foreground'
const card = 'space-y-4 rounded-xl border border-border-primary bg-card p-5'

function Inventory({ workspace, save, disabled }: { workspace: Workspace; save: (repos: Repository[]) => void; disabled: boolean }) {
  const [selected, setSelected] = useState(() => workspace.discovered.map(repo => ({ ...repo, selected: true, writable: workspace.repositories.some(r => r.path === repo.path && r.writable) })))
  return <div className="space-y-3">
    <p className="text-xs text-text-tertiary">Inventory revision {workspace.inventory_revision} · {workspace.confirmed_revision === workspace.inventory_revision ? 'Confirmed' : 'Needs confirmation'}</p>
    {!selected.length && <p className="text-sm text-text-secondary">Waiting for the runner to discover repositories. Your browser cannot scan remote folders.</p>}
    {selected.map(repo => <div key={repo.path} className="flex flex-wrap items-center gap-3 rounded-md border border-border-primary p-3">
      <label className="flex min-w-0 flex-1 items-center gap-2 text-sm"><input type="checkbox" checked={repo.selected} disabled={disabled} onChange={e => setSelected(rows => rows.map(r => r.path === repo.path ? { ...r, selected: e.target.checked } : r))} /><span className="break-all font-mono">{repo.path}</span></label>
      <code className="text-xs text-text-tertiary">{repo.head_sha.slice(0, 12)}</code>
      <label className="flex items-center gap-2 text-xs"><input type="checkbox" checked={repo.writable} disabled={disabled || !repo.selected} onChange={e => setSelected(rows => rows.map(r => r.path === repo.path ? { ...r, writable: e.target.checked } : r))} />Allow implementation writes</label>
    </div>)}
    {selected.length > 0 && <Button size="sm" disabled={disabled || !selected.some(r => r.selected)} onClick={() => save(selected.filter(r => r.selected).map(({ selected: _selected, ...repo }) => repo))}>Confirm selected repositories</Button>}
  </div>
}

function RunView({ run, controlRun, approve, artifact, disabled }: { run: WorkflowRun; controlRun: (action: 'cancel' | 'retry' | 'resume', step?: string) => void; approve: (action:string, revision:number)=>void; artifact:(id:string)=>void; disabled: boolean }) {
  return <section className={card} aria-label="Workflow execution">
    <div className="flex flex-wrap items-center justify-between gap-3"><SectionTitle>{run.snapshot.definition.name}</SectionTitle><Badge>{run.status}</Badge></div>
    <p className="break-all font-mono text-xs text-text-tertiary">{run.id}</p>
    {run.execution && <div className="space-y-2 text-sm"><p>Change revision {run.execution.revision} · repair cycles {run.execution.repairs}/{run.snapshot.definition.max_repairs ?? 2} · ${run.execution.cost_usd.toFixed(3)} / ${run.snapshot.definition.max_cost_usd ?? 20}</p><RawJson label="Pinned change set" value={run.execution.heads} />{run.execution.publications.map(p => <p key={p.path}>{p.path}: {p.result.url?.startsWith('https://github.com/') ? <a className="underline" href={p.result.url} target="_blank" rel="noopener noreferrer">PR #{p.result.number}</a> : p.state} <code>{p.head_sha.slice(0,12)}</code></p>)}</div>}
    {run.status === 'blocked' && <div className="space-y-2 rounded-md border border-border-primary p-3"><p className="text-sm">Approve only after inspecting the task, change set and verification evidence. Each action is independent; never policies and missing verification cannot be overridden.</p><div className="flex flex-wrap gap-2">{['fix','publish','open_pr'].map(action => <Button key={action} size="sm" variant="secondary" disabled={disabled} onClick={() => {if(window.confirm(`Approve ${action} for this exact workflow change set?`)) approve(action, action==='fix'?0:(run.execution?.revision ?? 0))}}>Approve {action}</Button>)}<Button size="sm" variant="ghost" disabled={disabled} onClick={() => controlRun('resume')}>Resume reconciled queue</Button></div></div>}
    <ol aria-label="Workflow phases" className="space-y-2">
      {run.snapshot.definition.steps.map(step => {
        const state = run.steps.find(s => s.id === step.id)
        return <li key={step.id} className="rounded-md border border-border-primary p-3">
          <div className="flex flex-wrap items-center justify-between gap-3"><span className="text-sm font-medium">{step.name}</span><Badge>{state?.status ?? 'pending'}</Badge></div>
          <p className="mt-1 text-xs text-text-tertiary">{step.kind} · {step.adapter} · attempt {state?.attempt ?? 0}/{step.max_attempts}</p>
          {step.depends_on.length > 0 && <p className="mt-1 text-xs text-text-tertiary">After: {step.depends_on.join(', ')}</p>}
          {state?.result != null && <RawJson label="Evidence and result" value={state.result} />}
          {run.status === 'blocked' && state?.status === 'blocked' && (state.attempt < step.max_attempts) && <Button size="sm" variant="secondary" disabled={disabled} onClick={() => { if (window.confirm('Have you reconciled any external effects and stopped the previous worker? Retrying can repeat work.')) controlRun('retry', step.id) }}>Retry after reconciliation</Button>}
        </li>
      })}
    </ol>
    {['running', 'queued', 'blocked'].includes(run.status) && <Button size="sm" variant="secondary" disabled={disabled} onClick={() => controlRun('cancel')}>Cancel execution</Button>}
    <p className="text-xs text-text-tertiary">Cancellation fences future results; it does not undo external side effects or instantly terminate a disconnected runner.</p>
    <SectionTitle>Logs & artifacts</SectionTitle><ul className="space-y-1">{run.artifacts?.map(a => <li key={a.id}><button type="button" className="text-left text-sm underline" onClick={() => artifact(a.id)}>{a.step_id} · attempt {a.attempt} · {a.name} ({Math.ceil(a.size/1024)} KB)</button></li>)}</ul>
    <SectionTitle>Event log</SectionTitle>
    <ol className="max-h-96 space-y-2 overflow-auto" aria-label="Workflow event log">{[...run.events].reverse().map(event => <li key={event.seq} className="border-b border-border-primary pb-2 text-xs"><span className="text-text-tertiary">{event.created_at} · </span>{event.step_id && <code>{event.step_id} · </code>}{event.kind}<RawJson label="Event details" value={event.payload} /></li>)}</ol>
  </section>
}

export default function WorkflowsPage() {
  const { client, can } = useFactory()
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [runnerName, setRunnerName] = useState('')
  const [newToken, setNewToken] = useState<{ id: string; token: string } | null>(null)
  const [runnerId, setRunnerId] = useState('')
  const [workspaceName, setWorkspaceName] = useState('')
  const [rootPath, setRootPath] = useState('')
  const [editor, setEditor] = useState('')
  const [definitionId, setDefinitionId] = useState('')
  const [sourceKey, setSourceKey] = useState('')
  const [taskTitle, setTaskTitle] = useState('')
  const [selectedRun, setSelectedRun] = useState('')
  const [linearConnector, setLinearConnector] = useState('')
  const [linearTeam, setLinearTeam] = useState('')
  const [linearProject, setLinearProject] = useState('')
  const [intakeNotice, setIntakeNotice] = useState('')
  const [intakeKind, setIntakeKind] = useState('linear')
  const [intakeConnector, setIntakeConnector] = useState('')
  const [intakeConfig, setIntakeConfig] = useState('{}')
  const [openedArtifact, setOpenedArtifact] = useState<{mime:string;content_base64:string}|null>(null)
  const readable = can('factory_policy:read')
  const writable = can('factory_policy:write')
  const catalog = useQuery({ queryKey: ['workflow-catalog'], queryFn: () => client.getWorkflowCatalog(), enabled: readable, refetchInterval: 5000 })
  const run = useQuery({ queryKey: ['workflow-run', selectedRun], queryFn: () => client.getFactoryWorkflowRun(selectedRun), enabled: readable && Boolean(selectedRun), refetchInterval: 2500 })
  async function perform(work: () => Promise<unknown>) {
    setBusy(true); setError('')
    try { await work(); await catalog.refetch(); if (selectedRun) await run.refetch() }
    catch (failure) { setError(errorMessage(failure, 'Operation failed')) }
    finally { setBusy(false) }
  }
  let editorWorkspaceId = ''
  try { editorWorkspaceId = (JSON.parse(editor) as WorkflowDefinition).workspace_id } catch { /* Advanced drafts may be incomplete. */ }
  const submit = (event: FormEvent, work: () => Promise<unknown>) => { event.preventDefault(); void perform(work) }
  if (!readable) return <div className="p-6"><InlineError message="The factory_policy:read permission is required." /></div>
  return <div className="mx-auto max-w-7xl space-y-6 p-6 md:p-8">
    <PageHeader title="Workspaces & workflows" subtitle="Register folders on a runner, authorize repositories, and follow durable execution phases." />
    <div role="note" className="rounded-md border border-border-primary bg-muted p-4 text-sm text-text-secondary">Repository code runs only in locally configured container/VM adapters. Configure pinned images, platform capabilities, test services and dedicated development environment references on the runner. Missing adapters wait visibly; missing QA evidence blocks publication. Draft PRs are never automatically merged.</div>
    {(error || catalog.isError || run.isError) && <InlineError message={error || errorMessage(catalog.error ?? run.error, 'Could not load workflows')} onRetry={() => { void catalog.refetch(); if (selectedRun) void run.refetch() }} />}
    {catalog.isPending && <p role="status">Loading workspaces…</p>}
    <GithubReviewHooks />
    <div className="grid gap-6 lg:grid-cols-2">
      <section className={card}>
        <SectionTitle>Runners</SectionTitle>
        <p className="text-sm text-text-secondary">A runner lives on the machine with your clones. Its credential cannot edit policies or workflows.</p>
        {catalog.data?.runners.map(r => <div key={r.id} className="flex flex-wrap items-center justify-between gap-2 border-b border-border-primary py-2"><div><p className="text-sm font-medium">{r.name} {r.revoked && '(revoked)'}</p><p className="text-xs text-text-tertiary">Last seen: {r.last_seen ?? 'Not connected'}</p></div>{writable && <Button size="sm" variant="ghost" disabled={busy} onClick={() => {if(window.confirm('Rotate this runner credential and fence active jobs?')) void perform(async()=>setNewToken(await client.rotateFactoryRunner(r.id)))}}>Rotate credential</Button>}{!r.revoked && writable && <Button size="sm" variant="ghost" disabled={busy} onClick={() => { if (window.confirm(`Revoke ${r.name}?`)) void perform(() => client.revokeFactoryRunner(r.id)) }}>Revoke</Button>}</div>)}
        {writable && <form className="flex gap-2" onSubmit={e => submit(e, async () => { const runner = await client.registerFactoryRunner(runnerName); setNewToken(runner); setRunnerId(runner.id); setRunnerName('') })}><Input aria-label="Runner name" placeholder="My workstation" value={runnerName} onChange={e => setRunnerName(e.target.value)} required maxLength={120} /><Button type="submit" size="sm" disabled={busy}>Register runner</Button></form>}
        {newToken && <div className="space-y-3 rounded-md border border-border-primary p-3" role="status"><p className="text-sm">Save this runner credential in a local 0600 configuration file. It is shown only now.</p><pre className="overflow-auto whitespace-pre-wrap break-all text-xs">{newToken.token}</pre><Button size="sm" variant="secondary" onClick={() => setNewToken(null)}>I saved it — hide credential</Button></div>}
      </section>
      <section className={card}>
        <SectionTitle>Register a workspace folder</SectionTitle>
        <p className="text-sm text-text-secondary">Use an absolute path on the runner, for example /home/you/work/kasymir. Clones are never reorganized or edited in place.</p>
        {writable && <form className="space-y-3" onSubmit={e => submit(e, async () => { await client.createFactoryWorkspace({ runner_id: runnerId, name: workspaceName, root_path: rootPath }); setWorkspaceName(''); setRootPath('') })}>
          <label className="block text-xs">Runner<select className={control} value={runnerId} onChange={e => setRunnerId(e.target.value)} required><option value="">Choose a runner</option>{catalog.data?.runners.filter(r => !r.revoked).map(r => <option key={r.id} value={r.id}>{r.name}</option>)}</select></label>
          <label className="block text-xs">Workspace name<Input value={workspaceName} onChange={e => setWorkspaceName(e.target.value)} placeholder="Kasymir" required maxLength={120} /></label>
          <label className="block text-xs">Folder on runner<Input value={rootPath} onChange={e => setRootPath(e.target.value)} placeholder="/home/you/work/kasymir" required pattern="/.*" /></label>
          <Button type="submit" size="sm" disabled={busy}>Register folder</Button>
        </form>}
      </section>
    </div>
    {catalog.data?.workspaces.length === 0 && <p className="text-sm text-text-secondary">No workspaces yet. Register a runner and its folder to begin.</p>}
    {catalog.data?.workspaces.map(w => <section key={w.id} className={card}><SectionTitle>{w.name}</SectionTitle><p className="break-all font-mono text-xs text-text-tertiary">{w.root_path}</p>
      <Inventory key={`${w.id}:${w.inventory_revision}:${w.confirmed_revision}`} workspace={w} disabled={!writable || busy} save={repos => { void perform(() => client.confirmFactoryWorkspace(w.id, w.inventory_revision, repos)) }} />
      {writable && w.confirmed_revision !== null && w.confirmed_revision === w.inventory_revision && <div className="flex flex-wrap gap-2"><Button size="sm" variant="secondary" onClick={() => setEditor(JSON.stringify(preparationWorkflow(w), null, 2))}>Prepare workspace workflow</Button><Button size="sm" variant="secondary" disabled={!w.repositories.some(r=>r.writable)} onClick={() => setEditor(JSON.stringify(deliveryWorkflow(w), null, 2))}>New end-to-end workflow</Button></div>}
    </section>)}
    {editor && writable && <form className={card} onSubmit={e => submit(e, async () => { const definition = JSON.parse(editor) as WorkflowDefinition; await client.createFactoryWorkflow(definition); setEditor('') })}><SectionTitle>New immutable workflow revision</SectionTitle><WorkflowEditor value={editor} onChange={setEditor} adapters={catalog.data?.runners.find(r=>r.id===catalog.data?.workspaces.find(w=>w.id===editorWorkspaceId)?.runner_id)?.capabilities.filter(c=>c.startsWith('adapter:')).map(c=>c.slice(8))??[]} repositories={catalog.data?.workspaces.find(w=>w.id===editorWorkspaceId)?.repositories.filter(r=>r.writable).map(r=>r.path)??[]} /><div className="flex gap-2"><Button type="submit" size="sm" disabled={busy}>Save definition</Button><Button size="sm" variant="ghost" onClick={() => setEditor('')}>Discard</Button></div></form>}
    <section className={card}><SectionTitle>Start a workflow</SectionTitle>
      {writable && <form className="space-y-3" onSubmit={e => submit(e, async () => { const started = await client.startFactoryWorkflow(definitionId, sourceKey, { title: taskTitle, origin_trust: 'untrusted' }); setSelectedRun(started.id) })}>
        <label className="block text-xs">Workflow<select className={control} required value={definitionId} onChange={e => setDefinitionId(e.target.value)}><option value="">Choose a workflow</option>{catalog.data?.definitions.map(d => <option key={d.id} value={d.id}>{d.definition.name} · {d.id.slice(0, 8)}</option>)}</select></label>
        <label className="block text-xs">Source key (stable deduplication ID)<Input value={sourceKey} onChange={e => setSourceKey(e.target.value)} placeholder="linear:issue-uuid or manual:task-id" required maxLength={512} /></label>
        <label className="block text-xs">Task title<Input value={taskTitle} onChange={e => setTaskTitle(e.target.value)} required /></label><Button type="submit" size="sm" disabled={busy}>Start execution</Button>
      </form>}
    </section>
    {writable && <section className={card}><SectionTitle>Linear intake</SectionTitle>
      <p className="text-sm text-text-secondary">Uses the selected Linear key’s assigned issues, excluding completed/canceled states. Urgent first, unprioritized last; oldest wins ties. An existing source key is not started again in this workspace. Unresolved blockers and incomplete dependency information are excluded.</p>
      <p className="text-xs text-text-tertiary">Register an encrypted target_secret connector with purpose factory_workflow_linear and scope linear:read. The key stays on the control plane.</p>
      <form className="space-y-3" onSubmit={e => submit(e, async () => {
        setIntakeNotice(''); const result = await client.startNextLinearWorkflow(definitionId, linearConnector, linearTeam, linearProject)
        if (result.run) setSelectedRun(result.run.id); else setIntakeNotice('No unclaimed assigned issue was found.')
      })}>
        <label className="block text-xs">Workflow<select className={control} required value={definitionId} onChange={e => setDefinitionId(e.target.value)}><option value="">Choose a workflow</option>{catalog.data?.definitions.map(d => <option key={d.id} value={d.id}>{d.definition.name}</option>)}</select></label>
        <label className="block text-xs">Linear connector ID<Input value={linearConnector} onChange={e => setLinearConnector(e.target.value)} required /></label>
        <label className="block text-xs">Team ID (optional)<Input value={linearTeam} onChange={e => setLinearTeam(e.target.value)} /></label>
        <label className="block text-xs">Project ID (optional)<Input value={linearProject} onChange={e => setLinearProject(e.target.value)} /></label>
        <Button type="submit" size="sm" disabled={busy}>Take next assigned issue</Button>
      </form>{intakeNotice && <p role="status" className="text-sm">{intakeNotice}</p>}
    </section>}
    <section className={card}><SectionTitle>Executions</SectionTitle>{catalog.data?.runs.length === 0 && <p className="text-sm text-text-secondary">No workflow executions yet.</p>}<ul className="space-y-2">{catalog.data?.runs.map(r => <li key={r.id}><button type="button" className="flex w-full flex-wrap items-center justify-between gap-2 rounded-md border border-border-primary p-3 text-left focus-visible:outline focus-visible:outline-2" onClick={() => setSelectedRun(r.id)}><span className="break-all text-sm">{r.source_key}</span><Badge>{r.status}</Badge></button></li>)}</ul></section>
    {writable && <section className={card}><SectionTitle>Connected intakes</SectionTitle><p className="text-sm text-text-secondary">Linear, Slack and Sentry feed the same workflow. Sources poll every five minutes while the runner is active, one outstanding task per workspace. New tasks obey fix policies and may require human approval.</p>
      <ul className="space-y-2">{catalog.data?.intakes?.map(i=><li key={i.id} className="flex flex-wrap items-center gap-3 text-sm"><span>{i.kind} · {i.enabled?'Enabled':'Disabled'} · {i.last_error ?? 'No polling errors'}</span><Button size="sm" variant="ghost" disabled={busy} onClick={()=>{void perform(()=>client.deleteWorkflowIntake(i.id))}}>Remove source</Button></li>)}</ul>
      <form className="space-y-3" onSubmit={e=>submit(e,async()=>{await client.createWorkflowIntake({definition_id:definitionId,kind:intakeKind,connector_id:intakeConnector,config:JSON.parse(intakeConfig)});setIntakeConnector('')})}>
        <label className="block text-xs">Target workflow<select className={control} value={definitionId} onChange={e=>setDefinitionId(e.target.value)} required><option value="">Choose a workflow</option>{catalog.data?.definitions.map(d=><option key={d.id} value={d.id}>{d.definition.name}</option>)}</select></label>
        <label className="block text-xs">Intake app<select className={control} value={intakeKind} onChange={e=>{setIntakeKind(e.target.value);setIntakeConfig(e.target.value==='slack'?'{"channel_id":"","allowed_reactors":[]}':e.target.value==='sentry'?'{"base_url":"https://sentry.io","org_slug":"","project_slug":"","query":"is:unresolved"}':'{}')}}><option>linear</option><option>slack</option><option>sentry</option></select></label>
        <label className="block text-xs">Dedicated intake connector ID<Input value={intakeConnector} onChange={e=>setIntakeConnector(e.target.value)} required /></label>
        <label className="block text-xs">Source configuration (no credentials)<textarea className={`${control} h-28 font-mono`} value={intakeConfig} onChange={e=>setIntakeConfig(e.target.value)} /></label>
        <Button type="submit" size="sm" disabled={busy}>Connect intake</Button><Button size="sm" variant="secondary" disabled={busy} onClick={()=>{void perform(()=>client.pollWorkflowIntakes())}}>Poll due sources</Button>
      </form></section>}
    {openedArtifact && <section className={card} aria-label="Artifact content"><div className="flex items-center justify-between"><SectionTitle>Artifact content</SectionTitle><Button size="sm" variant="ghost" onClick={()=>setOpenedArtifact(null)}>Close artifact</Button></div>{openedArtifact.mime.startsWith('image/')?<img className="max-w-full" alt="QA evidence" src={`data:${openedArtifact.mime};base64,${openedArtifact.content_base64}`} />:<pre className="max-h-96 overflow-auto whitespace-pre-wrap break-words text-xs">{new TextDecoder().decode(Uint8Array.from(atob(openedArtifact.content_base64),c=>c.charCodeAt(0)))}</pre>}</section>}
    {run.data && <RunView run={run.data} disabled={busy || !writable} artifact={id=>{void perform(async()=>setOpenedArtifact(await client.getWorkflowArtifact(id)))}} approve={(action,revision)=>{void perform(()=>client.approveWorkflowAction(run.data.id,action,revision))}} controlRun={(action, step) => { void perform(() => client.controlFactoryWorkflowRun(run.data.id, action, step)) }} />}
  </div>
}
