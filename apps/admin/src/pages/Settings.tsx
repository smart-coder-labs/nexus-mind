import { useMemo, useState, useEffect } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'

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

  const inputCls = 'w-full bg-transparent border border-border-primary rounded-lg px-3 py-2.5 text-sm text-text-primary placeholder:text-text-quaternary focus:outline-none focus:border-border-focus transition-colors'

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
            <label className="text-xs text-text-tertiary">Name</label>
            <input
              value={orgName}
              onChange={e => setOrgName(e.target.value)}
              readOnly={session?.user.role !== 'admin'}
              className={`${inputCls} ${session?.user.role !== 'admin' ? 'opacity-50 cursor-not-allowed' : ''}`}
            />
          </div>
          <div className="space-y-1.5">
            <label className="text-xs text-text-tertiary">Slug</label>
            <input value={org?.slug ?? ''} readOnly className={`${inputCls} opacity-50 cursor-not-allowed`} />
          </div>
          <div className="space-y-1.5">
            <label className="text-xs text-text-tertiary">Created</label>
            <input
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
                className="px-4 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-normal disabled:opacity-30 transition-colors"
              >
                {updateOrgMut.isPending ? 'Saving…' : orgSaved ? 'Saved!' : 'Save'}
              </button>
              {updateOrgMut.isError && (
                <p className="text-xs text-status-error/70">Failed to save.</p>
              )}
            </div>
          )}
        </div>
      </section>

      {/* Password */}
      <section className="space-y-4">
        <p className="text-text-tertiary text-[12px] tracking-[-0.12px]">Password</p>
        <div className="border border-border-primary rounded-[18px] p-5">
          <form onSubmit={handleChangePassword} className="space-y-4">
            <div className="space-y-1.5">
              <label className="text-xs text-text-tertiary">Current password</label>
              <input
                type="password"
                value={currentPassword}
                onChange={e => setCurrentPassword(e.target.value)}
                autoComplete="current-password"
                className={inputCls}
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-xs text-text-tertiary">New password</label>
              <input
                type="password"
                value={newPassword}
                onChange={e => setNewPassword(e.target.value)}
                autoComplete="new-password"
                className={inputCls}
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-xs text-text-tertiary">Confirm new password</label>
              <input
                type="password"
                value={confirmPassword}
                onChange={e => setConfirmPassword(e.target.value)}
                autoComplete="new-password"
                className={inputCls}
              />
            </div>
            {passwordError && <p className="text-xs text-status-error/70">{passwordError}</p>}
            <div className="flex items-center gap-3">
              <button
                type="submit"
                disabled={changePasswordMut.isPending || !currentPassword || !newPassword || !confirmPassword}
                className="px-4 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-normal disabled:opacity-30 transition-colors"
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
          <div className="flex items-center gap-3 bg-surface-secondary rounded-lg px-3 py-2">
            <code className="flex-1 text-xs text-text-tertiary truncate">
              Session managed via secure HttpOnly cookie
            </code>
          </div>

          {newKey ? (
            <div className="space-y-3">
              <p className="text-xs text-text-tertiary">New key — copy it now, it won't be shown again.</p>
              <div className="flex items-center gap-2 bg-surface-secondary rounded-lg px-3 py-2">
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
                  className="flex-1 py-2 rounded-lg border border-border-primary text-sm text-text-tertiary hover:text-text-secondary transition-colors"
                >
                  Cancel
                </button>
                <button
                  onClick={() => rotateMut.mutate()}
                  disabled={rotateMut.isPending}
                  className="flex-1 py-2 rounded-full bg-accent-blue hover:bg-accent-blue-hover text-white text-sm font-normal disabled:opacity-40 transition-colors"
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
                className="text-xs text-text-tertiary hover:text-text-secondary border border-border-primary rounded-lg px-3 py-1.5 hover:bg-surface-secondary transition-colors"
              >
                Export
              </button>
            </div>
          </div>
        </section>
      )}
    </div>
  )
}
