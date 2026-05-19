/* ========================================
   BADGE - STYLES
   ======================================== */

import type { BadgeVariant, BadgeSize } from './Badge.types';

export const badgeBaseStyles = `
  inline-flex items-center justify-center gap-1.5
  font-medium transition-apple
  select-none
`;

export const badgeVariantStyles: Record<BadgeVariant, string> = {
    default: 'bg-surface-secondary text-text-secondary',
    primary: 'bg-accent-blue text-white',
    success: 'bg-status-success text-white',
    warning: 'bg-status-warning text-black',
    error: 'bg-status-error text-white',
    info: 'bg-accent-blue/10 text-accent-blue',
};

export const badgeSizeStyles: Record<BadgeSize, string> = {
    sm: 'text-[10px] px-2 py-0.5 rounded-md',
    md: 'text-xs px-2.5 py-1 rounded-lg',
    lg: 'text-sm px-3 py-1.5 rounded-lg',
};

export const badgeDotStyles = 'w-1.5 h-1.5 rounded-full';