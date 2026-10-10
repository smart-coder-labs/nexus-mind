import { useEffect, useMemo, useRef } from 'react'
import { Loader2, Wrench } from 'lucide-react'
import { Markdown } from '../../../components/ui/Markdown'
import type { AutonomousAgentEvent } from '../../../types'
import { asArr, asDict, asStr } from '../shared/format'

// ── Conversation transcript → readable chat ──
// Distils the raw Claude stream-json turns into human-readable chat messages:
// assistant prose, a one-line note per tool call, tool errors, and the final
// result. System/init turns and non-error tool outputs are dropped as noise.
function toolResultText(content: unknown): string {
  if (typeof content === 'string') return content
  const arr = asArr(content)
  if (arr) return arr.map(x => { const d = asDict(x); return asStr(d?.text) ?? (d?.type === 'image' ? '[image]' : JSON.stringify(x)) }).join('\n')
  return content == null ? '' : JSON.stringify(content, null, 2)
}

type ChatMsg = { key: string; kind: 'assistant' | 'tool' | 'tool-error'; tool?: string; markdown?: string; pretty?: string; plain?: string; badge?: string }

// The agent's messages are often the JSON output contract (e.g.
// {"no_op":true,"comment":"…"} or {"summary":…,"findings":[…]}). Rendered raw
// they're unreadable, so we lift the human field (comment/summary/result) and
// render it as markdown; JSON without one is pretty-printed, plain prose is kept.
function decodeAssistantText(text: string): Pick<ChatMsg, 'markdown' | 'pretty' | 'plain' | 'badge'> {
  const trimmed = text.trim()
  if (trimmed.startsWith('{') || trimmed.startsWith('```')) {
    const unfenced = trimmed.replace(/^```(?:json)?\s*/i, '').replace(/\s*```$/, '').trim()
    try {
      const obj = asDict(JSON.parse(unfenced))
      if (obj) {
        const md = asStr(obj.comment) ?? asStr(obj.summary) ?? asStr(obj.result)
        const findings = asArr(obj.findings)
        const badge = obj.no_op === true ? 'No changes'
          : findings ? `${findings.length} finding${findings.length === 1 ? '' : 's'}`
          : asStr(obj.title) ? 'Proposed change'
          : undefined
        if (md && md.trim()) return { markdown: md, badge }
        return { pretty: JSON.stringify(obj, null, 2), badge }
      }
    } catch { /* not JSON after all — fall through to plain */ }
  }
  return { plain: text }
}

function toChatMessages(turns: AutonomousAgentEvent[]): ChatMsg[] {
  const out: ChatMsg[] = []
  for (const turn of turns) {
    const p = asDict(turn.payload) ?? {}
    const type = asStr(p.type) ?? turn.kind
    if (type !== 'assistant' && type !== 'user') continue // drop system/init and result (dup of the final assistant message)
    const content = asArr(asDict(p.message)?.content) ?? []
    content.forEach((block, i) => {
      const b = asDict(block) ?? {}
      const bt = asStr(b.type)
      const key = `${turn.sequence}-${i}`
      if (bt === 'text' && asStr(b.text)?.trim()) out.push({ key, kind: 'assistant', ...decodeAssistantText(asStr(b.text)!) })
      else if (bt === 'tool_use') out.push({ key, kind: 'tool', tool: asStr(b.name) ?? 'tool' })
      else if (bt === 'tool_result' && b.is_error === true) out.push({ key, kind: 'tool-error', plain: toolResultText(b.content).slice(0, 800) })
    })
  }
  return out
}

function ChatRow({ m }: { m: ChatMsg }) {
  if (m.kind === 'assistant') return (
    <div className="rounded-md border border-border-primary bg-foreground/[0.04] px-3 py-2">
      {m.badge && <span className="mb-1.5 inline-block rounded-md bg-foreground/[0.06] px-2 py-0.5 text-[11px] font-medium text-text-tertiary">{m.badge}</span>}
      {m.markdown != null
        ? <div className="text-sm leading-relaxed text-text-primary [&_*]:!my-1 [&_h1]:text-[15px] [&_h2]:text-[15px] [&_h3]:text-sm [&_table]:text-[12px]"><Markdown content={m.markdown} /></div>
        : m.pretty != null
          ? <pre className="overflow-x-auto rounded-md bg-muted p-2 font-mono text-[11px] leading-relaxed text-text-secondary">{m.pretty}</pre>
          : <div className="whitespace-pre-wrap break-words text-sm leading-relaxed text-text-primary">{m.plain}</div>}
    </div>
  )
  if (m.kind === 'tool') return <div className="flex items-center gap-1.5 pl-1 text-[12px] text-text-tertiary"><Wrench className="h-3 w-3" aria-hidden /> Used <span className="font-mono text-text-secondary">{m.tool}</span></div>
  if (m.kind === 'tool-error') return <div className="whitespace-pre-wrap break-all rounded-md border border-status-error/25 bg-status-error/[0.07] px-3 py-2 font-mono text-[12px] text-status-error">{m.plain}</div>
  return null
}

export function TranscriptView({ turns, live }: { turns: AutonomousAgentEvent[]; live: boolean }) {
  const messages = useMemo(() => toChatMessages(turns), [turns])
  const scrollRef = useRef<HTMLDivElement>(null)
  // Keep the newest message in view while the run streams.
  useEffect(() => {
    if (live && scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight
  }, [messages.length, live])
  if (!turns.length) return <p className="text-sm text-text-tertiary">{live ? 'Waiting for the agent to start…' : 'No conversation was recorded for this run.'}</p>
  return (
    <div ref={scrollRef} className="max-h-[520px] space-y-2 overflow-y-auto rounded-md border border-border-primary bg-muted/50 p-3">
      {messages.length ? messages.map(m => <ChatRow key={m.key} m={m} />) : <p className="text-sm text-text-tertiary">No readable messages yet.</p>}
      {live && <div role="status" className="flex items-center gap-2 pl-1 text-[12px] text-text-tertiary"><Loader2 className="h-3 w-3 animate-spin motion-reduce:animate-none" aria-hidden /> Streaming…</div>}
    </div>
  )
}
