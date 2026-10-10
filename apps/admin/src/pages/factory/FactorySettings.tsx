import { useEffect, useState, type ReactNode } from 'react'
import { useMutation } from '@tanstack/react-query'
import { Navigate } from 'react-router-dom'
import { AlertTriangle, RefreshCw } from 'lucide-react'
import { Badge } from '../../components/ui/Badge'
import { Button } from '../../components/ui/Button'
import { Input } from '../../components/ui/Input'
import { Skeleton } from '../../components/ui/Skeleton'
import { SandboxBotPanel } from './SandboxBotPanel'
import { fmtDateTime, humanize, runtimeStatusMeta } from './shared/format'
import { PauseAllControl, useFactorySettings, useRuntimeHealth } from './shared/fleet'
import { InlineError, PageHeader, PermissionNote, SectionTitle, errorMessage, useFactory } from './shared/ui'

function Section({ id, title, description, children, tone }: { id: string; title: string; description: string; children: ReactNode; tone?: 'danger' }) {
  return (
    <section aria-labelledby={id} className={`min-w-0 space-y-4 rounded-xl border bg-card p-4 sm:p-5 ${tone === 'danger' ? 'border-status-error/25' : 'border-border-primary'}`}>
      <div>
        <SectionTitle id={id}>{title}</SectionTitle>
        <p className="mt-1 max-w-2xl text-sm text-text-secondary">{description}</p>
      </div>
      {children}
    </section>
  )
}

const RETENTION_MIN = 7
const RETENTION_MAX = 3650

