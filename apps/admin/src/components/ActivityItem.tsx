import type { AuditEntry } from '../types'

export interface ActivityItemProps {
  entry: AuditEntry
  userName?: string
}

function timeAgo(iso: string): string {
  const now = Date.now()
  const then = new Date(iso).getTime()
  const diffMs = now - then

  if (diffMs < 60_000) return 'just now'
  if (diffMs < 3_600_000) return `${Math.floor(diffMs / 60_000)}m ago`
  if (diffMs < 86_400_000) return `${Math.floor(diffMs / 3_600_000)}h ago`
  return `${Math.floor(diffMs / 86_400_000)}d ago`
}

export function ActivityItem({ entry, userName }: ActivityItemProps) {
  const displayName = userName ?? 'Unknown'

  return (
    <div className="flex items-center gap-4 py-3">
      <div className="flex-1 min-w-0 flex items-center gap-2">
        <span className="text-sm text-white/70 font-medium truncate">{displayName}</span>
        <span className="text-white/15">·</span>
        <span className="text-sm text-white/40 truncate">{entry.action}</span>
        <span className="text-white/15">·</span>
        <span className="text-[12px] text-white/25 truncate">{entry.resource_type}</span>
      </div>
      <time dateTime={entry.timestamp} className="flex-shrink-0 text-[11px] text-white/20 tabular-nums">
        {timeAgo(entry.timestamp)}
      </time>
    </div>
  )
}
