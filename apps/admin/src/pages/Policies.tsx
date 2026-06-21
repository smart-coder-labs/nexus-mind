import { useMemo, useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Shield, Plus, Trash2, ToggleLeft, ToggleRight } from 'lucide-react'
import { createClient } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import type { CreatePolicyRequest, Policy } from '../types'
import {
  Modal,
  ModalCloseButton,
  ModalHeader,
  ModalTitle,
  ModalDescription,
  ModalContent,
  ModalFooter,
} from '../components/ui/Modal/Modal'

const RULE_TYPE_LABELS: Record<Policy['rule_type'], string> = {
  model_whitelist: 'Model Whitelist',
  budget_limit:    'Budget Limit',
  pii_redact:      'PII Redact',
}

const CONFIG_HINTS: Record<Policy['rule_type'], string> = {
  model_whitelist: '{ "allowed_models": ["gpt-4o", "claude-3-5-sonnet"] }',
  budget_limit:    '{ "limit_usd": 100, "period": "month" }',
  pii_redact:      '{ "patterns": ["email", "phone", "ssn"] }',
}

const RULE_TYPES: Policy['rule_type'][] = ['model_whitelist', 'budget_limit', 'pii_redact']

export default function Policies() {
  const { session } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  const [modalOpen, setModalOpen] = useState(false)
  const [formName, setFormName] = useState('')
  const [formRuleType, setFormRuleType] = useState<Policy['rule_type']>('model_whitelist')
  const [formConfig, setFormConfig] = useState('')
  const [formEnabled, setFormEnabled] = useState(true)
  const [formError, setFormError] = useState('')

  const { data, isLoading } = useQuery({
    queryKey: ['policies'],
    queryFn: () => client.listPolicies(),
  })

  const policies = data?.policies ?? []

  const createMut = useMutation({
    mutationFn: (req: CreatePolicyRequest) => client.createPolicy(req),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['policies'] })
      setModalOpen(false)
      resetForm()
    },
    onError: (err: Error) => {
      setFormError(err.message || 'Failed to create policy')
    },
  })

  const toggleMut = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      client.updatePolicy(id, { enabled }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['policies'] })
    },
  })

  const deleteMut = useMutation({
    mutationFn: (id: string) => client.deletePolicy(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['policies'] })
    },
  })

  function resetForm() {
    setFormName('')
    setFormRuleType('model_whitelist')
    setFormConfig('')
    setFormEnabled(true)
    setFormError('')
  }

  function handleOpenModal() {
    resetForm()
    setModalOpen(true)
  }

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setFormError('')

    let parsedConfig: Record<string, unknown> = {}
    if (formConfig.trim()) {
      try {
        parsedConfig = JSON.parse(formConfig)
      } catch {
        setFormError('Config must be valid JSON')
        return
      }
    }

    if (!formName.trim()) {
      setFormError('Name is required')
      return
    }

    createMut.mutate({
      name: formName.trim(),
      rule_type: formRuleType,
      config: parsedConfig,
      enabled: formEnabled,
    })
  }

  return (
    <div className="px-6 pt-6 pb-4">
      {/* Page header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-base font-semibold text-text-primary">Policies</h1>
          <p className="text-xs text-text-tertiary mt-0.5">
            Manage organization-wide rules for model access, budget limits, and data handling.
          </p>
        </div>
        <button
          onClick={handleOpenModal}
          className="flex items-center gap-2 bg-accent-blue hover:bg-accent-blue-hover text-white text-xs font-semibold px-4 py-1.5 rounded-full transition-colors"
        >
          <Plus className="w-4 h-4" />
          Add policy
        </button>
      </div>

      {/* Policy grid */}
      {isLoading ? (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <div
              key={i}
              className="bg-[#272729] rounded-[18px] border border-border-primary p-5 space-y-3 animate-pulse"
            >
              <div className="flex items-center justify-between">
                <div className="h-4 w-1/3 bg-[#1d1d1f] rounded-[5px]" />
                <div className="h-4 w-16 bg-[#1d1d1f] rounded-full" />
              </div>
              <div className="h-3 w-1/4 bg-[#1d1d1f] rounded-[5px]" />
              <div className="h-3 w-1/2 bg-[#1d1d1f] rounded-[5px]" />
            </div>
          ))}
        </div>
      ) : policies.length === 0 ? (
        <div className="bg-[#272729] rounded-[18px] border border-border-primary p-12 flex flex-col items-center gap-3 text-center">
          <Shield className="w-8 h-8 text-text-quaternary/50" />
          <p className="text-xs font-semibold text-text-secondary">No policies yet</p>
          <p className="text-xs text-text-quaternary max-w-xs">
            Create a policy to enforce model restrictions, spending limits, or PII handling rules across your organization.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {policies.map(policy => (
            <div
              key={policy.id}
              className="group bg-[#272729] rounded-[18px] border border-border-primary p-5 flex flex-col gap-3"
            >
              {/* Top row: name + badge */}
              <div className="flex items-center justify-between gap-2">
                <span className="font-semibold text-text-primary text-xs truncate">{policy.name}</span>
                {policy.enabled ? (
                  <span className="text-[10px] px-1.5 py-0.5 rounded-[5px] bg-status-success/10 text-status-success shrink-0">
                    Active
                  </span>
                ) : (
                  <span className="text-[10px] px-1.5 py-0.5 rounded-[5px] bg-white/[0.06] text-text-quaternary shrink-0">
                    Disabled
                  </span>
                )}
              </div>

              {/* Rule type */}
              <span className="bg-white/[0.06] rounded-[5px] text-[10px] text-text-secondary px-1.5 py-0.5 self-start">
                {RULE_TYPE_LABELS[policy.rule_type]}
              </span>

              {/* Bottom row: toggle + delete */}
              <div className="flex items-center justify-between mt-auto pt-1">
                <button
                  onClick={() => toggleMut.mutate({ id: policy.id, enabled: !policy.enabled })}
                  disabled={toggleMut.isPending}
                  className="flex items-center gap-1.5 bg-white/[0.06] hover:bg-white/[0.10] text-text-primary text-xs px-3 py-1.5 rounded-full transition-colors disabled:opacity-40"
                  aria-label={policy.enabled ? 'Disable policy' : 'Enable policy'}
                >
                  {policy.enabled ? (
                    <ToggleRight className="w-4 h-4 text-status-success" />
                  ) : (
                    <ToggleLeft className="w-4 h-4 text-text-quaternary" />
                  )}
                  {policy.enabled ? 'Enabled' : 'Disabled'}
                </button>

                <button
                  onClick={() => {
                    if (confirm(`Delete policy "${policy.name}"? This cannot be undone.`)) {
                      deleteMut.mutate(policy.id)
                    }
                  }}
                  disabled={deleteMut.isPending}
                  className="p-1.5 rounded-[8px] opacity-0 group-hover:opacity-100 text-text-quaternary hover:text-status-error transition-opacity disabled:opacity-40"
                  aria-label={`Delete policy ${policy.name}`}
                >
                  <Trash2 className="w-3.5 h-3.5" />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Add Policy Modal */}
      <Modal open={modalOpen} onOpenChange={setModalOpen} size="md">
        <ModalCloseButton />
        <ModalHeader>
          <ModalTitle>Add Policy</ModalTitle>
          <ModalDescription>
            Define a new organization-wide rule.
          </ModalDescription>
        </ModalHeader>

        <ModalContent>
          <form id="policy-form" onSubmit={handleSubmit} className="space-y-4 text-xs">
            {formError && (
              <div className="p-3 text-xs bg-status-error/10 border border-status-error/20 text-status-error rounded-[11px]">
                {formError}
              </div>
            )}

            <div className="space-y-1">
              <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">
                Name
              </label>
              <input
                type="text"
                placeholder="e.g. Production Model Whitelist"
                value={formName}
                onChange={e => setFormName(e.target.value)}
                className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-2 py-1.5 focus:outline-none focus:border-accent-blue/60 transition-colors"
                required
              />
            </div>

            <div className="space-y-1">
              <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">
                Rule Type
              </label>
              <select
                value={formRuleType}
                onChange={e => {
                  setFormRuleType(e.target.value as Policy['rule_type'])
                  setFormConfig('')
                }}
                className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-2 py-1.5 focus:outline-none focus:border-accent-blue/60 transition-colors"
              >
                {RULE_TYPES.map(rt => (
                  <option key={rt} value={rt}>{RULE_TYPE_LABELS[rt]}</option>
                ))}
              </select>
            </div>

            <div className="space-y-1">
              <label className="text-[10px] font-semibold text-text-tertiary tracking-[-0.08px]">
                Config (JSON)
              </label>
              <textarea
                value={formConfig}
                onChange={e => setFormConfig(e.target.value)}
                placeholder={CONFIG_HINTS[formRuleType]}
                className="w-full rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-2 py-1.5 focus:outline-none focus:border-accent-blue/60 transition-colors min-h-[80px] resize-y font-mono"
              />
              <p className="text-[10px] text-text-quaternary">
                Example: <code className="text-text-tertiary">{CONFIG_HINTS[formRuleType]}</code>
              </p>
            </div>

            <div className="flex items-center gap-3">
              <label className="relative inline-flex items-center cursor-pointer">
                <input
                  type="checkbox"
                  checked={formEnabled}
                  onChange={e => setFormEnabled(e.target.checked)}
                  className="sr-only peer"
                />
                <div className="w-8 h-4 bg-white/[0.10] peer-checked:bg-accent-blue rounded-full transition-colors after:content-[''] after:absolute after:top-0.5 after:left-0.5 after:bg-white after:rounded-full after:h-3 after:w-3 after:transition-transform peer-checked:after:translate-x-4" />
              </label>
              <span className="text-xs text-text-secondary">Enable immediately</span>
            </div>
          </form>
        </ModalContent>

        <ModalFooter>
          <button
            type="button"
            onClick={() => setModalOpen(false)}
            className="bg-white/[0.06] hover:bg-white/[0.10] text-text-primary text-xs px-4 py-2 rounded-full transition-colors"
          >
            Cancel
          </button>
          <button
            type="submit"
            form="policy-form"
            disabled={createMut.isPending}
            className="flex items-center gap-2 bg-accent-blue hover:bg-accent-blue-hover text-white text-xs font-semibold px-4 py-2 rounded-full transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <Plus className="w-3.5 h-3.5" />
            {createMut.isPending ? 'Creating…' : 'Create Policy'}
          </button>
        </ModalFooter>
      </Modal>
    </div>
  )
}
