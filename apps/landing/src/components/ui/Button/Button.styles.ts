/* ========================================
   BUTTON - STYLES
   ======================================== */

import type { ButtonVariant, ButtonSize } from './Button.types';

export const buttonBaseStyles = `
  inline-flex items-center justify-center gap-2
  rounded-full font-semibold transition-apple
  focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-hover focus-visible:ring-offset-2 focus-visible:ring-offset-background-primary
  cursor-pointer
  disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none
  select-none
`;

export const buttonVariantStyles: Record<ButtonVariant, string> = {
    primary: `
    bg-accent-blue text-white
    hover:bg-accent-blue-hover
    active:bg-accent-blue-active
    shadow-lg shadow-accent-blue/20
    hover:shadow-xl hover:shadow-accent-blue/30
  `,
    secondary: `
    bg-white/5 text-text-primary
    border border-white/10
    hover:bg-white/10 hover:border-white/20
  `,
    ghost: `
    bg-transparent text-accent-blue
    hover:opacity-80 hover:underline underline-offset-4
  `,
    subtle: `
    bg-white/5 text-text-primary
    hover:bg-white/10
  `,
    outline: `
    bg-transparent text-text-primary
    border border-white/10
    hover:bg-white/5 hover:border-white/20
  `,
    destructive: `
    bg-status-error text-white
    hover:bg-red-600
    active:bg-red-700
    shadow-sm
  `,
};

export const buttonSizeStyles: Record<ButtonSize, string> = {
    sm: 'h-8 px-3 text-sm',
    md: 'h-10 px-4 text-base',
    lg: 'h-12 px-6 text-lg',
};
