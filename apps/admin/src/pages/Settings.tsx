import { useMemo, useState, useEffect } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { Switch } from '../components/ui/Switch/Switch'
import { Check, X, ChevronDown, ChevronUp, Download, Upload, Plus, Pencil, Trash2, AlertCircle } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { AgentEventSettings, OrgSettings, Webhook, CreateWebhookRequest, WebhookTestResult, ImportConfigResponse } from '../types'

// ── Memory Templates ──────────────────────────────────────────────────────────

const TEMPLATE_TYPES = ['auto', 'manual', 'summary', 'reflection'] as const
type TemplateType = typeof TEMPLATE_TYPES[number]

interface MemoryTemplate {
  id: string
  name: string
  type: TemplateType
  content: string
}

const TEMPLATES_KEY = 'nexusmind-memory-templates'

function loadTemplates(): MemoryTemplate[] {
  try {
    const raw = localStorage.getItem(TEMPLATES_KEY)
    return raw ? JSON.parse(raw) : []
  } catch {
    return []
  }
}

function saveTemplates(templates: MemoryTemplate[]) {
  localStorage.setItem(TEMPLATES_KEY, JSON.stringify(templates))
}

const TYPE_BADGE_CLS: Record<TemplateType, string> = {
  auto:       'bg-accent-blue/10 text-accent-blue border-accent-blue/25',
  manual:     'bg-white/[0.06] text-text-secondary border-border-secondary/60',
  summary:    'bg-status-success/10 text-status-success border-status-success/25',
  reflection: 'bg-status-warning/10 text-status-warning border-status-warning/25',
}

function MemoryTemplatesSection() {
  const [templates, setTemplates] = useState<MemoryTemplate[]>(() => loadTemplates())
  const [showForm, setShowForm] = useState(false)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [formName, setFormName] = useState('')
  const [formType, setFormType] = useState<TemplateType>('auto')
  const [formContent, setFormContent] = useState('')

  const persist = (next: MemoryTemplate[]) => {
    saveTemplates(next)
    setTemplates(next)
  }

  const openCreate = () => {
    setEditingId(null)
    setFormName('')
    setFormType('auto')
    setFormContent('')
    setShowForm(true)
  }

  const openEdit = (t: MemoryTemplate) => {
    setEditingId(t.id)
    setFormName(t.name)
    setFormType(t.type)
    setFormContent(t.content)
    setShowForm(true)
  }

  const handleSave = () => {
    if (!formName.trim() || !formContent.trim()) return
    if (editingId) {
      persist(templates.map(t => t.id === editingId ? { ...t, name: formName.trim(), type: formType, content: formContent.trim() } : t))
    } else {
      const newTemplate: MemoryTemplate = {
        id: crypto.randomUUID(),
        name: formName.trim(),
        type: formType,
        content: formContent.trim(),
      }
      persist([...templates, newTemplate])
    }
    setShowForm(false)
  }

  const handleDelete = (id: string) => {
    persist(templates.filter(t => t.id !== id))
  }

  const handleCancel = () => {
    setShowForm(false)
    setEditingId(null)
  }

  const inputCls = 'w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 transition-colors'

  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between">
        <p className="text-text-tertiary text-[12px] tracking-[-0.12px]">Memory Templates</p>
        {!showForm && (
          <button
            onClick={openCreate}
            className="flex items-center gap-1.5 text-xs border border-border-primary rounded-full px-3 py-1.5 text-text-secondary hover:text-text-primary hover:bg-[#272729] transition-colors"
          >
            <Plus className="w-3 h-3" />
            Add template
          </button>
        )}
      </div>
      <div className="border border-border-primary rounded-[18px] p-5 space-y-4">
        <p className="text-xs text-text-tertiary">
          Define reusable templates for creating memories. Templates pre-fill content when users create new memories.
        </p>

        {/* Template list */}
        {templates.length > 0 && !showForm && (
          <div className="space-y-2">
            {templates.map(t => (
              <div key={t.id} className="flex items-start gap-3 p-3 rounded-[11px] border border-border-secondary bg-[#272729]">
                <div className="flex-1 min-w-0 space-y-1">
                  <div className="flex items-center gap-2 flex-wrap">
                    <span className="text-sm font-semibold text-text-primary">{t.name}</span>
                    <span className={`text-[10px] font-semibold border rounded-[5px] px-1.5 py-0.5 ${TYPE_BADGE_CLS[t.type]}`}>
                      {t.type}
                    </span>
                  </div>
                  <p className="text-xs text-text-tertiary truncate">{t.content}</p>
                </div>
                <div className="flex items-center gap-1 shrink-0">
                  <button
                    onClick={() => openEdit(t)}
                    aria-label={`Edit template ${t.name}`}
                    className="p-1.5 rounded-[8px] text-text-quaternary hover:text-text-secondary hover:bg-[#272729] transition-colors"
                  >
                    <Pencil className="w-3.5 h-3.5" />
                  </button>
                  <button
                    onClick={() => handleDelete(t.id)}
                    aria-label={`Delete template ${t.name}`}
                    className="p-1.5 rounded-[8px] text-text-quaternary hover:text-status-error hover:bg-[#272729] transition-colors"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}

        {templates.length === 0 && !showForm && (
          <div className="text-center py-6 space-y-1">
            <p className="text-sm font-semibold text-text-secondary">No templates yet</p>
            <p className="text-xs text-text-tertiary">Add a template to speed up memory creation.</p>
          </div>
        )}

        {/* Create / Edit form */}
        {showForm && (
          <div className="space-y-3">
            <p className="text-xs font-semibold text-text-secondary">
              {editingId ? 'Edit template' : 'New template'}
            </p>
            <div className="space-y-1.5">
              <label className="text-xs text-text-tertiary">Name</label>
              <input
                value={formName}
                onChange={e => setFormName(e.target.value)}
                placeholder="e.g. Bug report"
                className={inputCls}
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-xs text-text-tertiary">Type</label>
              <select
                value={formType}
                onChange={e => setFormType(e.target.value as TemplateType)}
                className="bg-transparent border border-border-primary rounded-[11px] px-3 py-2.5 text-sm text-text-primary focus:outline-none focus:border-accent-blue/60 transition-colors appearance-none w-full"
              >
                {TEMPLATE_TYPES.map(t => (
                  <option key={t} value={t}>{t}</option>
                ))}
              </select>
            </div>
            <div className="space-y-1.5">
              <label className="text-xs text-text-tertiary">Content</label>
              <textarea
                value={formContent}
                onChange={e => setFormContent(e.target.value)}
                placeholder="Template content that will pre-fill the memory..."
                rows={5}
                className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 transition-colors resize-y min-h-[100px]"
              />
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={handleCancel}
                className="text-xs text-text-secondary hover:text-text-primary transition-colors"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleSave}
                disabled={!formName.trim() || !formContent.trim()}
                className="rounded-[8px] bg-accent-blue text-white px-3 py-1.5 text-xs font-semibold hover:opacity-90 disabled:opacity-50 transition-opacity"
              >
                {editingId ? 'Save changes' : 'Add template'}
              </button>
            </div>
          </div>
        )}
      </div>
    </section>
  )
}

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

// ── Webhook delivery panel ────────────────────────────────────────────────────

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime()
  const m = Math.floor(diff / 60000)
  if (m < 1) return 'just now'
  if (m < 60) return `${m}m ago`
  const h = Math.floor(m / 60)
  if (h < 24) return `${h}h ago`
  return `${Math.floor(h / 24)}d ago`
}

function WebhookDeliveryPanel({ webhookId, client }: { webhookId: string; client: ReturnType<typeof createClient> }) {
  const qc = useQueryClient()
  const { data, isLoading } = useQuery({
    queryKey: ['webhook-deliveries', webhookId],
    queryFn: () => client.listWebhookDeliveries(webhookId, 20),
  })
  const deliveries = data?.deliveries ?? []

  const retryMut = useMutation({
    mutationFn: (deliveryId: string) => client.retryWebhookDelivery(deliveryId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['webhook-deliveries', webhookId] }),
  })

  if (isLoading) {
    return (
      <div className="space-y-1.5 mt-2">
        {[0, 1, 2].map(i => (
          <div key={i} className="h-5 bg-[#272729] animate-pulse rounded-[5px]" />
        ))}
      </div>
    )
  }

  if (deliveries.length === 0) {
    return (
      <p className="text-[10px] text-text-tertiary mt-2">
        No deliveries yet. Use the Test button to send a test event.
      </p>
    )
  }

  return (
    <div className="space-y-1 mt-2">
      {deliveries.map(d => (
        <div key={d.id} className="flex items-start gap-1.5">
          {d.success
            ? <Check className="w-3 h-3 text-status-success shrink-0 mt-0.5" />
            : <X className="w-3 h-3 text-status-error shrink-0 mt-0.5" />
          }
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5">
              <span className="text-xs text-text-secondary">{d.event_type}</span>
              {d.status_code != null && (
                <span className="text-[10px] font-mono text-text-quaternary">{d.status_code}</span>
              )}
              <span className="text-[10px] text-text-tertiary ml-auto">{timeAgo(d.delivered_at)}</span>
              {!d.success && (
                <button
                  onClick={() => retryMut.mutate(d.id)}
                  disabled={retryMut.isPending && retryMut.variables === d.id}
                  className="text-[10px] border border-border-primary rounded-full px-1.5 py-0.5 text-text-secondary hover:text-text-primary disabled:opacity-40 transition-colors"
                  aria-label={`Retry delivery ${d.id}`}
                >
                  {retryMut.isPending && retryMut.variables === d.id ? '…' : 'Retry'}
                </button>
              )}
            </div>
            {d.error && (
              <p className="text-[10px] text-status-error mt-0.5 truncate">{d.error}</p>
            )}
          </div>
        </div>
      ))}
    </div>
  )
}

