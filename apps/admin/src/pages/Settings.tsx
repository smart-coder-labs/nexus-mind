import { useMemo, useState, useEffect } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { Switch } from '../components/ui/Switch/Switch'
import {
  Check, X, ChevronDown, ChevronUp, Download, Upload, Plus, Pencil, Trash2, AlertCircle,
  Settings as SettingsIcon, RefreshCw, CheckCircle2, GitPullRequest, MessageSquare, GitBranch, Search,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import type { AgentEventSettings, OrgSettings, Webhook, CreateWebhookRequest, WebhookTestResult, ImportConfigResponse } from '../types'

// Same glass recipe as GLASS_PANEL in src/pages/Sdd.tsx — inlined rather than
// imported to keep pages independent.
const GLASS_PANEL = 'border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px]'

// Top-level card surface per the Settings mockup: glass panel + 16px radius + 22px padding.
const CARD = `${GLASS_PANEL} rounded-[16px] p-[22px]`

const PRIMARY_BTN = 'h-10 px-4 rounded-[11px] bg-accent-blue hover:bg-accent-blue-hover text-white text-[13px] font-bold disabled:opacity-40 transition-colors shrink-0'
const NEUTRAL_BTN = 'h-[38px] px-4 rounded-[10px] bg-white/[0.06] border border-white/[0.09] text-[13px] font-semibold text-text-secondary hover:text-text-primary hover:bg-white/[0.10] transition-colors disabled:opacity-40'
const NEUTRAL_BTN_SM = 'h-8 px-3 rounded-[9px] bg-white/[0.06] border border-white/[0.09] text-xs font-semibold text-text-secondary hover:text-text-primary hover:bg-white/[0.10] transition-colors disabled:opacity-40 flex items-center gap-1.5'

const inputCls = 'w-full h-10 px-3.5 rounded-[11px] border border-white/[0.09] bg-white/[0.03] text-text-primary text-[13px] placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 transition-colors'
const selectCls = 'h-10 px-3.5 rounded-[11px] border border-white/[0.09] bg-white/[0.03] text-text-primary text-[13px] focus:outline-none focus:border-accent-blue/60 transition-colors appearance-none'

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

  const formInputCls = 'w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2.5 text-xs text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 transition-colors'

  return (
    <div className={`${CARD} flex flex-col gap-4`}>
      <div className="flex items-center justify-between gap-3">
        <div>
          <h2 className="text-[16px] font-bold text-text-primary">Memory templates</h2>
          <p className="text-[12.5px] text-text-tertiary mt-0.5">Reusable templates that pre-fill content when creating memories.</p>
        </div>
        {!showForm && (
          <button onClick={openCreate} className={NEUTRAL_BTN_SM}>
            <Plus className="w-3 h-3" />
            Add template
          </button>
        )}
      </div>

      {/* Template list */}
      {templates.length > 0 && !showForm && (
        <div className="space-y-2">
          {templates.map(t => (
            <div key={t.id} className="flex items-start gap-3 p-3 rounded-[11px] border border-white/[0.07] bg-white/[0.02]">
              <div className="flex-1 min-w-0 space-y-1">
                <div className="flex items-center gap-2 flex-wrap">
                  <span className="text-xs font-semibold text-text-primary">{t.name}</span>
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
                  className="p-1.5 rounded-[8px] text-text-quaternary hover:text-text-secondary hover:bg-white/[0.10] transition-colors"
                >
                  <Pencil className="w-3.5 h-3.5" />
                </button>
                <button
                  onClick={() => handleDelete(t.id)}
                  aria-label={`Delete template ${t.name}`}
                  className="p-1.5 rounded-[8px] text-text-quaternary hover:text-status-error hover:bg-white/[0.10] transition-colors"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {templates.length === 0 && !showForm && (
        <div className="flex flex-col items-center gap-1.5 py-6 px-6 rounded-[12px] border-[1.5px] border-dashed border-white/[0.1]">
          <p className="text-[13.5px] font-bold text-text-secondary">No templates yet</p>
          <p className="text-xs text-text-quaternary">Add a template to speed up memory creation.</p>
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
              className={formInputCls}
            />
          </div>
          <div className="space-y-1.5">
            <label className="text-xs text-text-tertiary">Type</label>
            <select
              value={formType}
              onChange={e => setFormType(e.target.value as TemplateType)}
              className="bg-transparent border border-border-primary rounded-[11px] px-3 py-2.5 text-xs text-text-primary focus:outline-none focus:border-accent-blue/60 transition-colors appearance-none w-full"
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
              className="w-full bg-transparent border border-border-primary rounded-[11px] px-3 py-2.5 text-xs text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 transition-colors resize-y min-h-[100px]"
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
              className="rounded-full bg-accent-blue text-white px-3 py-1.5 text-xs font-semibold hover:opacity-90 disabled:opacity-50 transition-opacity"
            >
              {editingId ? 'Save changes' : 'Add template'}
            </button>
          </div>
        </div>
      )}
    </div>
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
          <div key={i} className="h-5 bg-white/[0.06] animate-pulse rounded-[5px]" />
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

// ── Settings section nav ──────────────────────────────────────────────────────

type SectionId = 'account' | 'org' | 'agents' | 'integrations'

const EVENT_ICONS: Record<keyof AgentEventSettings, { Icon: LucideIcon; color: string; bg: string }> = {
  resolve_issues:   { Icon: CheckCircle2,   color: '#34d399', bg: 'rgba(52,211,153,0.1)' },
  review_prs:       { Icon: GitPullRequest, color: '#60a5fa', bg: 'rgba(96,165,250,0.1)' },
  respond_comments: { Icon: MessageSquare,  color: '#a78bfa', bg: 'rgba(167,139,250,0.1)' },
  auto_index:       { Icon: GitBranch,      color: '#facc15', bg: 'rgba(250,204,21,0.1)' },
  scanner:          { Icon: Search,         color: '#f87171', bg: 'rgba(248,113,113,0.1)' },
}

export default function Settings() {
  const { session, setSession } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(), [session])
  const isAdmin = session?.user.role === 'admin' || session?.user.role === 'super_user'

  const [activeSection, setActiveSection] = useState<SectionId>('account')

  const NAV_ITEMS: { id: SectionId; label: string }[] = [
    { id: 'account', label: 'My account' },
    { id: 'org', label: 'Organization' },
    { id: 'agents', label: 'Agents' },
    // Every card in Integrations & data is admin/super_user only — omit the
    // tab entirely for other roles instead of showing an empty panel.
    ...(isAdmin ? [{ id: 'integrations' as SectionId, label: 'Integrations & data' }] : []),
  ]

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

  const [displayName, setDisplayName] = useState(session?.user?.name ?? '')
  const [profileSaved, setProfileSaved] = useState(false)

  const updateProfileMut = useMutation({
    mutationFn: (data: { name?: string }) => client.updateProfile(data),
    onSuccess: () => {
      setProfileSaved(true)
      setTimeout(() => setProfileSaved(false), 2000)
    },
    onError: () => {
      // Backend may not support this endpoint yet — surface gracefully
    },
  })

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
    enabled: isAdmin,
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
    enabled: isAdmin,
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

  // ── Org Data Export ───────────────────────────────────────────────────────
  const [exportingMemories, setExportingMemories] = useState(false)
  const [exportingConventions, setExportingConventions] = useState(false)
  const [exportingAll, setExportingAll] = useState(false)

  const downloadJSON = (data: unknown, filename: string) => {
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = filename
    a.click()
    URL.revokeObjectURL(url)
  }

  const handleExportMemories = async () => {
    setExportingMemories(true)
    try {
      const data = await client.listMemories({ limit: 10000 })
      downloadJSON(data, `nexusmind-memories-${new Date().toISOString().slice(0, 10)}.json`)
    } finally { setExportingMemories(false) }
  }

  const handleExportConventions = async () => {
    setExportingConventions(true)
    try {
      const data = await client.listConventions()
      downloadJSON(data, `nexusmind-conventions-${new Date().toISOString().slice(0, 10)}.json`)
    } finally { setExportingConventions(false) }
  }

  const handleExportAllData = async () => {
    setExportingAll(true)
    try {
      const [memories, conventions, projects] = await Promise.all([
        client.listMemories({ limit: 10000 }).catch(() => []),
        client.listConventions().catch(() => []),
        client.listProjects().catch(() => []),
      ])
      downloadJSON(
        { memories, conventions, projects, exported_at: new Date().toISOString() },
        `nexusmind-backup-${new Date().toISOString().slice(0, 10)}.json`,
      )
    } finally { setExportingAll(false) }
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

  return (
    <div className="p-8 max-w-[1280px] mx-auto">
      {/* Header */}
      <div className="flex items-center gap-3.5 mb-6">
        <div className="w-11 h-11 rounded-[13px] bg-white/[0.06] flex items-center justify-center shrink-0">
          <SettingsIcon className="w-[22px] h-[22px] text-text-secondary" strokeWidth={1.7} />
        </div>
        <div>
          <h1 className="text-[26px] font-extrabold tracking-[-0.02em] text-text-primary">Settings</h1>
          <p className="text-[13px] text-text-tertiary mt-0.5">Organization and account configuration.</p>
        </div>
      </div>

      <div className="grid grid-cols-[190px_minmax(0,1fr)] gap-6 items-start">
        {/* Section nav */}
        <nav className="sticky top-6 flex flex-col gap-0.5">
          {NAV_ITEMS.map(item => (
            <button
              key={item.id}
              type="button"
              onClick={() => setActiveSection(item.id)}
              className={cn(
                'flex items-center gap-2.5 px-3 py-2 rounded-[9px] text-[13px] text-left transition-colors border-l-2',
                activeSection === item.id
                  ? 'font-bold text-text-primary bg-white/[0.06] border-accent-blue'
                  : 'font-medium text-text-tertiary border-transparent hover:text-text-primary',
              )}
            >
              {item.label}
            </button>
          ))}
        </nav>

        {/* Panels */}
        <div className="flex flex-col gap-4 min-w-0">

          {/* ── My account ────────────────────────────────────────────────── */}
          {activeSection === 'account' && (
            <>
              {/* My profile */}
              <div className={CARD}>
                <div className="flex items-center gap-3 mb-4.5">
                  <div className="w-11 h-11 rounded-full bg-accent-blue/20 flex items-center justify-center text-accent-blue text-base font-extrabold shrink-0">
                    {(session?.user?.name || session?.user?.email || '?').charAt(0).toUpperCase()}
                  </div>
                  <div>
                    <h2 className="text-[16px] font-bold text-text-primary">My profile</h2>
                    <p className="text-[12.5px] text-text-tertiary">{session?.user?.email}</p>
                  </div>
                </div>
                <div className="flex items-end gap-2.5">
                  <div className="flex-1 max-w-[380px] flex flex-col gap-1.5">
                    <label htmlFor="display-name" className="text-[12.5px] font-semibold text-text-secondary">Display name</label>
                    <input
                      id="display-name"
                      value={displayName}
                      onChange={e => setDisplayName(e.target.value)}
                      className={inputCls}
                    />
                  </div>
                  <button
                    onClick={() => updateProfileMut.mutate({ name: displayName })}
                    disabled={updateProfileMut.isPending}
                    className={PRIMARY_BTN}
                  >
                    {updateProfileMut.isPending ? 'Saving…' : profileSaved ? 'Saved!' : 'Save'}
                  </button>
                </div>
                {updateProfileMut.isError && (
                  <p className="text-[10px] text-status-error mt-2">Profile update not yet supported by backend</p>
                )}
              </div>

              {/* Password */}
              <div className={CARD}>
                <div className="mb-4">
                  <h2 className="text-[16px] font-bold text-text-primary">Password</h2>
                  <p className="text-[12.5px] text-text-tertiary mt-0.5">Minimum {minPasswordLength} characters per organization policy.</p>
                </div>
                <form onSubmit={handleChangePassword} className="space-y-4">
                  <div className="grid gap-3.5" style={{ gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))' }}>
                    <div className="space-y-1.5">
                      <label htmlFor="current-password" className="text-[12.5px] font-semibold text-text-secondary">Current password</label>
                      <input
                        id="current-password"
                        type="password"
                        value={currentPassword}
                        onChange={e => setCurrentPassword(e.target.value)}
                        autoComplete="current-password"
                        placeholder="••••••••"
                        className={inputCls}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <label htmlFor="new-password" className="text-[12.5px] font-semibold text-text-secondary">New password</label>
                      <input
                        id="new-password"
                        type="password"
                        value={newPassword}
                        onChange={e => setNewPassword(e.target.value)}
                        autoComplete="new-password"
                        placeholder="••••••••"
                        className={inputCls}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <label htmlFor="confirm-password" className="text-[12.5px] font-semibold text-text-secondary">Confirm new password</label>
                      <input
                        id="confirm-password"
                        type="password"
                        value={confirmPassword}
                        onChange={e => setConfirmPassword(e.target.value)}
                        autoComplete="new-password"
                        placeholder="••••••••"
                        className={inputCls}
                      />
                    </div>
                  </div>
                  {passwordError && <p className="text-[10px] text-status-error">{passwordError}</p>}
                  <div className="flex items-center gap-3">
                    <button
                      type="submit"
                      disabled={changePasswordMut.isPending || !currentPassword || !newPassword || !confirmPassword}
                      className={NEUTRAL_BTN}
                    >
                      {changePasswordMut.isPending ? 'Saving…' : passwordSaved ? 'Saved!' : 'Update password'}
                    </button>
                  </div>
                </form>
              </div>

              {/* My API key */}
              <div className={CARD}>
                <div className="mb-3.5">
                  <h2 className="text-[16px] font-bold text-text-primary">My API key</h2>
                  <p className="text-[12.5px] text-text-tertiary mt-0.5">
                    Your session is managed via a secure HttpOnly cookie. The API key is only used for agent/programmatic access.
                  </p>
                </div>
                {/*
                  The mockup shows a persistently masked key (nxm_••••4f2a) with a copy
                  button. There is no backend endpoint to fetch/re-mask an existing key —
                  rotateKey() only returns a brand-new key once, at rotation time — so we
                  don't fabricate a masked value here and keep the reveal-once flow below.
                */}
                {newKey ? (
                  <div className="space-y-3">
                    <p className="text-xs text-text-tertiary">New key — copy it now, it won't be shown again.</p>
                    <div className="flex items-center gap-2 rounded-[11px] px-3.5 py-2.5 border border-white/[0.07] bg-[#0a0c11] font-mono">
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
                        className={`${NEUTRAL_BTN} flex-1`}
                      >
                        Cancel
                      </button>
                      <button
                        onClick={() => rotateMut.mutate()}
                        disabled={rotateMut.isPending}
                        className={`${PRIMARY_BTN} flex-1`}
                      >
                        {rotateMut.isPending ? 'Rotating…' : 'Rotate'}
                      </button>
                    </div>
                  </div>
                ) : (
                  <button
                    onClick={() => setRotateConfirm(true)}
                    className={NEUTRAL_BTN_SM}
                  >
                    <RefreshCw className="w-3 h-3" />
                    Rotate key
                  </button>
                )}
              </div>
            </>
          )}

          {/* ── Organization ──────────────────────────────────────────────── */}
          {activeSection === 'org' && (
            <>
              <div className={CARD}>
                <h2 className="text-[16px] font-bold text-text-primary mb-4">Organization</h2>
                <div className="grid gap-3.5 mb-4" style={{ gridTemplateColumns: 'repeat(auto-fit, minmax(240px, 1fr))' }}>
                  <div className="space-y-1.5">
                    <label htmlFor="org-name" className="text-[12.5px] font-semibold text-text-secondary">Name</label>
                    <input
                      id="org-name"
                      value={orgName}
                      onChange={e => setOrgName(e.target.value)}
                      readOnly={!isAdmin}
                      className={`${inputCls} ${!isAdmin ? 'opacity-50 cursor-not-allowed' : ''}`}
                    />
                  </div>
                  <div className="space-y-1.5">
                    <label htmlFor="org-slug" className="text-[12.5px] font-semibold text-text-secondary">Slug</label>
                    <input id="org-slug" value={org?.slug ?? ''} readOnly className={`${inputCls} opacity-50 cursor-not-allowed font-mono`} />
                  </div>
                  <div className="space-y-1.5">
                    <label htmlFor="org-created" className="text-[12.5px] font-semibold text-text-secondary">Created</label>
                    <input
                      id="org-created"
                      value={org ? new Date(org.created_at).toLocaleDateString() : ''}
                      readOnly
                      className={`${inputCls} opacity-50 cursor-not-allowed`}
                    />
                  </div>
                </div>
                {isAdmin && (
                  <div className="flex items-center gap-3 mb-4">
                    <button
                      onClick={() => updateOrgMut.mutate(orgName)}
                      disabled={updateOrgMut.isPending || orgName === org?.name}
                      className={PRIMARY_BTN}
                    >
                      {updateOrgMut.isPending ? 'Saving…' : orgSaved ? 'Saved!' : 'Save'}
                    </button>
                    {updateOrgMut.isError && (
                      <p className="text-[10px] text-status-error">Failed to save.</p>
                    )}
                  </div>
                )}
                {isAdmin && (
                  <div className="pt-4 border-t border-white/[0.06] space-y-1.5">
                    <label className="text-[12.5px] font-semibold text-text-secondary">Logo URL</label>
                    <div className="flex items-end gap-2.5">
                      <div className="flex-1 max-w-[440px]">
                        <input
                          value={logoUrl}
                          onChange={e => setLogoUrl(e.target.value)}
                          placeholder="https://example.com/logo.png"
                          className={inputCls}
                        />
                      </div>
                      <button
                        onClick={() => updateLogoMut.mutate(logoUrl.trim() || null)}
                        disabled={updateLogoMut.isPending}
                        className={PRIMARY_BTN}
                      >
                        {updateLogoMut.isPending ? 'Saving…' : 'Save'}
                      </button>
                      {logoSaved && <span className="text-[10px] text-status-success">Saved</span>}
                    </div>
                    {logoUrl && (
                      <img
                        src={logoUrl}
                        className="w-8 h-8 rounded-full object-cover border border-white/[0.09] mt-2"
                        alt="org logo preview"
                      />
                    )}
                  </div>
                )}
              </div>

              {isAdmin && (
                <div className={CARD}>
                  <div className="mb-3.5">
                    <h2 className="text-[16px] font-bold text-text-primary">Announcement banner</h2>
                    <p className="text-[12.5px] text-text-tertiary mt-0.5">
                      Shown above the admin UI for all users. Leave blank to hide the banner.
                    </p>
                  </div>

                  {announcementText.trim() && (
                    <div className={cn(
                      'w-full px-4 py-2.5 mb-3.5 rounded-[10px] text-xs flex items-center gap-2',
                      announcementType === 'error'
                        ? 'bg-status-error/10 text-status-error border border-status-error/20'
                        : announcementType === 'warning'
                        ? 'bg-status-warning/10 text-status-warning border border-status-warning/20'
                        : 'bg-accent-blue/10 text-accent-blue border border-accent-blue/20',
                    )}>
                      <AlertCircle className="w-3 h-3 shrink-0" />
                      <span>{announcementText}</span>
                    </div>
                  )}

                  <textarea
                    value={announcementText}
                    onChange={e => setAnnouncementText(e.target.value)}
                    placeholder="e.g. Scheduled maintenance on Saturday 2 AM UTC. Expect ~30 min downtime."
                    className="w-full min-h-[72px] px-3.5 py-3 rounded-[11px] border border-white/[0.09] bg-white/[0.03] text-text-primary text-[13px] leading-[1.55] placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 resize-y transition-colors"
                  />

                  <div className="flex items-center gap-2.5 mt-3.5 flex-wrap">
                    <div className="inline-flex items-center p-[3px] rounded-[10px] border border-white/[0.08] bg-[#0d0f14]/70">
                      {(['info', 'warning', 'error'] as const).map(level => {
                        const dotColor = level === 'error' ? '#f87171' : level === 'warning' ? '#facc15' : '#7aa2ff'
                        const active = announcementType === level
                        return (
                          <button
                            key={level}
                            type="button"
                            onClick={() => setAnnouncementType(level)}
                            className={cn(
                              'flex items-center gap-1.5 px-3.5 py-1.5 rounded-[8px] text-[12px] font-semibold transition-colors',
                              active ? 'bg-white/[0.07] text-text-primary' : 'text-text-tertiary hover:text-text-secondary',
                            )}
                          >
                            <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: dotColor }} />
                            {level === 'error' ? 'Critical' : level.charAt(0).toUpperCase() + level.slice(1)}
                          </button>
                        )
                      })}
                    </div>
                    <div className="flex-1" />
                    <button
                      onClick={() => updateAnnouncementMut.mutate({ text: announcementText.trim(), type: announcementType })}
                      disabled={updateAnnouncementMut.isPending}
                      className={PRIMARY_BTN}
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
                    {announcementSaved && <span className="text-[10px] text-status-success">Saved ✓</span>}
                  </div>
                </div>
              )}

              {isAdmin && (
                <div className="grid gap-4" style={{ gridTemplateColumns: 'repeat(auto-fit, minmax(300px, 1fr))' }}>
                  <div className={CARD}>
                    <h2 className="text-[15px] font-bold text-text-primary">Data retention</h2>
                    <p className="text-[12.5px] text-text-tertiary mt-0.5 mb-3.5 leading-[1.5]">
                      Automatically delete memories older than the selected period. "Never" keeps all memories.
                    </p>
                    <div className="flex items-center gap-2.5">
                      <select
                        value={retentionDays ?? ''}
                        onChange={(e) => setRetentionDays(e.target.value ? parseInt(e.target.value) : null)}
                        className={`${selectCls} flex-1`}
                      >
                        <option value="">Never (keep all)</option>
                        <option value="30">30 days</option>
                        <option value="60">60 days</option>
                        <option value="90">90 days</option>
                        <option value="180">180 days</option>
                        <option value="365">1 year</option>
                      </select>
                      <button
                        onClick={() => updateRetentionMut.mutate(retentionDays)}
                        disabled={updateRetentionMut.isPending}
                        className={PRIMARY_BTN}
                      >
                        {updateRetentionMut.isPending ? 'Saving…' : 'Save'}
                      </button>
                    </div>
                    {updateRetentionMut.isSuccess && (
                      <span className="text-[10px] text-status-success mt-1.5 block">Saved</span>
                    )}
                    {orgSettings?.retention_days && (
                      <p className="text-xs text-text-quaternary mt-1.5">
                        {retentionPreview ? `${retentionPreview.would_delete} memories would be deleted with current settings` : '…'}
                      </p>
                    )}
                  </div>

                  <div className={CARD}>
                    <h2 className="text-[15px] font-bold text-text-primary">Password policy</h2>
                    <p className="text-[12.5px] text-text-tertiary mt-0.5 mb-3.5 leading-[1.5]">
                      Minimum character length for all passwords in this organization.
                    </p>
                    <div className="flex items-center gap-2.5">
                      <div className="flex-1 flex items-center gap-3">
                        <input
                          type="range"
                          min={6}
                          max={24}
                          value={minPasswordLength}
                          onChange={e => setMinPasswordLength(Number(e.target.value))}
                          className="flex-1 accent-accent-blue cursor-pointer"
                        />
                        <span className="text-sm font-extrabold text-accent-blue tabular-nums w-16 shrink-0">{minPasswordLength} chars</span>
                      </div>
                      <button
                        onClick={() => updatePasswordPolicyMut.mutate(minPasswordLength)}
                        disabled={updatePasswordPolicyMut.isPending}
                        className={PRIMARY_BTN}
                      >
                        {updatePasswordPolicyMut.isPending ? 'Saving…' : 'Save'}
                      </button>
                    </div>
                    {passwordPolicySaved && <span className="text-[10px] text-status-success mt-1.5 block">Saved ✓</span>}
                  </div>
                </div>
              )}
            </>
          )}

          {/* ── Agents ────────────────────────────────────────────────────── */}
          {activeSection === 'agents' && (
            <>
              {isAdmin && (
                <div className={CARD}>
                  <div className="mb-3.5">
                    <h2 className="text-[16px] font-bold text-text-primary">Agent instructions</h2>
                    <p className="text-[12.5px] text-text-tertiary mt-0.5">
                      System-level instructions added to every agent's context for this organization.
                      Use this to set team conventions, coding standards, or custom behavior.
                    </p>
                  </div>
                  <textarea
                    value={customInstructions}
                    onChange={e => setCustomInstructions(e.target.value)}
                    rows={5}
                    placeholder="e.g. Always use TypeScript strict mode. Prefer functional components. Follow our naming conventions…"
                    className="w-full min-h-[120px] px-3.5 py-3 rounded-[11px] border border-white/[0.09] bg-white/[0.03] text-text-primary text-[13px] leading-[1.6] font-mono placeholder:text-text-quaternary focus:outline-none focus:border-accent-blue/60 resize-y transition-colors"
                  />
                  <div className="flex items-center gap-3 mt-3.5">
                    <button
                      onClick={() => updateInstructionsMut.mutate(customInstructions.trim() || null)}
                      disabled={updateInstructionsMut.isPending}
                      className={PRIMARY_BTN}
                    >
                      {updateInstructionsMut.isPending ? 'Saving…' : 'Save instructions'}
                    </button>
                    {instructionsSaved && <span className="text-[10px] text-status-success">Saved ✓</span>}
                  </div>
                </div>
              )}

              {isAdmin && (
                <div className={CARD}>
                  <div className="mb-2.5">
                    <h2 className="text-[16px] font-bold text-text-primary">
                      Agent events
                      {eventSaved && <span className="ml-2 text-status-success text-[10px] font-normal align-middle">Saved</span>}
                    </h2>
                    <p className="text-[12.5px] text-text-tertiary mt-0.5">
                      Control which GitHub events the agent reacts to automatically.
                    </p>
                  </div>
                  <div className="divide-y divide-white/[0.04]">
                    {([
                      { key: 'resolve_issues' as const, label: 'Resolve issues', description: 'Agent responds to newly opened GitHub issues.' },
                      { key: 'review_prs' as const, label: 'Review pull requests', description: 'Auto-review of PRs when opened or updated.' },
                      { key: 'respond_comments' as const, label: 'Respond to comments', description: 'Agent replies to issue and PR review comments.' },
                      { key: 'auto_index' as const, label: 'Auto-index on push', description: 'Trigger code indexing jobs on every push.' },
                      { key: 'scanner' as const, label: 'Proactive scanner', description: 'Periodically scans for issues without being invoked.' },
                    ] as { key: keyof AgentEventSettings; label: string; description: string }[]).map(({ key, label, description }) => {
                      const { Icon, color, bg } = EVENT_ICONS[key]
                      return (
                        <div key={key} className="flex items-center gap-3.5 py-3.5 first:pt-0 last:pb-0">
                          <div className="w-9 h-9 rounded-[10px] flex items-center justify-center shrink-0" style={{ background: bg }}>
                            <Icon className="w-4 h-4" style={{ color }} strokeWidth={1.7} />
                          </div>
                          <div className="flex-1 min-w-0">
                            <p className="text-[13.5px] font-bold text-text-primary">{label}</p>
                            <p className="text-xs text-text-tertiary mt-0.5">{description}</p>
                          </div>
                          <Switch
                            checked={eventSettings[key]}
                            onCheckedChange={() => handleEventToggle(key)}
                            size="sm"
                          />
                        </div>
                      )
                    })}
                  </div>
                </div>
              )}

              <MemoryTemplatesSection />
            </>
          )}

          {/* ── Integrations & data ──────────────────────────────────────── */}
          {activeSection === 'integrations' && isAdmin && (
            <>
              <div className={CARD}>
                <div className="flex items-center justify-between gap-3 mb-3.5">
                  <div>
                    <h2 className="text-[16px] font-bold text-text-primary">Webhooks</h2>
                    <p className="text-[12.5px] text-text-tertiary mt-0.5">GitHub endpoints for this organization.</p>
                  </div>
                  {!showAddWebhook && !webhooksLoading && (
                    <button onClick={() => setShowAddWebhook(true)} className={NEUTRAL_BTN_SM}>
                      <Plus className="w-3 h-3" />
                      Add webhook
                    </button>
                  )}
                </div>

                {webhooksLoading && (
                  <div className="space-y-3">
                    <div className="animate-pulse h-16 bg-white/[0.06] rounded-[14px]" />
                    <div className="animate-pulse h-16 bg-white/[0.06] rounded-[14px]" />
                  </div>
                )}

                {!webhooksLoading && webhooks.length > 0 && (
                  <div className="space-y-3">
                    {webhooks.map(wh => (
                      <div key={wh.id} className="border border-white/[0.07] rounded-[14px] p-4 space-y-3">
                        <div className="flex items-center justify-between gap-3">
                          <p className="text-xs font-semibold text-text-primary truncate">{wh.name}</p>
                          <Switch
                            checked={wh.active}
                            onCheckedChange={(checked) =>
                              updateWebhookMut.mutate({ id: wh.id, data: { active: checked } })
                            }
                            size="sm"
                          />
                        </div>
                        <p className="text-xs font-mono text-text-tertiary truncate">{wh.target_url}</p>
                        <div className="flex flex-wrap gap-1">
                          {wh.events.map(ev => (
                            <span
                              key={ev}
                              className="rounded-[5px] px-1.5 py-0.5 text-[10px] font-semibold bg-white/[0.06] border border-white/[0.07] text-text-tertiary"
                            >
                              {ev}
                            </span>
                          ))}
                        </div>
                        <div className="flex items-center justify-between gap-3">
                          <span className="text-[11px] text-text-quaternary">
                            Created {new Date(wh.created_at).toLocaleDateString()}
                          </span>
                          <div className="flex items-center gap-2">
                            {testStates[wh.id]?.result && (
                              testStates[wh.id].result!.success
                                ? <span className="text-[10px] text-status-success">✓ {testStates[wh.id].result!.status_code}</span>
                                : <span className="text-[10px] text-status-error">✗ {testStates[wh.id].result!.error}</span>
                            )}
                            <button
                              onClick={() => handleTestWebhook(wh.id)}
                              disabled={!!testStates[wh.id]?.testing}
                              className="border border-white/[0.09] rounded-[8px] px-2.5 py-1 text-xs text-text-secondary hover:text-text-primary transition-colors disabled:opacity-40"
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

                {!webhooksLoading && webhooks.length === 0 && !showAddWebhook && (
                  <div className="flex flex-col items-center gap-1.5 py-6 px-6 rounded-[12px] border-[1.5px] border-dashed border-white/[0.1]">
                    <p className="text-[13.5px] font-bold text-text-secondary">No webhooks configured</p>
                    <p className="text-xs text-text-quaternary">Add a webhook to receive GitHub events.</p>
                  </div>
                )}

                {showAddWebhook && (
                  <form onSubmit={handleAddWebhook} className="space-y-3 mt-3.5">
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
                        <label className="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
                          <input
                            type="checkbox"
                            checked={webhookEvents.includes('*')}
                            onChange={e => handleWebhookEventsChange('*', e.target.checked)}
                            className="accent-accent-blue"
                          />
                          All events (*)
                        </label>
                        {WEBHOOK_EVENTS.map(ev => (
                          <label key={ev} className="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
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
                    {webhookError && <p className="text-[10px] text-status-error">{webhookError}</p>}
                    <div className="flex items-center gap-2">
                      <button
                        type="button"
                        onClick={() => { setShowAddWebhook(false); setWebhookError('') }}
                        className={NEUTRAL_BTN}
                      >
                        Cancel
                      </button>
                      <button
                        type="submit"
                        disabled={createWebhookMut.isPending}
                        className={PRIMARY_BTN}
                      >
                        {createWebhookMut.isPending ? 'Saving…' : 'Save webhook'}
                      </button>
                    </div>
                  </form>
                )}
              </div>

              <div className={CARD}>
                <h2 className="text-[16px] font-bold text-text-primary mb-1">Org data export</h2>
                <p className="text-[12.5px] text-text-tertiary mb-4">
                  Download your organization's data as JSON for backup or migration.
                </p>
                <div className="flex flex-wrap gap-2">
                  <button onClick={handleExportMemories} disabled={exportingMemories} className={NEUTRAL_BTN_SM}>
                    <Download className="w-3 h-3" />
                    {exportingMemories ? 'Exporting…' : 'Export memories'}
                  </button>
                  <button onClick={handleExportConventions} disabled={exportingConventions} className={NEUTRAL_BTN_SM}>
                    <Download className="w-3 h-3" />
                    {exportingConventions ? 'Exporting…' : 'Export conventions'}
                  </button>
                  <button
                    onClick={handleExportAllData}
                    disabled={exportingAll}
                    className="h-9 px-4 rounded-[10px] bg-accent-blue hover:bg-accent-blue-hover text-white text-xs font-bold transition-colors flex items-center gap-1.5 disabled:opacity-40"
                  >
                    <Download className="w-3 h-3" />
                    {exportingAll ? 'Preparing…' : 'Export all data'}
                  </button>
                </div>
              </div>

              <div className="rounded-[16px] border border-status-error/25 bg-gradient-to-br from-status-error/[0.05] via-status-error/[0.01] to-transparent backdrop-blur-[12px] p-[22px]">
                <h2 className="text-[16px] font-bold text-status-error/90 mb-2">Danger zone</h2>
                <div className="flex items-center justify-between gap-3 py-3.5 border-b border-status-error/10">
                  <div className="min-w-0">
                    <p className="text-[13.5px] font-bold text-text-primary">Export all data</p>
                    <p className="text-xs text-text-tertiary mt-0.5">All memories, users, and audit logs as JSON.</p>
                  </div>
                  <button onClick={handleExportAll} className="border border-white/[0.09] rounded-[9px] px-3.5 py-2 text-xs font-semibold text-text-secondary hover:text-text-primary hover:bg-white/[0.06] transition-colors shrink-0">
                    Export
                  </button>
                </div>
                <div className="flex items-center justify-between gap-3 py-3.5">
                  <div className="min-w-0">
                    <p className="text-[13.5px] font-bold text-text-primary">Org config</p>
                    <p className="text-xs text-text-tertiary mt-0.5">Settings, webhooks, and project list as JSON.</p>
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    <button
                      onClick={handleImportConfig}
                      className="border border-white/[0.09] rounded-[9px] px-3.5 py-2 text-xs font-semibold text-text-secondary hover:text-text-primary hover:bg-white/[0.06] transition-colors flex items-center gap-1.5"
                    >
                      <Upload className="w-3 h-3" /> Import
                    </button>
                    <button
                      onClick={() => client.exportOrgConfig().then(blob => downloadBlob(blob, 'nexusmind-config.json'))}
                      className="border border-white/[0.09] rounded-[9px] px-3.5 py-2 text-xs font-semibold text-text-secondary hover:text-text-primary hover:bg-white/[0.06] transition-colors flex items-center gap-1.5"
                    >
                      <Download className="w-3 h-3" /> Export
                    </button>
                  </div>
                </div>
                {importFlash && (
                  <p className={`text-[10px] mt-1 ${importFlash.type === 'success' ? 'text-status-success' : importFlash.type === 'warning' ? 'text-status-warning' : 'text-status-error'}`}>
                    {importFlash.type === 'success' ? '✓ ' : ''}{importFlash.message}
                  </p>
                )}
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  )
}
