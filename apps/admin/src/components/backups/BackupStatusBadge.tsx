import { Badge } from '../ui/Badge'
import type { BackupStatus } from '../../types'

interface BackupStatusBadgeProps {
  status: BackupStatus
}

function statusVariant(status: BackupStatus): 'success' | 'info' | 'warning' | 'error' | 'default' {
  switch (status) {
    case 'completed':
      return 'success'
    case 'running':
      return 'info'
    case 'pending':
      return 'warning'
    case 'failed':
      return 'error'
    default:
      return 'default'
  }
}

export function BackupStatusBadge({ status }: BackupStatusBadgeProps) {
  return (
    <Badge variant={statusVariant(status)} size="sm" dot>
      {status}
    </Badge>
  )
}