export default function Settings() {
  const { session, setSession } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])

  const { data: org } = useQuery({
    queryKey: ['org'],
    queryFn: () => client.getOrg(),
  })

  const [orgName, setOrgName] = useState('')
  const [orgSaved, setOrgSaved] = useState(false)

  useEffect(() => { if (org) setOrgName(org.name) }, [org])

  const updateOrgMut = useMutation({
    mutationFn: (name: string) => client.updateOrg({ name }),
    onSuccess: (updated) => {
      qc.invalidateQueries({ queryKey: ['org'] })
      const newSession = { ...session!, org: updated }
      setSession(newSession)
      setOrgSaved(true)
      setTimeout(() => setOrgSaved(false), 2000)
    },
  })

  const [rotateConfirm, setRotateConfirm] = useState(false)
  const [newKey, setNewKey] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [passwordError, setPasswordError] = useState('')
  const [passwordSaved, setPasswordSaved] = useState(false)

  const changePasswordMut = useMutation({
    mutationFn: () => client.changePassword({ current_password: currentPassword, new_password: newPassword }),
    onSuccess: () => {
      setCurrentPassword('')
      setNewPassword('')
      setConfirmPassword('')
      setPasswordError('')
      setPasswordSaved(true)
      setTimeout(() => setPasswordSaved(false), 2000)
    },
    onError: (err: Error) => setPasswordError(err.message),
  })

  const handleChangePassword = (e: React.FormEvent) => {
    e.preventDefault()
    if (newPassword !== confirmPassword) { setPasswordError('Passwords do not match.'); return }
    if (newPassword.length < 8) { setPasswordError('New password must be at least 8 characters.'); return }
    setPasswordError('')
    changePasswordMut.mutate()
  }

  const rotateMut = useMutation({
    mutationFn: () => client.rotateKey(session!.user.id),
    onSuccess: (data) => { setRotateConfirm(false); setNewKey(data.api_key) },
  })

  const defaultEventSettings: AgentEventSettings = {
    resolve_issues: true,
    review_prs: true,
    respond_comments: true,
    auto_index: true,
    scanner: true,
  }

  const [eventSettings, setEventSettings] = useState<AgentEventSettings>(defaultEventSettings)
  const [eventSaved, setEventSaved] = useState(false)

  const [retentionDays, setRetentionDays] = useState<number | null>(null)
  const [customInstructions, setCustomInstructions] = useState('')
  const [instructionsSaved, setInstructionsSaved] = useState(false)
  const [minPasswordLength, setMinPasswordLength] = useState<number>(8)
  const [passwordPolicySaved, setPasswordPolicySaved] = useState(false)
  const [announcementText, setAnnouncementText] = useState('')
  const [announcementType, setAnnouncementType] = useState<'info' | 'warning' | 'error'>('info')
  const [announcementSaved, setAnnouncementSaved] = useState(false)
  const [logoUrl, setLogoUrl] = useState('')
  const [logoSaved, setLogoSaved] = useState(false)

  const { data: orgSettings } = useQuery({
    queryKey: ['org-settings'],
    queryFn: () => client.getOrgSettings(),
    enabled: session?.user.role === 'admin',
  })

  const { data: retentionPreview } = useQuery({
    queryKey: ['retention-preview'],
    queryFn: () => client.getRetentionPreview(),
    enabled: !!orgSettings?.retention_days,
  })

  useEffect(() => {
    if (orgSettings) {
      setEventSettings(orgSettings.events)
      setRetentionDays(orgSettings.retention_days ?? null)
      setCustomInstructions(orgSettings.custom_instructions ?? '')
      setMinPasswordLength(orgSettings.min_password_length ?? 8)
      setAnnouncementText(orgSettings.announcement ?? '')
      setAnnouncementType((orgSettings.announcement_type as 'info' | 'warning' | 'error') ?? 'info')
      setLogoUrl(orgSettings.logo_url ?? '')
    }
  }, [orgSettings])

  const updateEventSettingsMut = useMutation({
    mutationFn: (data: OrgSettings) => client.updateOrgSettings(data),
    onSuccess: (updated) => {
      setEventSettings(updated.events)
      setEventSaved(true)
      setTimeout(() => setEventSaved(false), 2000)
    },
  })

  const updateRetentionMut = useMutation({
    mutationFn: (days: number | null) =>
      client.updateOrgSettings({ events: eventSettings, retention_days: days, custom_instructions: customInstructions || null, min_password_length: minPasswordLength }),
    onSuccess: (updated) => {
      setRetentionDays(updated.retention_days ?? null)
    },
  })

  const updateInstructionsMut = useMutation({
    mutationFn: (instructions: string | null) =>
      client.updateOrgSettings({ events: eventSettings, retention_days: retentionDays, custom_instructions: instructions, min_password_length: minPasswordLength }),
    onSuccess: () => {
      setInstructionsSaved(true)
      setTimeout(() => setInstructionsSaved(false), 2000)
    },
  })

  const updatePasswordPolicyMut = useMutation({
    mutationFn: (length: number) =>
      client.updateOrgSettings({ events: eventSettings, retention_days: retentionDays, custom_instructions: customInstructions || null, min_password_length: length }),
    onSuccess: (updated) => {
      setMinPasswordLength(updated.min_password_length ?? 8)
      setPasswordPolicySaved(true)
      setTimeout(() => setPasswordPolicySaved(false), 2000)
    },
  })

  const updateAnnouncementMut = useMutation({
    mutationFn: ({ text, type }: { text: string; type: string }) =>
      client.updateAnnouncement(text, type),
    onSuccess: (updated) => {
      qc.invalidateQueries({ queryKey: ['org-settings'] })
      setAnnouncementText(updated.announcement ?? '')
      setAnnouncementSaved(true)
      setTimeout(() => setAnnouncementSaved(false), 2000)
    },
  })

  const updateLogoMut = useMutation({
    mutationFn: (url: string | null) => client.updateOrgLogo(url),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['org-settings'] })
      setLogoSaved(true)
      setTimeout(() => setLogoSaved(false), 2000)
    },
  })

  const handleEventToggle = (key: keyof AgentEventSettings) => {
    const next = { ...eventSettings, [key]: !eventSettings[key] }
    setEventSettings(next)
    updateEventSettingsMut.mutate({ events: next })
  }

  // ── Webhooks ──────────────────────────────────────────────────────────────

  const WEBHOOK_EVENTS = ['issues', 'pull_request', 'issue_comment', 'push'] as const

  const [showAddWebhook, setShowAddWebhook] = useState(false)
  const [webhookName, setWebhookName] = useState('')
  const [webhookUrl, setWebhookUrl] = useState('')
  const [webhookSecret, setWebhookSecret] = useState('')
  const [webhookEvents, setWebhookEvents] = useState<string[]>(['*'])
  const [webhookError, setWebhookError] = useState('')

  const { data: webhooksData, isLoading: webhooksLoading, refetch: refetchWebhooks } = useQuery({
    queryKey: ['webhooks'],
    queryFn: () => client.listWebhooks(),
    enabled: session?.user.role === 'admin',
  })
  const webhooks: Webhook[] = webhooksData?.webhooks ?? []

  const createWebhookMut = useMutation({
    mutationFn: (data: CreateWebhookRequest) => client.createWebhook(data),
    onSuccess: () => {
      refetchWebhooks()
      setShowAddWebhook(false)
      setWebhookName('')
      setWebhookUrl('')
      setWebhookSecret('')
      setWebhookEvents(['*'])
      setWebhookError('')
    },
    onError: (err: Error) => setWebhookError(err.message),
  })

  const updateWebhookMut = useMutation({
    mutationFn: ({ id, data }: { id: string; data: { active?: boolean; secret?: string; events?: string[] } }) =>
      client.updateWebhook(id, data),
    onSuccess: () => refetchWebhooks(),
  })

  const deleteWebhookMut = useMutation({
    mutationFn: (id: string) => client.deleteWebhook(id),
    onSuccess: () => refetchWebhooks(),
  })

  // Per-webhook test state: { [id]: { testing, result } }
  const [testStates, setTestStates] = useState<
    Record<string, { testing: boolean; result: WebhookTestResult | null }>
  >({})

  // Per-webhook deliveries panel expanded state
  const [deliveriesExpanded, setDeliveriesExpanded] = useState<Record<string, boolean>>({})

  const handleTestWebhook = async (id: string) => {
    setTestStates(s => ({ ...s, [id]: { testing: true, result: null } }))
    try {
      const result = await client.testWebhook(id)
      setTestStates(s => ({ ...s, [id]: { testing: false, result } }))
      setTimeout(() => setTestStates(s => ({ ...s, [id]: { testing: false, result: null } })), 3000)
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Request failed'
      const result: WebhookTestResult = { success: false, status_code: null, error: msg }
      setTestStates(s => ({ ...s, [id]: { testing: false, result } }))
      setTimeout(() => setTestStates(s => ({ ...s, [id]: { testing: false, result: null } })), 3000)
    }
  }

  const handleWebhookEventsChange = (event: string, checked: boolean) => {
    if (event === '*') {
      setWebhookEvents(checked ? ['*'] : [])
    } else {
      setWebhookEvents(prev => {
        const without = prev.filter(e => e !== '*' && e !== event)
        return checked ? [...without, event] : without
      })
    }
  }

  const handleAddWebhook = (e: React.FormEvent) => {
    e.preventDefault()
    if (!webhookName.trim()) { setWebhookError('Name is required.'); return }
    if (!webhookUrl.trim()) { setWebhookError('Target URL is required.'); return }
    setWebhookError('')
    createWebhookMut.mutate({
      name: webhookName.trim(),
      target_url: webhookUrl.trim(),
      secret: webhookSecret.trim() || undefined,
      events: webhookEvents.length > 0 ? webhookEvents : ['*'],
    })
  }

  const handleDeleteWebhook = (id: string, name: string) => {
    if (!window.confirm(`Delete webhook "${name}"? This cannot be undone.`)) return
    deleteWebhookMut.mutate(id)
  }

  const [importFlash, setImportFlash] = useState<{ type: 'success' | 'warning' | 'error'; message: string } | null>(null)

  const handleImportConfig = () => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = '.json'
    input.onchange = async (e) => {
      const file = (e.target as HTMLInputElement).files?.[0]
      if (!file) return
      try {
        const text = await file.text()
        const data = JSON.parse(text)
        const result: ImportConfigResponse = await client.importOrgConfig(data)
        qc.invalidateQueries({ queryKey: ['org-settings'] })
        if (result.applied_fields.length === 0) {
          setImportFlash({ type: 'warning', message: 'No applicable fields found in the config file.' })
        } else {
          const appliedMsg = `Applied: ${result.applied_fields.join(', ')}`
          const skippedMsg = result.skipped_fields.length > 0
            ? ` · Skipped: ${result.skipped_fields.join(', ')} (not imported for safety)`
            : ''
          setImportFlash({ type: result.skipped_fields.length > 0 ? 'warning' : 'success', message: appliedMsg + skippedMsg })
        }
        setTimeout(() => setImportFlash(null), 5000)
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : 'Import failed'
        setImportFlash({ type: 'error', message: msg })
        setTimeout(() => setImportFlash(null), 5000)
      }
    }
    input.click()
  }

  const handleExportAll = async () => {
    const [memories, users, audit] = await Promise.all([
      client.listMemories({ limit: 10_000 }),
      client.listUsers(),
      client.getAuditLog({ limit: 10_000 }),
    ])
    const blob = new Blob([JSON.stringify({ memories, users, audit }, null, 2)], { type: 'application/json' })
    const a = document.createElement('a')
    a.href = URL.createObjectURL(blob)
    a.download = `nexusmind-export-${new Date().toISOString().slice(0, 10)}.json`
    a.click()
  }

  const inputCls = 'w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 transition-colors'

  return (
    <div className="p-8 max-w-2xl mx-auto space-y-10">
      <div>
        <h1 className="text-[21px] font-semibold text-text-primary tracking-[0.231px]">Settings</h1>
        <p className="text-[14px] text-text-tertiary mt-0.5 tracking-[-0.224px]">Organization and account configuration</p>
      </div>

      {/* Organization */}
      <section className="space-y-4">
        <p className="text-text-tertiary text-[12px] tracking-[-0.12px]">Organization</p>
        <div className="border border-border-primary rounded-[18px] p-5 space-y-4">
          <div className="space-y-1.5">
            <label htmlFor="org-name" className="text-xs text-text-tertiary">Name</label>
            <input
              id="org-name"
              value={orgName}
              onChange={e => setOrgName(e.target.value)}
              readOnly={session?.user.role !== 'admin'}
              className={`${inputCls} ${session?.user.role !== 'admin' ? 'opacity-50 cursor-not-allowed' : ''}`}
            />
          </div>
          <div className="space-y-1.5">
            <label htmlFor="org-slug" className="text-xs text-text-tertiary">Slug</label>
            <input id="org-slug" value={org?.slug ?? ''} readOnly className={`${inputCls} opacity-50 cursor-not-allowed`} />
          </div>
          <div className="space-y-1.5">
            <label htmlFor="org-created" className="text-xs text-text-tertiary">Created</label>
            <input
              id="org-created"
              value={org ? new Date(org.created_at).toLocaleDateString() : ''}
              readOnly
              className={`${inputCls} opacity-50 cursor-not-allowed`}
            />
          </div>
          {session?.user.role === 'admin' && (
            <div className="flex items-center gap-3">
              <button
                onClick={() => updateOrgMut.mutate(orgName)}
                disabled={updateOrgMut.isPending || orgName === org?.name}
                className="px-4 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-semibold disabled:opacity-30 transition-colors"
              >
                {updateOrgMut.isPending ? 'Saving…' : orgSaved ? 'Saved!' : 'Save'}
              </button>
              {updateOrgMut.isError && (
                <p className="text-xs text-status-error/80">Failed to save.</p>
              )}
            </div>
          )}
          {session?.user.role === 'admin' && (
            <div className="space-y-2 pt-2 border-t border-border-secondary/30">
              <p className="text-xs text-text-tertiary font-semibold">Branding</p>
              <div className="space-y-1.5">
                <label className="text-xs text-text-tertiary">Logo URL</label>
                <div className="flex items-center gap-2">
                  <input
                    value={logoUrl}
                    onChange={e => setLogoUrl(e.target.value)}
                    placeholder="https://example.com/logo.png"
                    className="rounded-[8px] border border-border-primary bg-white/[0.04] text-xs text-text-primary px-2 py-1.5 focus:outline-none focus:border-accent-blue/60 flex-1"
                  />
                  <button
                    onClick={() => updateLogoMut.mutate(logoUrl.trim() || null)}
                    disabled={updateLogoMut.isPending}
                    className="rounded-full border border-border-primary px-3 py-1.5 text-xs text-text-secondary hover:text-text-primary hover:bg-[#272729] transition-colors disabled:opacity-50"
                  >
                    {updateLogoMut.isPending ? 'Saving…' : 'Save'}
                  </button>
                  {logoSaved && <span className="text-xs text-status-success">Saved</span>}
                </div>
                {logoUrl && (
                  <img
                    src={logoUrl}
                    className="w-8 h-8 rounded-full object-cover border border-border-primary"
                    alt="org logo preview"
                  />
                )}
              </div>
            </div>
          )}
        </div>
      </section>

      {/* Data Retention */}
      {session?.user.role === 'admin' && (
        <section className="space-y-4">
          <p className="text-text-tertiary text-[12px] tracking-[-0.12px]">Data Retention</p>
          <div className="border border-border-primary rounded-[18px] p-5 space-y-4">
            <div className="space-y-4">
              <div>
                <p className="text-sm font-semibold text-text-primary">Data Retention</p>
                <p className="text-xs text-text-tertiary mt-0.5">
                  Automatically delete memories older than the selected period. Set to "Never" to keep all memories.
                </p>
              </div>
              <div className="flex items-center gap-3">
                <div className="relative">
                  <select
                    value={retentionDays ?? ''}
                    onChange={(e) => setRetentionDays(e.target.value ? parseInt(e.target.value) : null)}
                    className="bg-transparent border border-border-primary rounded-[11px] px-3 py-2 text-sm text-text-primary focus:outline-none focus:border-accent-blue/60 appearance-none pr-8"
                  >
                    <option value="">Never (keep all)</option>
                    <option value="30">30 days</option>
                    <option value="60">60 days</option>
                    <option value="90">90 days</option>
                    <option value="180">180 days</option>
                    <option value="365">1 year</option>
                  </select>
                </div>
                <button
                  onClick={() => updateRetentionMut.mutate(retentionDays)}
                  disabled={updateRetentionMut.isPending}
                  className="px-4 py-2 bg-accent-blue text-white text-sm font-semibold rounded-full hover:opacity-90 transition-opacity disabled:opacity-50"
                >
                  {updateRetentionMut.isPending ? 'Saving…' : 'Save'}
                </button>
                {updateRetentionMut.isSuccess && (
                  <span className="text-xs text-status-success">Saved</span>
                )}
              </div>
              {orgSettings?.retention_days && (
                <p className="text-xs text-text-quaternary mt-1.5">
                  {retentionPreview ? `${retentionPreview.would_delete} memories would be deleted with current settings` : '…'}
                </p>
              )}
            </div>
          </div>
        </section>
      )}

      {/* Agent Instructions */}
      {session?.user.role === 'admin' && (
        <section className="space-y-4">
          <p className="text-text-tertiary text-[12px] tracking-[-0.12px]">Agent Instructions</p>
          <div className="border border-border-primary rounded-[18px] p-5 space-y-4">
            <div>
              <p className="text-sm font-semibold text-text-primary">Agent Instructions</p>
              <p className="text-xs text-text-tertiary mt-0.5 mb-3">
                System-level instructions added to every agent's context for this organization.
                Use this to set team conventions, coding standards, or custom behavior.
              </p>
            </div>
            <textarea
              value={customInstructions}
              onChange={e => setCustomInstructions(e.target.value)}
              rows={5}
              placeholder="e.g., Always use TypeScript strict mode. Prefer functional components. Follow our naming conventions..."
              className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 resize-y min-h-[100px]"
            />
            <div className="flex items-center gap-3">
              <button
                onClick={() => updateInstructionsMut.mutate(customInstructions.trim() || null)}
                disabled={updateInstructionsMut.isPending}
                className="px-4 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-semibold disabled:opacity-30 transition-colors"
              >
                {updateInstructionsMut.isPending ? 'Saving…' : 'Save Instructions'}
              </button>
              {instructionsSaved && <span className="text-xs text-status-success">Saved ✓</span>}
            </div>
          </div>
        </section>
      )}

      {/* Announcement Banner */}
      {session?.user.role === 'admin' && (
        <section className="space-y-4">
          <p className="text-text-tertiary text-[12px] tracking-[-0.12px]">Announcement Banner</p>
          <div className="border border-border-primary rounded-[18px] p-5 space-y-4">
            <div>
              <p className="text-sm font-semibold text-text-primary">Announcement Banner</p>
              <p className="text-xs text-text-tertiary mt-0.5">
                Display a banner at the top of the admin UI for all users. Leave blank to hide the banner.
              </p>
            </div>

            {/* Live preview */}
            {announcementText.trim() && (
              <div className={cn(
                'w-full px-5 py-2.5 text-xs flex items-center gap-2',
                announcementType === 'error'
                  ? 'bg-status-error/10 text-status-error border-b border-status-error/20'
                  : announcementType === 'warning'
                  ? 'bg-status-warning/10 text-status-warning border-b border-status-warning/20'
                  : 'bg-accent-blue/10 text-accent-blue border-b border-accent-blue/20',
              )}>
                <AlertCircle className="w-3 h-3 shrink-0" />
                <span>{announcementText}</span>
              </div>
            )}

            <textarea
              value={announcementText}
              onChange={e => setAnnouncementText(e.target.value)}
              placeholder="e.g. Scheduled maintenance on Saturday 2 AM UTC. Expect ~30 min downtime."
              className="w-full bg-white/[0.04] border border-border-primary rounded-[8px] px-3 py-2 text-xs text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 resize-none h-20"
            />

            <div className="flex items-center gap-3">
              <select
                value={announcementType}
                onChange={e => setAnnouncementType(e.target.value as 'info' | 'warning' | 'error')}
                className="bg-white/[0.04] border border-border-primary rounded-[11px] px-3 py-2 text-sm text-text-primary focus:outline-none focus:border-accent-blue/60 appearance-none"
              >
                <option value="info">Info</option>
                <option value="warning">Warning</option>
                <option value="error">Error</option>
              </select>
              <button
                onClick={() => updateAnnouncementMut.mutate({ text: announcementText.trim(), type: announcementType })}
                disabled={updateAnnouncementMut.isPending}
                className="rounded-[8px] bg-accent-blue text-white text-xs font-semibold px-3 py-1.5 hover:opacity-90 disabled:opacity-50 transition-opacity"
              >
                {updateAnnouncementMut.isPending ? 'Saving…' : 'Save'}
              </button>
              {announcementText.trim() && (
                <button
                  onClick={() => {
                    setAnnouncementText('')
                    updateAnnouncementMut.mutate({ text: '', type: announcementType })
                  }}
                  disabled={updateAnnouncementMut.isPending}
                  className="text-xs text-text-quaternary hover:text-status-error transition-colors disabled:opacity-50"
                >
                  Clear
                </button>
              )}
              {announcementSaved && <span className="text-xs text-status-success">Saved ✓</span>}
            </div>
          </div>
        </section>
      )}

      {/* Password Policy */}
      {session?.user.role === 'admin' && (
        <section className="space-y-4">
          <p className="text-text-tertiary text-[12px] tracking-[-0.12px]">Password Policy</p>
          <div className="border border-border-primary rounded-[18px] p-5 space-y-4">
            <div>
              <p className="text-sm font-semibold text-text-primary">Minimum password length</p>
              <p className="text-xs text-text-tertiary mt-0.5">
                Enforce a minimum character count for all passwords in this organization.
              </p>
            </div>
            <div className="flex items-center gap-3">
              <select
                value={minPasswordLength}
                onChange={e => setMinPasswordLength(parseInt(e.target.value))}
                className="rounded-[11px] bg-transparent border border-border-primary px-3 py-2 text-sm text-text-primary focus:outline-none focus:border-accent-blue/60 appearance-none"
              >
                <option value={6}>6 characters</option>
                <option value={8}>8 characters</option>
                <option value={10}>10 characters</option>
                <option value={12}>12 characters</option>
                <option value={16}>16 characters</option>
                <option value={20}>20 characters</option>
              </select>
              <button
                onClick={() => updatePasswordPolicyMut.mutate(minPasswordLength)}
                disabled={updatePasswordPolicyMut.isPending}
                className="rounded-full bg-accent-blue text-white font-semibold px-4 py-2 text-sm hover:opacity-90 disabled:opacity-50 transition-opacity"
              >
                {updatePasswordPolicyMut.isPending ? 'Saving…' : 'Save'}
              </button>
              {passwordPolicySaved && <span className="text-xs text-status-success">Saved ✓</span>}
            </div>
          </div>
        </section>
      )}

      {/* Password */}
      <section className="space-y-4">
        <p className="text-text-tertiary text-[12px] tracking-[-0.12px]">Password</p>
        <div className="border border-border-primary rounded-[18px] p-5">
          <form onSubmit={handleChangePassword} className="space-y-4">
            <div className="space-y-1.5">
              <label htmlFor="current-password" className="text-xs text-text-tertiary">Current password</label>
              <input
                id="current-password"
                type="password"
                value={currentPassword}
                onChange={e => setCurrentPassword(e.target.value)}
                autoComplete="current-password"
                className={inputCls}
              />
            </div>
            <div className="space-y-1.5">
              <label htmlFor="new-password" className="text-xs text-text-tertiary">New password</label>
              <input
                id="new-password"
                type="password"
                value={newPassword}
                onChange={e => setNewPassword(e.target.value)}
                autoComplete="new-password"
                className={inputCls}
              />
              <p className="text-[11px] text-text-quaternary mt-1">At least 8 characters</p>
            </div>
            <div className="space-y-1.5">
              <label htmlFor="confirm-password" className="text-xs text-text-tertiary">Confirm new password</label>
              <input
                id="confirm-password"
                type="password"
                value={confirmPassword}
                onChange={e => setConfirmPassword(e.target.value)}
                autoComplete="new-password"
                className={inputCls}
              />
            </div>
            {passwordError && <p className="text-xs text-status-error/80">{passwordError}</p>}
            <div className="flex items-center gap-3">
              <button
                type="submit"
                disabled={changePasswordMut.isPending || !currentPassword || !newPassword || !confirmPassword}
                className="px-4 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-semibold disabled:opacity-30 transition-colors"
              >
                {changePasswordMut.isPending ? 'Saving…' : passwordSaved ? 'Saved!' : 'Update password'}
              </button>
            </div>
          </form>
        </div>
      </section>

      {/* My API Key */}
      <section className="space-y-4">
        <p className="text-text-tertiary text-[12px] tracking-[-0.12px]">My API Key</p>
        <div className="border border-border-primary rounded-[18px] p-5 space-y-4">
          <div className="flex items-center gap-3 bg-[#272729] rounded-[11px] px-3 py-2">
            <code className="flex-1 text-xs text-text-tertiary truncate">
              Session managed via secure HttpOnly cookie
            </code>
          </div>

          {newKey ? (
            <div className="space-y-3">
              <p className="text-xs text-text-tertiary">New key — copy it now, it won't be shown again.</p>
              <div className="flex items-center gap-2 bg-[#272729] rounded-[11px] px-3 py-2">
                <code className="flex-1 text-xs text-text-secondary break-all">{newKey}</code>
                <button
                  onClick={() => { navigator.clipboard.writeText(newKey); setCopied(true); setTimeout(() => setCopied(false), 2000) }}
                  className="text-xs text-text-tertiary hover:text-text-secondary transition-colors shrink-0"
                >
                  {copied ? 'Copied!' : 'Copy'}
                </button>
              </div>
              <button
                onClick={() => setNewKey(null)}
                className="text-xs text-text-tertiary hover:text-text-secondary transition-colors"
              >
                Done
              </button>
            </div>
          ) : rotateConfirm ? (
            <div className="space-y-3">
              <p className="text-xs text-text-secondary">Your current key will stop working immediately. Continue?</p>
              <div className="flex gap-2">
                <button
                  onClick={() => setRotateConfirm(false)}
                  className="flex-1 py-2 rounded-full border border-border-primary text-sm text-text-tertiary hover:text-text-secondary transition-colors"
                >
                  Cancel
                </button>
                <button
                  onClick={() => rotateMut.mutate()}
                  disabled={rotateMut.isPending}
                  className="flex-1 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-semibold disabled:opacity-40 transition-colors"
                >
                  {rotateMut.isPending ? 'Rotating…' : 'Rotate'}
                </button>
              </div>
            </div>
          ) : (
            <button
              onClick={() => setRotateConfirm(true)}
              className="text-xs text-text-tertiary hover:text-text-secondary transition-colors"
            >
              Rotate key
            </button>
          )}
        </div>
      </section>

      {/* Agent Events */}
      {session?.user.role === 'admin' && (
        <section className="space-y-4">
          <p className="text-text-tertiary text-[12px] tracking-[-0.12px]">
            Agent Events
            {eventSaved && <span className="ml-2 text-status-success text-xs">Saved</span>}
          </p>
          <div className="border border-border-primary rounded-[18px] p-5 space-y-4">
            <p className="text-xs text-text-tertiary">
              Control which GitHub events the AI agent reacts to automatically.
            </p>
            <div className="divide-y divide-border-secondary">
              {([
                { key: 'resolve_issues' as const, label: 'Resolve Issues', description: 'Agent responds to newly opened GitHub issues' },
                { key: 'review_prs' as const, label: 'Review Pull Requests', description: 'Agent auto-reviews PRs when opened or updated' },
                { key: 'respond_comments' as const, label: 'Respond to Comments', description: 'Agent replies to issue and PR review comments' },
                { key: 'auto_index' as const, label: 'Auto-index on Push', description: 'Trigger code indexing jobs on every push' },
                { key: 'scanner' as const, label: 'Proactive Scanner', description: 'Periodically scan for issues without being triggered' },
              ] as { key: keyof AgentEventSettings; label: string; description: string }[]).map(({ key, label, description }) => (
                <div key={key} className="flex items-center justify-between gap-4 py-3 first:pt-0 last:pb-0">
                  <div>
                    <p className="text-sm text-text-secondary font-semibold">{label}</p>
                    <p className="text-xs text-text-tertiary mt-0.5">{description}</p>
                  </div>
                  <Switch
                    checked={eventSettings[key]}
                    onCheckedChange={() => handleEventToggle(key)}
                    size="sm"
                  />
                </div>
              ))}
            </div>
          </div>
        </section>
      )}

      {/* Memory Templates */}
      <MemoryTemplatesSection />

      {/* Webhooks */}
      {session?.user.role === 'admin' && (
        <section className="space-y-4">
          <p className="text-text-tertiary text-[12px] tracking-[-0.12px]">Webhooks</p>
          <div className="border border-border-primary rounded-[18px] p-5 space-y-4">
            <p className="text-xs text-text-tertiary">Manage GitHub webhook endpoints for this organization.</p>

            {/* Loading skeletons */}
            {webhooksLoading && (
              <div className="space-y-3">
                <div className="animate-pulse h-16 bg-[#272729] rounded-[18px]" />
                <div className="animate-pulse h-16 bg-[#272729] rounded-[18px]" />
              </div>
            )}

            {/* Webhook cards */}
            {!webhooksLoading && webhooks.length > 0 && (
              <div className="space-y-3">
                {webhooks.map(wh => (
                  <div key={wh.id} className="border border-border-primary rounded-[18px] p-4 space-y-3">
                    {/* Header row: name + switch */}
                    <div className="flex items-center justify-between gap-3">
                      <p className="text-sm font-semibold text-text-primary truncate">{wh.name}</p>
                      <Switch
                        checked={wh.active}
                        onCheckedChange={(checked) =>
                          updateWebhookMut.mutate({ id: wh.id, data: { active: checked } })
                        }
                        size="sm"
                      />
                    </div>
                    {/* URL row */}
                    <p className="text-xs font-mono text-text-tertiary truncate">{wh.target_url}</p>
                    {/* Events chips */}
                    <div className="flex flex-wrap gap-1">
                      {wh.events.map(ev => (
                        <span
                          key={ev}
                          className="rounded-[5px] px-1.5 py-0.5 text-[10px] font-semibold bg-[#272729] border border-border-secondary text-text-tertiary"
                        >
                          {ev}
                        </span>
                      ))}
                    </div>
                    {/* Footer row: date + actions */}
                    <div className="flex items-center justify-between gap-3">
                      <span className="text-[11px] text-text-quaternary">
                        Created {new Date(wh.created_at).toLocaleDateString()}
                      </span>
                      <div className="flex items-center gap-2">
                        {testStates[wh.id]?.result && (
                          testStates[wh.id].result!.success
                            ? <span className="text-xs text-status-success">✓ {testStates[wh.id].result!.status_code}</span>
                            : <span className="text-xs text-status-error">✗ {testStates[wh.id].result!.error}</span>
                        )}
                        <button
                          onClick={() => handleTestWebhook(wh.id)}
                          disabled={!!testStates[wh.id]?.testing}
                          className="border border-border-primary rounded-[8px] px-2.5 py-1 text-xs text-text-secondary hover:text-text-primary transition-colors disabled:opacity-40"
                        >
                          {testStates[wh.id]?.testing ? 'Testing…' : 'Test'}
                        </button>
                        <button
                          onClick={() => handleDeleteWebhook(wh.id, wh.name)}
                          className="text-xs border border-status-error/20 rounded-full px-3 py-1 text-status-error/60 hover:text-status-error transition-colors"
                        >
                          Delete
                        </button>
                      </div>
                    </div>
                    {/* Deliveries collapsible */}
                    <div>
                      <button
                        onClick={() => setDeliveriesExpanded(s => ({ ...s, [wh.id]: !s[wh.id] }))}
                        className="flex items-center gap-1 text-[11px] text-text-tertiary hover:text-text-secondary transition-colors"
                      >
                        {deliveriesExpanded[wh.id]
                          ? <ChevronUp className="w-3 h-3" />
                          : <ChevronDown className="w-3 h-3" />
                        }
                        Deliveries (last 20)
                      </button>
                      {deliveriesExpanded[wh.id] && (
                        <WebhookDeliveryPanel webhookId={wh.id} client={client} />
                      )}
                    </div>
                  </div>
                ))}
              </div>
            )}

            {/* Empty state */}
            {!webhooksLoading && webhooks.length === 0 && !showAddWebhook && (
              <div className="text-center py-6 space-y-2">
                <p className="text-sm font-semibold text-text-primary">No webhooks configured</p>
                <p className="text-xs text-text-tertiary">Add a webhook to receive GitHub events.</p>
              </div>
            )}

            {/* Add Webhook form */}
            {showAddWebhook && (
              <form onSubmit={handleAddWebhook} className="space-y-3">
                <div className="space-y-1.5">
                  <label htmlFor="webhook-name" className="text-xs text-text-tertiary">Name</label>
                  <input
                    id="webhook-name"
                    value={webhookName}
                    onChange={e => setWebhookName(e.target.value)}
                    placeholder="e.g. pr-reviewer"
                    className={inputCls}
                  />
                </div>
                <div className="space-y-1.5">
                  <label htmlFor="webhook-url" className="text-xs text-text-tertiary">Target URL</label>
                  <input
                    id="webhook-url"
                    type="url"
                    value={webhookUrl}
                    onChange={e => setWebhookUrl(e.target.value)}
                    placeholder="https://your-server.com/webhook"
                    className={inputCls}
                  />
                </div>
                <div className="space-y-1.5">
                  <label htmlFor="webhook-secret" className="text-xs text-text-tertiary">Secret (optional)</label>
                  <input
                    id="webhook-secret"
                    type="password"
                    value={webhookSecret}
                    onChange={e => setWebhookSecret(e.target.value)}
                    placeholder="Webhook signing secret"
                    className={inputCls}
                    autoComplete="off"
                  />
                </div>
                <div className="space-y-1.5">
                  <label className="text-xs text-text-tertiary">Events</label>
                  <div className="space-y-1.5">
                    <label className="flex items-center gap-2 text-sm text-text-secondary cursor-pointer">
                      <input
                        type="checkbox"
                        checked={webhookEvents.includes('*')}
                        onChange={e => handleWebhookEventsChange('*', e.target.checked)}
                        className="accent-accent-blue"
                      />
                      All events (*)
                    </label>
                    {WEBHOOK_EVENTS.map(ev => (
                      <label key={ev} className="flex items-center gap-2 text-sm text-text-secondary cursor-pointer">
                        <input
                          type="checkbox"
                          checked={webhookEvents.includes(ev)}
                          onChange={e => handleWebhookEventsChange(ev, e.target.checked)}
                          disabled={webhookEvents.includes('*')}
                          className="accent-accent-blue"
                        />
                        {ev}
                      </label>
                    ))}
                  </div>
                </div>
                {webhookError && <p className="text-xs text-status-error/80">{webhookError}</p>}
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() => { setShowAddWebhook(false); setWebhookError('') }}
                    className="rounded-full border border-border-primary px-4 py-1.5 text-sm text-text-secondary hover:text-text-primary transition-colors"
                  >
                    Cancel
                  </button>
                  <button
                    type="submit"
                    disabled={createWebhookMut.isPending}
                    className="rounded-full bg-accent-blue text-white px-4 py-1.5 text-sm font-semibold hover:opacity-90 disabled:opacity-50 transition-opacity"
                  >
                    {createWebhookMut.isPending ? 'Saving…' : 'Save webhook'}
                  </button>
                </div>
              </form>
            )}

            {/* Add Webhook CTA */}
            {!showAddWebhook && !webhooksLoading && (
              <button
                onClick={() => setShowAddWebhook(true)}
                className="text-xs border border-border-primary rounded-full px-3 py-1.5 text-text-tertiary hover:text-text-secondary hover:bg-[#272729] transition-colors"
              >
                + Add Webhook
              </button>
            )}
          </div>
        </section>
      )}

      {/* Danger zone */}
      {session?.user.role === 'admin' && (
        <section className="space-y-4">
          <p className="text-text-tertiary text-[12px] tracking-[-0.12px]">Danger Zone</p>
          <div className="border border-status-error/15 rounded-[18px] p-5 space-y-3">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-text-secondary font-semibold">Export all data</p>
                <p className="text-xs text-text-tertiary mt-0.5">Download all memories, users, and audit logs as JSON.</p>
              </div>
              <button
                onClick={handleExportAll}
                className="text-xs text-text-tertiary hover:text-text-secondary border border-border-primary rounded-full px-3 py-1.5 hover:bg-[#272729] transition-colors"
              >
                Export
              </button>
            </div>
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-text-secondary font-semibold">Export org config</p>
                <p className="text-xs text-text-tertiary mt-0.5">Download org settings, webhooks, and project list as JSON.</p>
              </div>
              <div className="flex items-center gap-3">
                <button
                  onClick={handleImportConfig}
                  className="border border-border-primary rounded-full px-4 py-2 text-sm text-text-secondary hover:text-text-primary flex items-center gap-2 transition-colors"
                >
                  <Upload className="w-3.5 h-3.5" /> Import config
                </button>
                <button
                  onClick={() => client.exportOrgConfig().then(blob => downloadBlob(blob, 'nexusmind-config.json'))}
                  className="border border-border-primary rounded-full px-4 py-2 text-sm text-text-secondary hover:text-text-primary flex items-center gap-2 transition-colors"
                >
                  <Download className="w-3.5 h-3.5" /> Export org config
                </button>
              </div>
            </div>
            {importFlash && (
              <p className={`text-xs mt-1 ${importFlash.type === 'success' ? 'text-status-success' : importFlash.type === 'warning' ? 'text-status-warning' : 'text-status-error'}`}>
                {importFlash.type === 'success' ? '✓ ' : ''}{importFlash.message}
              </p>
            )}
          </div>
        </section>
      )}
    </div>
  )
}
