import { useEffect, useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { X } from 'lucide-react'
import { createClient } from '../../../api/client'
import { Badge } from '../../../components/ui/Badge'
import { Button } from '../../../components/ui/Button'
import { Input } from '../../../components/ui/Input'
import { Modal, ModalContent, ModalFooter, ModalHeader, ModalTitle } from '../../../components/ui/Modal'
import type { AutonomousAgentDefinition } from '../../../types'

export type RunTarget = { repository: string; type: 'pr' | 'issue'; number: number }

/** Native select styled as the kit's input (36px, 11px radius, focus ring). */
export const SELECT_CLASS = 'mt-1 block h-9 w-full rounded-[11px] border border-border-primary bg-white/[0.04] px-3 text-[13px] text-text-primary focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring'

/**
 * Run dialog for the PR reviewer: the PR to review is chosen per run. Repository
 * defaults to the agent's configured `repository` (editable for multi-repo setups).
 */
export function ReviewerRunDialog({ agent, pending, onClose, onRun }: { agent: AutonomousAgentDefinition; pending: boolean; onClose: () => void; onRun: (target: { repository: string; type: 'pr'; number: number }) => void }) {
  const client = useMemo(() => createClient(), [])
  const detail = useQuery({ queryKey: ['autonomous-agent-detail', agent.id], queryFn: () => client.getAutonomousAgent(agent.id) })
  const configRepo = (typeof detail.data?.revision?.config?.repository === 'string' ? detail.data?.revision?.config?.repository : '') as string
  const [repository, setRepository] = useState('')
  const [number, setNumber] = useState('')
  useEffect(() => { if (!repository && configRepo) setRepository(configRepo) }, [configRepo, repository])
  const parsed = Number(String(number).replace('#', '').trim())
  const valid = repository.trim().length > 0 && Number.isInteger(parsed) && parsed > 0
  return (
    <Modal open onOpenChange={value => { if (!value) onClose() }} size="md">
      <ModalHeader>
        <ModalTitle>Run “{agent.name}”</ModalTitle>
      </ModalHeader>
      <ModalContent className="space-y-4">
        <p className="text-[13px] text-text-secondary">Choose the pull request to review this run. Only this PR is reviewed; nothing is merged.</p>
        <label className="block">
          <span className="text-[12px] font-medium text-text-secondary">Repository</span>
          <Input inputSize="sm" className="mt-1 w-full" value={repository} onChange={e => setRepository(e.target.value)} placeholder="owner/repo" />
        </label>
        <label className="block">
          <span className="text-[12px] font-medium text-text-secondary">Pull request number</span>
          <Input inputSize="sm" className="mt-1 w-40" value={number} onChange={e => setNumber(e.target.value)} onKeyDown={e => { if (e.key === 'Enter' && valid) { e.preventDefault(); onRun({ repository: repository.trim(), type: 'pr', number: parsed }) } }} placeholder="123" />
        </label>
        <ModalFooter>
          <Button size="sm" variant="ghost" onClick={onClose}>Cancel</Button>
          <Button size="sm" variant="primary" loading={pending} disabled={!valid} onClick={() => onRun({ repository: repository.trim(), type: 'pr', number: parsed })}>Review PR</Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}

/**
 * Run dialog for the Judge template: the PR/issue targets are chosen per run (not
 * baked into the agent), each scoped to one of the agent's configured repositories.
 */
export function JudgeRunDialog({ agent, pending, onClose, onRun }: { agent: AutonomousAgentDefinition; pending: boolean; onClose: () => void; onRun: (targets: RunTarget[]) => void }) {
  const client = useMemo(() => createClient(), [])
  const detail = useQuery({ queryKey: ['autonomous-agent-detail', agent.id], queryFn: () => client.getAutonomousAgent(agent.id) })
  const repos = (Array.isArray(detail.data?.revision?.config?.repositories) ? detail.data?.revision?.config?.repositories : []) as string[]
  const [repository, setRepository] = useState('')
  const [type, setType] = useState<'pr' | 'issue'>('pr')
  const [number, setNumber] = useState('')
  const [targets, setTargets] = useState<RunTarget[]>([])
  useEffect(() => { if (!repository && repos.length) setRepository(repos[0]) }, [repos, repository])

  const add = () => {
    const n = Number(String(number).replace('#', '').trim())
    if (!repository || !Number.isInteger(n) || n <= 0) return
    if (targets.some(t => t.repository === repository && t.type === type && t.number === n)) { setNumber(''); return }
    setTargets(prev => [...prev, { repository, type, number: n }])
    setNumber('')
  }

  return (
    <Modal open onOpenChange={value => { if (!value) onClose() }} size="md">
      <ModalHeader>
        <ModalTitle>Run “{agent.name}”</ModalTitle>
      </ModalHeader>
      <ModalContent className="space-y-4">
        <p className="text-[13px] text-text-secondary">Choose the pull requests or issues to judge this run. Each is verified against the live app, scoped to what it touches.</p>
        {repos.length === 0 && !detail.isLoading && <p className="text-[12px] text-status-warning">This agent has no configured repositories. Edit it to add some first.</p>}
        <div className="grid grid-cols-1 items-end gap-2 sm:grid-cols-[1fr_auto_auto_auto]">
          <label className="block">
            <span className="text-[12px] font-medium text-text-secondary">Repository</span>
            <select value={repository} onChange={e => setRepository(e.target.value)} className={SELECT_CLASS}>
              {repos.map(r => <option key={r} value={r}>{r}</option>)}
            </select>
          </label>
          <label className="block">
            <span className="text-[12px] font-medium text-text-secondary">Type</span>
            <select value={type} onChange={e => setType(e.target.value as 'pr' | 'issue')} className={SELECT_CLASS}>
              <option value="pr">Pull request</option>
              <option value="issue">Issue</option>
            </select>
          </label>
          <label className="block">
            <span className="text-[12px] font-medium text-text-secondary">Number</span>
            <Input inputSize="sm" className="mt-1 w-24" value={number} onChange={e => setNumber(e.target.value)} onKeyDown={e => { if (e.key === 'Enter') { e.preventDefault(); add() } }} placeholder="123" />
          </label>
          <Button size="sm" variant="secondary" onClick={add} disabled={!repository || !number.trim()}>Add</Button>
        </div>
        {targets.length > 0 && (
          <ul className="flex list-none flex-wrap gap-1.5 p-0" aria-label="Targets to judge">
            {targets.map((t, i) => (
              <li key={`${t.repository}-${t.type}-${t.number}`}>
                <Badge size="sm" variant="default">
                  <span className="font-mono">{t.repository}#{t.number}</span>
                  <span>{t.type === 'pr' ? 'PR' : 'Issue'}</span>
                  <button type="button" className="ml-0.5 rounded-full text-text-tertiary hover:text-text-primary focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring" onClick={() => setTargets(prev => prev.filter((_, idx) => idx !== i))} aria-label={`Remove ${t.repository}#${t.number}`}>
                    <X className="h-3 w-3" aria-hidden />
                  </button>
                </Badge>
              </li>
            ))}
          </ul>
        )}
        <ModalFooter>
          <Button size="sm" variant="ghost" onClick={onClose}>Cancel</Button>
          <Button size="sm" variant="primary" loading={pending} disabled={targets.length === 0} onClick={() => onRun(targets)}>Run judge</Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}
