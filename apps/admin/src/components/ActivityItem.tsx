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
  if (diffMs < 3_600_000) {
    const mins = Math.floor(diffMs / 60_000)
    return `${mins}m ago`
  }
  if (diffMs < 86_400_000) {
    const hours = Math.floor(diffMs / 3_600_000)
    return `${hours}h ago`
  }
  const days = Math.floor(diffMs / 86_400_000)
  return `${days}d ago`
}

export function ActivityItem({ entry, userName }: ActivityItemProps) {
  const displayName = userName ?? 'Unknown'
  const initials = displayName === 'Unknown' ? '?' : displayName[0].toUpperCase()

  return (
    <div className="flex items-center gap-3 py-3">
      <div className="flex-shrink-0 w-8 h-8 rounded-full bg-accent-blue/20 text-accent-blue flex items-center justify-center text-xs font-bold">
        {initials}
      </div>
      <div className="flex-1 min-w-0">
        <p className="text-sm text-text-primary truncate">
          <span className="font-medium">{displayName}</span>
          <span className="text-text-tertiary mx-1">·</span>
          <span>{entry.action}</span>
          <span className="text-text-tertiary mx-1">·</span>
          <span className="text-text-secondary">{entry.resource_type}</span>
        </p>
      </div>
      <time
        dateTime={entry.timestamp}
        className="flex-shrink-0 text-xs text-text-tertiary"
      >
        {timeAgo(entry.timestamp)}
      </time>
    </div>
  )
}
