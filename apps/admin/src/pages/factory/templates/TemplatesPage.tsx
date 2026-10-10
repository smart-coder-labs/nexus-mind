import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { LayoutTemplate, Plus } from 'lucide-react'
import { Badge } from '../../../components/ui/Badge'
import { Button } from '../../../components/ui/Button'
import { EmptyState } from '../../../components/ui/EmptyState'
import { Modal, ModalContent, ModalFooter, ModalHeader, ModalTitle } from '../../../components/ui/Modal'
import { Skeleton } from '../../../components/ui/Skeleton'
import AutonomousAgentWizard from '../../AutonomousAgentWizard'
import type { AutonomousAgentTemplate } from '../../../types'
import { dur, humanize, stepName, usd } from '../shared/format'
import { FieldLabel, InlineError, PageHeader, RawJson, TEXT_LINK, useFactory } from '../shared/ui'

const BUDGET_LABELS: Record<string, { label: string; format?: (n: number) => string }> = {
  max_cost_usd: { label: 'Cost limit per run', format: usd },
  wall_time_seconds: { label: 'Time limit per run', format: dur },
  max_attempts: { label: 'Attempts' },
  max_changed_files: { label: 'Files it may change' },
  max_changed_lines: { label: 'Lines it may change' },
  max_definition_concurrency: { label: 'Runs at once, per agent' },
  max_repository_concurrency: { label: 'Runs at once, per repository' },
  max_organization_concurrency: { label: 'Runs at once, per organization' },
  requests_per_second: { label: 'Requests per second' },
}

/** Budgets as a definition list when every value is a plain number/string/boolean. */
function BudgetList({ budgets }: { budgets: Record<string, unknown> }) {
  const entries = Object.entries(budgets ?? {})
  const simple = entries.every(([, v]) => ['number', 'string', 'boolean'].includes(typeof v))
  if (!entries.length) return <p className="text-sm text-text-tertiary">No default budgets.</p>
  if (!simple) return <pre className="overflow-auto rounded-md bg-muted p-3 font-mono text-[11px] text-text-secondary">{JSON.stringify(budgets, null, 2)}</pre>
  return (
    <dl className="grid grid-cols-[1fr_auto] gap-x-6 gap-y-1.5 text-sm">
      {entries.map(([key, value]) => {
        const meta = BUDGET_LABELS[key]
        const shown = typeof value === 'number' && meta?.format ? meta.format(value) : typeof value === 'boolean' ? (value ? 'Yes' : 'No') : String(value)
        return (
          <div key={key} className="contents">
            <dt className="text-text-tertiary">{meta?.label ?? humanize(key)}</dt>
            <dd className="text-right text-text-primary tabular-nums">{shown}</dd>
          </div>
        )
      })}
    </dl>
  )
}

