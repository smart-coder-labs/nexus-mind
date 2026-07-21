/* ========================================
   BADGE - STYLES
   ======================================== */

import type { BadgeVariant, BadgeSize } from './Badge.types';

export const badgeBaseStyles = `
  inline-flex items-center justify-center gap-1.5
  font-medium transition-apple
  select-none rounded-full
`;

// Mirrors the single badge grammar in Badge.tsx (DESIGN_DIRECTION §5):
// tinted background at ~10% + 20% border; `default` is the neutral surface variant.
export const badgeVariantStyles: Record<BadgeVariant, string> = {
    default: 'bg-white/[0.06] text-text-secondary border border-white/[0.09]',
    primary: 'bg-accent-blue/10 text-accent-blue border border-accent-blue/20',
    success: 'bg-status-success/10 text-status-success border border-status-success/20',
    warning: 'bg-status-warning/10 text-status-warning border border-status-warning/20',
    error: 'bg-status-error/10 text-status-error border border-status-error/20',
    info: 'bg-status-info/10 text-status-info border border-status-info/20',
    purple: 'bg-accent-purple/10 text-accent-purple border border-accent-purple/20',
};

export const badgeSizeStyles: Record<BadgeSize, string> = {
    sm: 'text-[11px] px-2 py-0.5',
    md: 'text-[11px] px-2.5 py-1',
    lg: 'text-xs px-3 py-1.5',
};

export const badgeDotStyles = 'w-1.5 h-1.5 rounded-full';
