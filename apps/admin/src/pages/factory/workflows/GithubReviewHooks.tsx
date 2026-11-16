import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Button } from '../../../components/ui/Button'
import { Input } from '../../../components/ui/Input'
import { InlineError, SectionTitle, errorMessage, useFactory } from '../shared/ui'

export function GithubReviewHooks() {
  const { client, can } = useFactory()
  const readable = can('autonomous_agent:read')
  const writable = can('factory_policy:write') && can('autonomous_agent:manage_connectors')
  const hooks = useQuery({ queryKey: ['github-review-hooks'], queryFn: () => client.getGithubReviewHooks(), enabled: readable })
  const agents = useQuery({ queryKey: ['github-review-hook-agents'], queryFn: () => client.listAutonomousAgents(), enabled: readable })
  const [agent, setAgent] = useState('')
  const [base, setBase] = useState(import.meta.env.VITE_API_URL ?? '')
  const [setupToken, setSetupToken] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  async function action(work: () => Promise<unknown>) {
    setBusy(true)
    setError('')
    setNotice('')
    try {
      await work()
    } catch (e) {
      setError(errorMessage(e, 'GitHub webhook operation failed'))
    } finally {
      setSetupToken('')
      await hooks.refetch()
      setBusy(false)
    }
  }
  if (!readable) return null
  return <section className="space-y-4 rounded-xl border border-border-primary bg-card p-5" aria-label="GitHub requested reviews">
    <SectionTitle>GitHub requested reviews</SectionTitle>
    <p className="text-sm text-text-secondary">Reviews use the GitHub account authenticated in the backend’s <code>gh</code> CLI. Webhook administration can use a separate saved token; this does not change the reviewer account. The signed hook also synchronizes linked findings when GitHub issues close.</p>
    <p className="text-sm">Current CLI reviewer: <strong>{hooks.data?.login ?? 'Unavailable — authenticate gh on the worker'}</strong></p>
    {(error || hooks.isError || agents.isError) && <InlineError message={error || errorMessage(hooks.error ?? agents.error, 'Could not load GitHub integration')} />}
    {notice && <p role="status" className="text-sm">{notice}</p>}
    {writable && <div className="block text-xs">
      <label htmlFor="webhook-admin-token">Webhook administration token (optional)</label>
      <Input id="webhook-admin-token" type="password" autoComplete="new-password" value={setupToken} onChange={e => setSetupToken(e.target.value)} disabled={busy} aria-describedby="webhook-token-help" />
      <span id="webhook-token-help" className="text-text-tertiary">Saved encrypted for this organization and the repository you connect or disconnect. Leave blank to reuse its saved token, otherwise the backend gh credentials are used. Requires GitHub Webhooks: Read and write. Never used for reviews.</span>
    </div>}
    <ul className="space-y-3">{hooks.data?.hooks.map(h => <li key={h.id} className="flex flex-wrap items-center justify-between gap-3 border-b border-border-primary pb-2">
      <div className="text-sm">
        <p>{h.metadata.repository} · {h.health}</p>
        <p className="break-all text-xs text-text-tertiary">{h.metadata.callback_url}</p>
        <p className="text-xs text-text-tertiary">{h.management_credential_id ? 'Encrypted administration token saved' : 'No separate administration token saved'}</p>
        {h.health === 'revoked' && h.metadata.remote_deleted !== true && <p className="text-xs">Disconnected locally; GitHub deletion is pending.</p>}
      </div>
      {writable && h.management_credential_id && <Button size="sm" variant="secondary" disabled={busy} onClick={() => {
        if (window.confirm('Forget this saved administration token? This does not revoke it in GitHub.')) {
          void action(async () => {
            await client.revokeAutonomousAgentConnector(h.management_credential_id!)
            setNotice('Saved administration token removed. Revoke the PAT in GitHub too if it is no longer needed.')
          })
        }
      }}>Forget administration token</Button>}
      {writable && (h.health !== 'revoked' || h.metadata.remote_deleted !== true) && <Button size="sm" variant="secondary" disabled={busy} onClick={() => {
        if (window.confirm(`Disconnect and delete the GitHub webhook for ${h.metadata.repository}? A supplied token will be saved for this repository.`)) {
          void action(async () => {
            const result = await client.disconnectGithubReviewHook(h.id, setupToken.trim() || undefined)
            setNotice(result.remote_deleted
              ? 'Disconnected locally and removed from GitHub. The administration credential remains saved for reconnection.'
              : 'Disconnected locally, but GitHub deletion failed. Supply or replace the administration token and retry GitHub deletion, or remove the hook in GitHub repository settings.')
          })
        }
      }}>{h.health === 'revoked' ? 'Retry GitHub deletion' : 'Disconnect'}</Button>}
    </li>)}</ul>
    {writable && <form className="space-y-3" onSubmit={e => {
      e.preventDefault()
      void action(async () => {
        const result = await client.registerGithubReviewHook(agent, base, setupToken.trim() || undefined)
        setNotice(`Webhook connected for ${result.login}. Issues and pull requests are subscribed. Request that account as reviewer in GitHub.`)
      })
    }}>
      <label className="block text-xs">Enabled PR reviewer agent<select className="mt-1 w-full rounded-md border border-input bg-background p-2 text-sm" required value={agent} onChange={e => setAgent(e.target.value)}><option value="">Choose a reviewer (repository comes from its configuration)</option>{agents.data?.filter(a => a.template_key === 'github_pr_reviewer' && a.status === 'enabled' && a.validation_status === 'valid').map(a => <option key={a.id} value={a.id}>{a.name}</option>)}</select></label>
      <label htmlFor="webhook-backend-url" className="block text-xs">Public HTTPS backend URL<Input id="webhook-backend-url" type="url" value={base} onChange={e => setBase(e.target.value)} placeholder="https://api.nexusmind.example.com" required /></label>
      <p className="text-xs text-text-tertiary">Connect again with the same reviewer and URL to update the existing hook. Backend token encryption is required; saved credentials are never returned to the browser. Use Forget administration token when it is no longer needed; this removes the stored copy, not the PAT in GitHub.</p>
      <Button type="submit" size="sm" disabled={busy || !hooks.data?.login}>Connect GitHub webhook</Button>
    </form>}
  </section>
}
