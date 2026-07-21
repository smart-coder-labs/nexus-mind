/* ========================================
   BADGE - STYLES
   ======================================== */

import type { BadgeVariant, BadgeSize } from './Badge.types';

export const badgeBaseStyles = `
  inline-flex items-center justify-center gap-1.5
  font-medium
  rounded-full
  transition-apple
  select-none
`;

/* Glass pill badges: tinted background + matching border, per the
   dark-glass recipe (see ref-components.png "Badges de estado"). */
export const badgeVariantStyles: Record<BadgeVariant, string> = {
  default: `
    bg-white/5
    text-text-primary
    border border-white/10
  `,
  primary: `
    bg-accent-blue
    text-white
  `,
  success: `
    bg-status-success/10
    text-status-success
    border border-status-success/20
  `,
  warning: `
    bg-status-warning/10
    text-status-warning
    border border-status-warning/20
  `,
  error: `
    bg-status-error/10
    text-status-error
    border border-status-error/20
  `,
  info: `
    bg-accent-blue/10
    text-accent-blue-hover
    border border-accent-blue/20
  `,
};

export const badgeSizeStyles: Record<BadgeSize, string> = {
  sm: 'h-5 px-2 text-xs',
  md: 'h-6 px-2.5 text-sm',
  lg: 'h-7 px-3 text-base',
};

export const badgeDotSizeStyles: Record<BadgeSize, string> = {
  sm: 'w-1.5 h-1.5',
  md: 'w-2 h-2',
  lg: 'w-2.5 h-2.5',
};

export const badgeDotColorStyles: Record<BadgeVariant, string> = {
  default: 'bg-text-primary',
  primary: 'bg-white',
  success: 'bg-status-success',
  warning: 'bg-status-warning',
  error: 'bg-status-error',
  info: 'bg-accent-blue-hover',
};
