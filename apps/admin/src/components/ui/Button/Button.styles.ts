/* ========================================
   BUTTON - STYLES
   ======================================== */

import type { ButtonVariant, ButtonSize } from './Button.types';

export const buttonBaseStyles = `
  inline-flex items-center justify-center gap-2
  font-medium transition-apple
  focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue focus-visible:ring-offset-2
  cursor-pointer
  disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none
  select-none
`;

export const buttonVariantStyles: Record<ButtonVariant, string> = {
    primary: `
    bg-accent-blue text-white
    hover:bg-accent-blue-hover
    active:bg-accent-blue-active
    shadow-sm
  `,
    secondary: `
    bg-surface-secondary text-text-primary
    border border-border-primary
    hover:bg-surface-primary hover:border-border-primary
    active:bg-surface-secondary
    shadow-xs
  `,
    ghost: `
    bg-transparent text-accent-blue
    hover:bg-accent-blue-tint
    active:bg-accent-blue-tint
  `,
    subtle: `
    bg-surface-secondary text-text-primary
    hover:bg-surface-primary
    active:bg-surface-secondary
  `,
    outline: `
    bg-transparent text-text-primary
    border border-border-primary
    hover:bg-surface-secondary hover:border-border-secondary
    active:bg-surface-tertiary
  `,
    destructive: `
    bg-status-error text-white
    hover:bg-red-600
    active:bg-red-700
    shadow-sm
  `,
};

export const buttonSizeStyles: Record<ButtonSize, string> = {
    sm: 'h-8 px-3 text-sm rounded-lg',
    md: 'h-10 px-4 text-base rounded-xl',
    lg: 'h-12 px-6 text-lg rounded-xl',
};