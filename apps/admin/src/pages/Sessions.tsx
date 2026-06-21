import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { MessageSquare, Plus, Trash2, ChevronRight, ExternalLink } from 'lucide-react'
import { createClient } from '../api/client'

function formatDuration(start: string, end?: string | null): string {
  const ms = new Date(end ?? new Date()).getTime() - new Date(start).getTime()
  const h = Math.floor(ms / 3_600_000)
  const m = Math.floor((ms % 3_600_000) / 60_000)
  if (h > 0) return `${h}h ${m}m`
  if (m > 0) return `${m}m`
  return '< 1m'
}

const client = createClient()

export default function Sessions() {
  const navigate = useNavigate()
  const qc = useQueryClient()
  const [creating, setCreating] = useState(false)
  const [newSessionName, setNewSessionName] = useState('')
  const [expandedId, setExpandedId] = useState<string | null>(null)

  const { data: sessions = [], isLoading } = useQuery({
    queryKey: ['sessions'],
    queryFn: () => client.listSessions({ limit: 50 }),
  })

  const createMut = useMutation({
    mutationFn: (summary: string) => client.createSession({ summary }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sessions'] })
      setCreating(false)
      setNewSessionName('')
    },
  })

  const deleteMut = useMutation({
    mutationFn: (id: string) => client.deleteSession(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['sessions'] }),
  })

  const { data: sessionMemories } = useQuery({
    queryKey: ['session-memories', expandedId],
    queryFn: () => client.getSessionMemories(expandedId!),
    enabled: !!expandedId,
  })

  return (
    <div className="p-6 max-w-4xl">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-base font-semibold text-text-primary">Sessions</h1>
          <p className="text-xs text-text-quaternary mt-0.5">{sessions.length} sessions</p>
        </div>
        <button
          onClick={() => setCreating(true)}
          className="bg-accent-blue text-white rounded-full px-4 py-1.5 text-xs font-semibold flex items-center gap-1.5"
        >
          <Plus className="w-3.5 h-3.5" />
          New Session
        </button>
      </div>

      {/* Create inline */}
      {creating && (
        <div className="mb-4 flex items-center gap-2 rounded-[11px] border border-accent-blue/30 bg-accent-blue/[0.06] p-3">
          <input
            autoFocus
            value={newSessionName}
            onChange={e => setNewSessionName(e.target.value)}
            onKeyDown={e => {
              if (e.key === 'Enter') createMut.mutate(newSessionName)
              if (e.key === 'Escape') setCreating(false)
            }}
            placeholder="Session summary…"
            className="flex-1 bg-transparent text-xs text-text-primary placeholder:text-text-quaternary focus:outline-none"
          />
          <button
            onClick={() => createMut.mutate(newSessionName)}
            disabled={!newSessionName}
            className="text-accent-blue text-xs font-semibold disabled:opacity-40"
          >
            Create
          </button>
          <button onClick={() => setCreating(false)} className="text-text-quaternary hover:text-text-primary text-xs">
            Cancel
          </button>
        </div>
      )}

      {/* Session list */}
      {isLoading ? (
        <div className="space-y-2">
          {[...Array(5)].map((_, i) => (
            <div key={i} className="rounded-[18px] bg-[#272729] border border-border-primary h-16 animate-pulse" />
          ))}
        </div>
      ) : sessions.length === 0 ? (
        <div className="text-center py-16 text-xs text-text-quaternary">No sessions yet</div>
      ) : (
        <div className="space-y-2">
          {sessions.map((session: any) => (
            <div key={session.id} className="rounded-[18px] bg-[#272729] border border-border-primary">
              <div
                className="flex items-center gap-3 p-4 cursor-pointer hover:bg-white/[0.04] rounded-[18px] transition-colors group"
                onClick={() => setExpandedId(expandedId === session.id ? null : session.id)}
              >
                <MessageSquare className="w-4 h-4 text-text-quaternary shrink-0" />
                <div className="flex-1 min-w-0">
                  <p className="text-xs font-semibold text-text-primary truncate">
                    {session.summary || 'Untitled Session'}
                  </p>
                  <p className="text-[10px] text-text-quaternary mt-0.5">
                    {session.memory_count ?? 0} memories
                    {session.started_at ? ` · ${new Date(session.started_at).toLocaleDateString()}` : ''}
                    {session.started_at ? ` · ${formatDuration(session.started_at, session.ended_at)}` : ''}
                  </p>
                </div>
                <button
                  onClick={e => {
                    e.stopPropagation()
                    navigate(`/memories?session_id=${session.id}`)
                  }}
                  className="opacity-0 group-hover:opacity-100 transition-opacity flex items-center gap-1 text-[10px] text-accent-blue hover:text-accent-blue/80"
                >
                  <ExternalLink className="w-3 h-3" />
                  View memories
                </button>
                <button
                  onClick={e => {
                    e.stopPropagation()
                    deleteMut.mutate(session.id)
                  }}
                  className="opacity-0 group-hover:opacity-100 text-text-quaternary hover:text-status-error transition-all"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                </button>
                <ChevronRight
                  className={`w-3.5 h-3.5 text-text-quaternary transition-transform ${expandedId === session.id ? 'rotate-90' : ''}`}
                />
              </div>
              {expandedId === session.id && (
                <div className="px-4 pb-4 space-y-1.5 border-t border-border-primary mt-3 pt-3">
                  {(sessionMemories ?? []).length === 0 ? (
                    <p className="text-[10px] text-text-quaternary py-2">No memories in this session</p>
                  ) : (
                    (sessionMemories ?? []).map((m: any) => (
                      <div key={m.id} className="rounded-[8px] bg-white/[0.04] p-2.5">
                        <p className="text-xs text-text-secondary leading-relaxed">
                          {m.content?.slice(0, 200)}
                          {(m.content?.length ?? 0) > 200 ? '…' : ''}
                        </p>
                        {m.tags?.length > 0 && (
                          <div className="flex gap-1 mt-1.5 flex-wrap">
                            {m.tags.map((t: string) => (
                              <span
                                key={t}
                                className="rounded-full bg-white/[0.06] px-2 py-0.5 text-[10px] text-text-secondary"
                              >
                                {t}
                              </span>
                            ))}
                          </div>
                        )}
                      </div>
                    ))
                  )}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
