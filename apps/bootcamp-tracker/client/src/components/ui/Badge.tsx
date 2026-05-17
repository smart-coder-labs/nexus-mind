export { Badge, NotificationBadge } from './Badge/index';
export type { BadgeProps, BadgeVariant, BadgeSize } from './Badge/Badge.types';

import { Badge } from './Badge/index';
export function PriorityBadge({ priority }: { priority: 'P0' | 'P1' | 'P2' }) {
  const variantMap: Record<string, 'error' | 'warning' | 'default'> = {
    P0: 'error', P1: 'warning', P2: 'default',
  };
  return <Badge variant={variantMap[priority] ?? 'default'} size="sm">{priority}</Badge>;
}
