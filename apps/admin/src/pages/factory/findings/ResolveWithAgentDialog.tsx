import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { Button } from '../../../components/ui/Button'
import { Modal, ModalContent, ModalFooter, ModalHeader, ModalTitle } from '../../../components/ui/Modal'
import type { AutonomousAgentDefinition } from '../../../types'
import { SELECT_CLASS } from '../agents/RunDialogs'
import { TEXT_LINK } from '../shared/ui'

/**
 * "Resolve with agent": pick ONE enabled issue-resolver to fix a single finding.
 * The finding content (and the GitHub issue it was filed as, when present) are
 * handed to that resolver, which resolves ONLY this — nothing else in the repo.
 */
export function ResolveWithAgentDialog({ target, resolvers, pending, error, onClose, onRun }: { target: { title: string; issue?: { repository: string; number: number } }; resolvers: AutonomousAgentDefinition[]; pending: boolean; error?: string; onClose: () => void; onRun: (agentId: string) => void }) {
  const [agentId, setAgentId] = useState('')
  useEffect(() => { if (!agentId && resolvers.length) setAgentId(resolvers[0].id) }, [resolvers, agentId])
  return (
    <Modal open onOpenChange={value => { if (!value) onClose() }} size="md">
      <ModalHeader>
        <ModalTitle>Resolve with agent</ModalTitle>
      </ModalHeader>
      <ModalContent className="space-y-4">
        <p className="text-[13px] text-text-secondary">Hand this finding to an issue resolver. It fixes <span className="text-text-primary">only</span> this, nothing else in the repository.</p>
        <div className="rounded-[11px] border border-border-primary bg-white/[0.02] px-3 py-2 text-[13px] text-text-primary">{target.title}</div>
        {target.issue
          ? <p className="text-[12px] text-text-tertiary">Linked issue <span className="font-mono text-text-secondary">{target.issue.repository}#{target.issue.number}</span>. The pull request will close it.</p>
          : <p className="text-[12px] text-text-tertiary">No GitHub issue is linked, so the finding itself becomes the task.</p>}
        {resolvers.length === 0
          ? <p className="text-[13px] text-status-warning">No issue resolver is enabled. <Link to="/factory" className={TEXT_LINK}>Create or enable one in Agents</Link>, then come back.</p>
          : (
            <label className="block">
              <span className="text-[12px] font-medium text-text-secondary">Resolver agent</span>
              <select value={agentId} onChange={e => setAgentId(e.target.value)} className={SELECT_CLASS}>
                {resolvers.map(r => <option key={r.id} value={r.id}>{r.name}</option>)}
              </select>
            </label>
          )}
        {error && <p role="alert" className="text-[12px] text-status-error">{error}</p>}
        <ModalFooter>
          <Button size="sm" variant="ghost" onClick={onClose}>Cancel</Button>
          <Button size="sm" variant="primary" loading={pending} disabled={!agentId || resolvers.length === 0} onClick={() => agentId && onRun(agentId)}>Start resolver</Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}