/** /factory/settings — runtime health, pause all agents, retention and the sandbox bot. */
export default function FactorySettings() {
  const { can, client, invalidate } = useFactory()
  const { runtime, check } = useRuntimeHealth()
  const { settings, toggle } = useFactorySettings()
  const [retention, setRetention] = useState('')
  const saveRetention = useMutation({ mutationFn: (days: number) => client.patchAutonomousAgentSettings({ retention_days: days }), onSuccess: () => invalidate('autonomous-settings') })
  useEffect(() => { if (settings.data) setRetention(String(settings.data.retention_days)) }, [settings.data])

  if (!can('autonomous_agent:read')) return <Navigate to="/401" replace />

  const canManage = can('autonomous_agent:enable')
  const meta = runtimeStatusMeta(runtime.data?.status)
  const days = Number(retention)
  const retentionValid = retention.trim() !== '' && Number.isInteger(days) && days >= RETENTION_MIN && days <= RETENTION_MAX
  const retentionDirty = settings.data != null && String(settings.data.retention_days) !== retention

  return (
    <div className="mx-auto min-w-0 max-w-4xl space-y-6 p-6 md:p-8">
      <PageHeader title="Factory settings" subtitle="The runtime your agents run on, the switch that pauses them all, and how long their history is kept." />

      <Section id="runtime-title" title="Runtime" description="Agents run inside the Claude Code runtime on this server. While it is not ready, runs wait instead of failing.">
        {runtime.isLoading ? <Skeleton className="h-6 w-48" /> : runtime.isError ? (
          <InlineError message="Could not check the runtime." onRetry={() => void runtime.refetch()} />
        ) : (
          <div className="space-y-3">
            <div className="flex flex-wrap items-center gap-3">
              <Badge size="md" variant={meta.variant} dot>{meta.label}</Badge>
              {runtime.data?.claude_version && <span className="text-sm text-text-secondary">Claude Code {runtime.data.claude_version}</span>}
              {runtime.data?.reason_code && runtime.data.status !== 'ready' && <span className="text-sm text-text-tertiary">{humanize(runtime.data.reason_code)}</span>}
            </div>
            <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1 text-sm">
              <dt className="text-text-tertiary">Last checked</dt><dd className="text-text-secondary">{fmtDateTime(runtime.data?.checked_at) || 'Never'}</dd>
              {runtime.data?.last_success_at && <><dt className="text-text-tertiary">Last healthy</dt><dd className="text-text-secondary">{fmtDateTime(runtime.data.last_success_at)}</dd></>}
              {runtime.data?.last_failure_at && <><dt className="text-text-tertiary">Last problem</dt><dd className="text-text-secondary">{fmtDateTime(runtime.data.last_failure_at)}</dd></>}
            </dl>
            {runtime.data?.status === 'reauth_required' && (
              <p className="flex gap-2 rounded-md border border-status-warning/25 bg-status-warning/[0.08] px-4 py-3 text-sm text-text-primary">
                <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-status-warning" aria-hidden />
                <span>Authenticate Claude Code again as the backend OS account, then check again. Schedules stay saved and leasing is paused until then.</span>
              </p>
            )}
          </div>
        )}
        {canManage
          ? <div className="flex flex-wrap items-center gap-3">
              <Button size="sm" variant="secondary" leftIcon={<RefreshCw className="h-3.5 w-3.5" />} loading={check.isPending} onClick={() => check.mutate()}>Check again</Button>
              {check.isError && <span role="alert" className="text-[12px] text-status-error">{errorMessage(check.error, 'The check failed.')} Try again.</span>}
            </div>
          : <PermissionNote permission="autonomous_agent:enable" action="re-check the runtime" />}
      </Section>

      <Section id="pause-title" title="Pause all agents" tone="danger" description="Stops every agent in this organization at once. Runs in progress are cancelled and nothing new starts until you resume.">
        {settings.isLoading ? <Skeleton className="h-6 w-40" /> : settings.isError ? (
          <InlineError message="Could not load the fleet state." onRetry={() => void settings.refetch()} />
        ) : (
          <div className="flex flex-wrap items-center gap-3">
            <Badge size="md" variant={settings.data?.enabled ? 'success' : 'warning'} dot>{settings.data?.enabled ? 'Agents are running' : 'All agents are paused'}</Badge>
            {canManage
              ? <PauseAllControl enabled={Boolean(settings.data?.enabled)} pending={toggle.isPending} onToggle={value => toggle.mutate(value)} error={toggle.error ?? undefined} />
              : <PermissionNote permission="autonomous_agent:enable" action="pause or resume agents" />}
          </div>
        )}
      </Section>

      <Section id="retention-title" title="Retention" description={`How long runs, conversations and findings are kept before they are deleted, from ${RETENTION_MIN} to ${RETENTION_MAX} days.`}>
        {settings.isLoading ? <Skeleton className="h-9 w-40" /> : settings.isSuccess && (
          canManage ? (
            <form className="space-y-2" onSubmit={event => { event.preventDefault(); if (retentionValid && retentionDirty) saveRetention.mutate(days) }}>
              <label htmlFor="retention-days" className="block text-[12px] font-medium text-text-secondary">Keep history for (days)</label>
              <div className="flex flex-wrap items-center gap-3">
                <div className="w-32">
                  <Input
                    id="retention-days"
                    inputSize="md"
                    type="number"
                    min={RETENTION_MIN}
                    max={RETENTION_MAX}
                    value={retention}
                    aria-invalid={!retentionValid || undefined}
                    aria-describedby="retention-help"
                    onChange={event => { setRetention(event.target.value); saveRetention.reset() }}
                  />
                </div>
                <Button type="submit" size="sm" variant="primary" loading={saveRetention.isPending} disabled={!retentionValid || !retentionDirty}>Save</Button>
                {saveRetention.isSuccess && !retentionDirty && <span role="status" className="text-[12px] text-text-secondary">Saved.</span>}
              </div>
              <p id="retention-help" className={`text-[12px] ${retentionValid ? 'text-text-tertiary' : 'text-status-error'}`}>
                {retentionValid ? 'Older history is removed automatically.' : `Enter a whole number from ${RETENTION_MIN} to ${RETENTION_MAX}.`}
              </p>
              {saveRetention.isError && <p role="alert" className="text-[12px] text-status-error">{errorMessage(saveRetention.error, 'Could not save retention.')} Try again.</p>}
            </form>
          ) : (
            <div className="space-y-2">
              <p className="text-sm text-text-primary">History is kept for {settings.data.retention_days} days.</p>
              <PermissionNote permission="autonomous_agent:enable" action="change retention" />
            </div>
          )
        )}
      </Section>

      <SandboxBotPanel client={client} canWrite={can('factory_policy:write')} />
    </div>
  )
}
