/* ========================================
   BUTTON - STYLES
   ======================================== */

import type { ButtonVariant, ButtonSize } from './Button.types';

export const buttonBaseStyles = `
  inline-flex items-center justify-center gap-2
  font-semibold transition-apple
  focus-visible:outline-none
  cursor-pointer
  disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none
  select-none
`;

// Kept in sync with Button.tsx's inline variantStyles (the NexusMind UI Kit's
// Primary/Secondary/Ghost/Danger grammar) even though this file isn't
// currently imported by Button.tsx — see that file for the live styles.
export const buttonVariantStyles: Record<ButtonVariant, string> = {
    primary: `
    bg-accent-blue text-white
    hover:bg-accent-blue-hover
    active:bg-accent-blue-active
    shadow-md shadow-accent-blue/30
  `,
    secondary: `
    bg-transparent text-text-secondary
    border border-border-primary
    hover:bg-white/5 hover:border-border-secondary
    active:bg-white/10
  `,
    ghost: `
    bg-transparent text-text-tertiary
    hover:text-text-primary
    active:text-text-primary
  `,
    subtle: `
    bg-white/[0.06] text-text-primary
    hover:bg-white/[0.10]
    active:bg-white/[0.06]
  `,
    outline: `
    bg-transparent text-text-primary
    border border-border-primary
    hover:bg-white/[0.06] hover:border-border-secondary
    active:bg-white/[0.10]
  `,
    destructive: `
    bg-transparent text-status-error
    border border-status-error/35
    hover:bg-status-error/10
    active:bg-status-error/15
  `,
};

export const buttonSizeStyles: Record<ButtonSize, string> = {
    sm: 'h-8 px-3 text-xs rounded-full',
    md: 'h-10 px-4 text-xs rounded-full',
    lg: 'h-12 px-6 text-xs rounded-full',
};