/** /factory/templates — managed templates catalog. */
export default function TemplatesPage() {
  const { can, client } = useFactory()
  const [detail, setDetail] = useState<AutonomousAgentTemplate | null>(null)
  const [creating, setCreating] = useState(false)
  const templates = useQuery({ queryKey: ['autonomous-templates'], queryFn: () => client.listAutonomousAgentTemplates() })

  return (
    <div className="mx-auto min-w-0 max-w-7xl space-y-6 p-6 md:p-8">
      <PageHeader
        title="Templates"
        subtitle="Managed templates every agent starts from. They are versioned on the server and cannot be edited here."
        actions={<>
          <Link to="/factory" className={`px-2 ${TEXT_LINK}`}>Back to agents</Link>
          {can('autonomous_agent:create') && <Button size="sm" variant="primary" leftIcon={<Plus className="h-4 w-4" />} onClick={() => setCreating(true)}>New agent</Button>}
        </>}
      />

      {templates.isLoading && (
        <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,260px),1fr))] gap-4" aria-busy="true" aria-label="Loading templates">
          {[0, 1, 2].map(i => <div key={i} className="space-y-3 rounded-xl border border-border-primary p-5"><Skeleton className="h-4 w-32" /><Skeleton className="h-3 w-full" /><Skeleton className="h-3 w-2/3" /></div>)}
        </div>
      )}
      {templates.isError && <InlineError message="Could not load templates." onRetry={() => void templates.refetch()} />}
      {templates.isSuccess && templates.data.length === 0 && <EmptyState icon={<LayoutTemplate className="h-6 w-6" />} title="No templates available" description="This server ships no managed templates. Check the backend version." />}

      {templates.isSuccess && templates.data.length > 0 && (
        <ul className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,260px),1fr))] list-none gap-4 p-0">
          {templates.data.map(item => (
            <li key={item.key} className="min-w-0">
              <button type="button" onClick={() => setDetail(item)} className="flex h-full w-full flex-col gap-3 rounded-xl border border-border-primary bg-card p-4 text-left transition-colors hover:bg-muted/50 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring">
                <span className="flex w-full flex-wrap items-start justify-between gap-2">
                  <span className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary">{item.name}</span>
                  <Badge size="sm" variant="default">Version {item.version}</Badge>
                </span>
                <span className="text-sm leading-relaxed text-text-secondary">{item.description}</span>
                {item.workflow?.length > 0 && (
                  <span className="text-[12px] text-text-tertiary">{item.workflow.length} steps, from {stepName(item.workflow[0]).toLowerCase()} to {stepName(item.workflow[item.workflow.length - 1]).toLowerCase()}</span>
                )}
                <span className="mt-auto flex flex-wrap gap-1">
                  {item.capabilities.slice(0, 3).map(cap => <Badge key={cap} size="sm" variant="default" className="max-w-full whitespace-normal break-all font-mono">{cap}</Badge>)}
                  {item.capabilities.length > 3 && <Badge size="sm" variant="default">+{item.capabilities.length - 3} more</Badge>}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}

      <Modal open={Boolean(detail)} onOpenChange={value => { if (!value) setDetail(null) }} size="lg">
        {detail && (
          <>
            <ModalHeader>
              <ModalTitle>{detail.name}</ModalTitle>
              <p className="mt-1 text-[12px] text-text-tertiary">Version {detail.version}</p>
            </ModalHeader>
            <ModalContent className="max-h-[65vh] space-y-5 overflow-y-auto">
              <p className="text-sm leading-relaxed text-text-secondary">{detail.description}</p>
              <div className="space-y-2">
                <FieldLabel>What it does, step by step</FieldLabel>
                <ol className="list-none space-y-1.5 p-0">
                  {detail.workflow.map((step, index) => (
                    <li key={`${step}-${index}`} className="flex items-center gap-2 text-sm text-text-primary">
                      <span className="grid h-5 w-5 place-items-center rounded-full bg-foreground/[0.06] text-[11px] text-text-tertiary tabular-nums">{index + 1}</span>{stepName(step)}
                    </li>
                  ))}
                </ol>
              </div>
              <div className="space-y-2">
                <FieldLabel>Capabilities it is granted</FieldLabel>
                <div className="flex flex-wrap gap-1">{detail.capabilities.map(cap => <Badge key={cap} size="sm" variant="default" className="max-w-full whitespace-normal break-all font-mono">{cap}</Badge>)}</div>
              </div>
              <div className="space-y-2">
                <FieldLabel>Default budgets</FieldLabel>
                <BudgetList budgets={detail.default_budgets} />
              </div>
              <RawJson label="Configuration schema (JSON)" value={detail.config_schema} />
              <p className="text-[12px] text-text-tertiary">Upgrading a template creates a new agent revision that must be validated again.</p>
              {can('autonomous_agent:create') && (
                <ModalFooter>
                  <Button size="sm" variant="ghost" onClick={() => setDetail(null)}>Close</Button>
                  <Button size="sm" variant="primary" leftIcon={<Plus className="h-4 w-4" />} onClick={() => { setDetail(null); setCreating(true) }}>New agent</Button>
                </ModalFooter>
              )}
            </ModalContent>
          </>
        )}
      </Modal>

      {creating && <AutonomousAgentWizard open={creating} templates={templates.data ?? []} onClose={() => setCreating(false)} />}
    </div>
  )
}
