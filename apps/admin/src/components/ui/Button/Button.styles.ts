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

export const buttonVariantStyles: Record<ButtonVariant, string> = {
    primary: `
    bg-accent-blue text-white
    hover:bg-accent-blue-hover
    active:bg-accent-blue-active
    shadow-sm
  `,
    secondary: `
    bg-[#272729] text-text-primary
    border border-border-primary
    hover:bg-[#1d1d1f] hover:border-border-primary
    active:bg-[#272729]
    shadow-xs
  `,
    ghost: `
    bg-transparent text-accent-blue
    hover:bg-accent-blue-tint
    active:bg-accent-blue-tint
  `,
    subtle: `
    bg-[#272729] text-text-primary
    hover:bg-[#1d1d1f]
    active:bg-[#272729]
  `,
    outline: `
    bg-transparent text-text-primary
    border border-border-primary
    hover:bg-[#272729] hover:border-border-secondary
    active:bg-[#1d1d1f]
  `,
    destructive: `
    bg-status-error text-white
    hover:bg-status-error/80
    active:bg-status-error/70
    shadow-sm
  `,
};

export const buttonSizeStyles: Record<ButtonSize, string> = {
    sm: 'h-8 px-3 text-xs rounded-full',
    md: 'h-10 px-4 text-xs rounded-full',
    lg: 'h-12 px-6 text-xs rounded-full',
};
