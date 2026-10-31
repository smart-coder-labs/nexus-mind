import { useEffect, useMemo, useRef, useState } from 'react'
import type { AutonomousAgentEvent } from '../../../types'
import { asArr, asDict, asStr, fmtTime } from '../shared/format'
import { toolResultText } from './Transcript'

type ToolLog = { key: string; name: string; time: string; input?: unknown; output?: string; error?: boolean; finished: boolean }

function toolLogs(events: AutonomousAgentEvent[]): ToolLog[] {
  const logs = new Map<string, ToolLog>()
  let session = ''
  const get = (id: string, name: string, time: string) => {
    let log = logs.get(id)
    if (!log) { log = { key: id, name, time, finished: false }; logs.set(id, log) }
    return log
  }
  for (const event of events) {
    const p = asDict(event.payload) ?? {}
    const type = asStr(p.type) ?? event.kind
    if (type === 'session') session = asStr(p.id) ?? String(event.sequence)
    if (type === 'tool_execution_start' || type === 'tool_execution_update' || type === 'tool_execution_end') {
      const log = get(`pi-${session}-${asStr(p.toolCallId) ?? event.sequence}`, asStr(p.toolName) ?? 'tool', event.created_at)
      if (type === 'tool_execution_start') log.input = p.args
      else {
        const result = asDict(type === 'tool_execution_end' ? p.result : p.partialResult)
        if (result?.content != null) log.output = toolResultText(result.content)
        if (type === 'tool_execution_end') { log.finished = true; log.error = p.isError === true }
      }
    } else if (type === 'assistant' || type === 'user') {
      for (const block of asArr(asDict(p.message)?.content) ?? []) {
        const b = asDict(block)
        if (b?.type === 'tool_use') {
          get(`claude-${asStr(b.id) ?? event.sequence}`, asStr(b.name) ?? 'tool', event.created_at).input = b.input
        } else if (b?.type === 'tool_result') {
          const log = get(`claude-${asStr(b.tool_use_id) ?? event.sequence}`, 'tool', event.created_at)
          log.output = toolResultText(b.content)
          log.error = b.is_error === true
          log.finished = true
        }
      }
    } else if (type === 'raw' || type === 'stderr') {
      const text = asStr(p.text)
      if (text) logs.set(`raw-${event.sequence}`, { key: `raw-${event.sequence}`, name: type === 'stderr' ? 'stderr' : 'Runtime output', time: event.created_at, output: text, finished: true })
    }
  }
  return [...logs.values()]
}

/** Bound log previews and omit binary image data; persisted payloads are already redacted by the worker. */
function preview(value: unknown, tail = false): string {
  const text = typeof value === 'string' ? value : JSON.stringify(value, (key, data) => key === 'data' && typeof data === 'string' ? '[binary data omitted]' : data, 2) ?? ''
  return text.length > 16_000 ? (tail ? `… earlier output omitted\n${text.slice(-16_000)}` : `${text.slice(0, 16_000)}\n… preview truncated`) : text
}

function ToolLogRow({ log, live, follow }: { log: ToolLog; live: boolean; follow: boolean }) {
  const [open, setOpen] = useState(false)
  const outputRef = useRef<HTMLPreElement>(null)
  useEffect(() => {
    if (follow && open && outputRef.current) outputRef.current.scrollTop = outputRef.current.scrollHeight
  }, [log.output, follow, open])
  const status = log.error ? 'Failed' : log.finished ? 'Completed' : live ? 'Running' : 'No result recorded'
  return (
    <details onToggle={event => setOpen(event.currentTarget.open)} className="rounded-md border border-border-secondary p-2">
      <summary className="cursor-pointer text-[12px] text-text-secondary">
        <time className="mr-2 text-text-tertiary tabular-nums">{fmtTime(log.time)}</time>
        <span className="font-mono">{log.name}</span> · <span className={log.error ? 'text-status-error' : undefined}>{status}</span>
      </summary>
      {open && <div className="mt-2 space-y-2">
        {log.input != null && <div><p className="text-[11px] text-text-tertiary">Input</p><pre className="overflow-x-auto whitespace-pre-wrap break-words text-[11px]">{preview(log.input)}</pre></div>}
        {log.output != null ? <div><p className="text-[11px] text-text-tertiary">Output</p><pre ref={outputRef} className={`max-h-80 overflow-auto whitespace-pre-wrap break-words text-[11px] ${log.error ? 'text-status-error' : 'text-text-secondary'}`}>{preview(log.output, true)}</pre></div>
          : <p className="text-[12px] text-text-tertiary">{live && !log.finished ? 'Waiting for tool output…' : 'No tool output was recorded.'}</p>}
      </div>}
    </details>
  )
}

function LifecycleRow({ event, description }: { event: AutonomousAgentEvent; description: string }) {
  const [open, setOpen] = useState(false)
  return (
    <li>
      <details onToggle={e => setOpen(e.currentTarget.open)}>
        <summary className="cursor-pointer text-sm text-text-secondary"><time className="mr-3 text-[12px] text-text-tertiary tabular-nums">{fmtTime(event.created_at)}</time>{description}</summary>
        {open && <pre className="mt-2 max-h-80 overflow-auto whitespace-pre-wrap break-words text-[11px] text-text-tertiary">{preview(event.payload)}</pre>}
      </details>
    </li>
  )
}

export function RunLogs({ events, transcript, live, describeEvent }: {
  events: AutonomousAgentEvent[]
  transcript: AutonomousAgentEvent[]
  live: boolean
  describeEvent: (event: AutonomousAgentEvent) => string
}) {
  const tools = useMemo(() => toolLogs(transcript), [transcript])
  const [visible, setVisible] = useState(100)
  const [following, setFollowing] = useState(true)
  const logRef = useRef<HTMLDivElement>(null)
  const shown = tools.slice(-visible)
  useEffect(() => {
    if (live && following && logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight
  }, [tools, live, following])
  return (
    <details className="border-t border-border-secondary pt-3">
      <summary className="cursor-pointer rounded-md text-[12px] text-text-tertiary">Event log ({events.length}) · Tool logs ({tools.length})</summary>
      {live && <div className="mt-2 space-y-1 text-[12px] text-text-tertiary">
        <p role="status">Updating logs while the run is active…</p>
        <label className="flex items-center gap-2"><input type="checkbox" checked={following} onChange={event => setFollowing(event.target.checked)} />Follow live logs</label>
      </div>}
      {events.length ? <ol aria-label="Lifecycle events" className="mt-3 max-h-80 list-none space-y-1.5 overflow-auto p-0">
        {events.map(event => <LifecycleRow key={event.sequence} event={event} description={describeEvent(event)} />)}
      </ol> : <p className="mt-2 text-sm text-text-tertiary">No lifecycle events recorded yet.</p>}
      {tools.length > 0 && <section aria-label="Tool execution logs" className="mt-4 space-y-2">
        <p className="text-[12px] text-text-tertiary">Expand a tool to inspect its input and output.</p>
        {shown.length < tools.length && <button type="button" onClick={() => { setFollowing(false); setVisible(count => count + 100) }} className="text-[12px] text-text-secondary underline">Show older tool logs ({tools.length - shown.length} remaining)</button>}
        <div ref={logRef} className="max-h-[520px] space-y-2 overflow-auto">{shown.map(log => <ToolLogRow key={log.key} log={log} live={live} follow={live && following} />)}</div>
      </section>}
    </details>
  )
}
