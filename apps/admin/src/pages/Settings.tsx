import { useMemo, useState, useEffect } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useAuth } from '../auth/AuthContext'
import { createClient } from '../api/client'
import { saveSession } from '../auth/session'

export default function Settings() {
  const { session, setSession } = useAuth()
  const qc = useQueryClient()
  const client = useMemo(() => createClient(session!.apiKey), [session])

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
      saveSession(newSession)
      setOrgSaved(true)
      setTimeout(() => setOrgSaved(false), 2000)
    },
  })

  const [rotateConfirm, setRotateConfirm] = useState(false)
  const [newKey, setNewKey] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

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

  const inputCls = 'w-full bg-transparent border border-white/8 rounded-lg px-3 py-2.5 text-sm text-white placeholder:text-white/15 focus:outline-none focus:border-white/20 transition-colors'

  return (
    <div className="p-8 max-w-2xl mx-auto space-y-10">
      <div>
        <h1 className="text-lg font-semibold text-white">Settings</h1>
        <p className="text-[12px] text-white/30 mt-0.5">Organization and account configuration</p>
      </div>

      {/* Organization */}
      <section className="space-y-4">
        <p className="text-[11px] text-white/30 uppercase tracking-wide">Organization</p>
        <div className="border border-white/8 rounded-xl p-5 space-y-4">
          <div className="space-y-1.5">
            <label className="text-xs text-white/40">Name</label>
            <input
              value={orgName}
              onChange={e => setOrgName(e.target.value)}
              className={inputCls}
            />
          </div>
          <div className="space-y-1.5">
            <label className="text-xs text-white/40">Slug</label>
            <input value={org?.slug ?? ''} readOnly className={`${inputCls} opacity-40 cursor-not-allowed`} />
          </div>
          <div className="space-y-1.5">
            <label className="text-xs text-white/40">Created</label>
            <input
              value={org ? new Date(org.created_at).toLocaleDateString() : ''}
              readOnly
              className={`${inputCls} opacity-40 cursor-not-allowed`}
            />
          </div>
          <div className="flex items-center gap-3">
            <button
              onClick={() => updateOrgMut.mutate(orgName)}
              disabled={updateOrgMut.isPending || orgName === org?.name}
              className="px-4 py-2 rounded-lg bg-white text-[#0c0c0e] text-sm font-medium hover:bg-white/90 disabled:opacity-30 transition-colors"
            >
              {updateOrgMut.isPending ? 'Saving…' : orgSaved ? 'Saved!' : 'Save'}
            </button>
            {updateOrgMut.isError && (
              <p className="text-xs text-red-400/70">Failed to save.</p>
            )}
          </div>
        </div>
      </section>

      {/* My API Key */}
      <section className="space-y-4">
        <p className="text-[11px] text-white/30 uppercase tracking-wide">My API Key</p>
        <div className="border border-white/8 rounded-xl p-5 space-y-4">
          <div className="flex items-center gap-3 bg-white/5 rounded-lg px-3 py-2">
            <code className="flex-1 text-xs text-white/40 truncate">
              {session?.apiKey.slice(0, 12)}••••••••••••••••
            </code>
          </div>

          {newKey ? (
            <div className="space-y-3">
              <p className="text-xs text-white/40">New key — copy it now, it won't be shown again.</p>
              <div className="flex items-center gap-2 bg-white/5 rounded-lg px-3 py-2">
                <code className="flex-1 text-xs text-white/70 break-all">{newKey}</code>
                <button
                  onClick={() => { navigator.clipboard.writeText(newKey); setCopied(true); setTimeout(() => setCopied(false), 2000) }}
                  className="text-xs text-white/40 hover:text-white/70 transition-colors shrink-0"
                >
                  {copied ? 'Copied!' : 'Copy'}
                </button>
              </div>
              <button
                onClick={() => setNewKey(null)}
                className="text-xs text-white/30 hover:text-white/60 transition-colors"
              >
                Done
              </button>
            </div>
          ) : rotateConfirm ? (
            <div className="space-y-3">
              <p className="text-xs text-white/50">Your current key will stop working immediately. Continue?</p>
              <div className="flex gap-2">
                <button
                  onClick={() => setRotateConfirm(false)}
                  className="flex-1 py-2 rounded-lg border border-white/8 text-sm text-white/40 hover:text-white/60 transition-colors"
                >
                  Cancel
                </button>
                <button
                  onClick={() => rotateMut.mutate()}
                  disabled={rotateMut.isPending}
                  className="flex-1 py-2 rounded-lg bg-white text-[#0c0c0e] text-sm font-medium hover:bg-white/90 disabled:opacity-40 transition-colors"
                >
                  {rotateMut.isPending ? 'Rotating…' : 'Rotate'}
                </button>
              </div>
            </div>
          ) : (
            <button
              onClick={() => setRotateConfirm(true)}
              className="text-xs text-white/30 hover:text-white/60 transition-colors"
            >
              Rotate key
            </button>
          )}
        </div>
      </section>

      {/* Danger zone */}
      {session?.user.role === 'admin' && (
        <section className="space-y-4">
          <p className="text-[11px] text-white/30 uppercase tracking-wide">Danger Zone</p>
          <div className="border border-red-500/15 rounded-xl p-5 space-y-3">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-white/60 font-medium">Export all data</p>
                <p className="text-xs text-white/25 mt-0.5">Download all memories, users, and audit logs as JSON.</p>
              </div>
              <button
                onClick={handleExportAll}
                className="text-xs text-white/30 hover:text-white/60 border border-white/10 rounded-lg px-3 py-1.5 hover:bg-white/5 transition-colors"
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
