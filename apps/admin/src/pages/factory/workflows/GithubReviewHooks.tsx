import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Button } from '../../../components/ui/Button'
import { Input } from '../../../components/ui/Input'
import { InlineError, SectionTitle, errorMessage, useFactory } from '../shared/ui'

export function GithubReviewHooks() {
  const { client, can } = useFactory()
  const readable=can('autonomous_agent:read')
  const writable=can('factory_policy:write')&&can('autonomous_agent:manage_connectors')
  const hooks=useQuery({queryKey:['github-review-hooks'],queryFn:()=>client.getGithubReviewHooks(),enabled:readable})
  const agents=useQuery({queryKey:['github-review-hook-agents'],queryFn:()=>client.listAutonomousAgents(),enabled:readable})
  const [agent,setAgent]=useState('')
  const [base,setBase]=useState(import.meta.env.VITE_API_URL??'')
  const [busy,setBusy]=useState(false)
  const [error,setError]=useState('')
  const [notice,setNotice]=useState('')
  async function action(work:()=>Promise<unknown>){setBusy(true);setError('');setNotice('');try{await work();await hooks.refetch()}catch(e){setError(errorMessage(e,'GitHub webhook operation failed'))}finally{setBusy(false)}}
  if(!readable)return null
  return <section className="space-y-4 rounded-xl border border-border-primary bg-card p-5" aria-label="GitHub requested reviews">
    <SectionTitle>GitHub requested reviews</SectionTitle>
    <p className="text-sm text-text-secondary">Uses the GitHub account authenticated in the backend’s <code>gh</code> CLI, not your browser or model login. A signed webhook starts a fresh review when that account is requested; new commits receive another SHA-bound review. Other users’ requests, self-authored PRs and forks are excluded.</p>
    <p className="text-sm">Current CLI reviewer: <strong>{hooks.data?.login??'Unavailable — authenticate gh on the worker'}</strong></p>
    {(error||hooks.isError||agents.isError)&&<InlineError message={error||errorMessage(hooks.error??agents.error,'Could not load GitHub integration')} />}
    {notice&&<p role="status" className="text-sm">{notice}</p>}
    <ul className="space-y-3">{hooks.data?.hooks.map(h=><li key={h.id} className="flex flex-wrap items-center justify-between gap-3 border-b border-border-primary pb-2"><div className="text-sm"><p>{h.metadata.repository} · {h.health}</p><p className="break-all text-xs text-text-tertiary">{h.metadata.callback_url}</p></div>{writable&&h.health!=='revoked'&&<Button size="sm" variant="secondary" disabled={busy} onClick={()=>{if(window.confirm('Disconnect this signed repository webhook?'))void action(async()=>{const result=await client.disconnectGithubReviewHook(h.id);if(!result.remote_deleted)setNotice('Local webhook revoked. Remove the old hook in GitHub repository settings if automatic deletion was unavailable.')})}}>Disconnect</Button>}</li>)}</ul>
    {writable&&<form className="space-y-3" onSubmit={e=>{e.preventDefault();void action(async()=>{const result=await client.registerGithubReviewHook(agent,base);setNotice(`Webhook connected for ${result.login}. Request that account as reviewer in GitHub.`)})}}>
      <label className="block text-xs">Enabled PR reviewer agent<select className="mt-1 w-full rounded-md border border-input bg-background p-2 text-sm" required value={agent} onChange={e=>setAgent(e.target.value)}><option value="">Choose a reviewer (repository comes from its configuration)</option>{agents.data?.filter(a=>a.template_key==='github_pr_reviewer'&&a.status==='enabled'&&a.validation_status==='valid').map(a=><option key={a.id} value={a.id}>{a.name}</option>)}</select></label>
      <label className="block text-xs">Public HTTPS backend URL<Input type="url" value={base} onChange={e=>setBase(e.target.value)} placeholder="https://api.nexusmind.example.com" required /></label>
      <p className="text-xs text-text-tertiary">Requires GitHub repository webhook-administration permissions and backend token encryption. The shared webhook secret is generated and encrypted server-side; no GitHub token or secret is shown here.</p>
      <Button type="submit" size="sm" disabled={busy||!hooks.data?.login}>Connect GitHub webhook</Button>
    </form>}
  </section>
}
