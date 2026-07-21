/* ========================================
   INPUT - STYLES
   ======================================== */

/* Glass input recipe: translucent white fill (not the opaque surface-primary
   tile) so fields read as frosted glass against the page background, with a
   visible accent-blue ring on focus (keyboard and pointer alike — inputs are
   the one control where showing the ring on click, not only on tab, is the
   expected UX). */
export const baseInputStyles = `
  w-full
  bg-white/[0.04]
  border border-white/10
  text-text-primary
  placeholder:text-text-tertiary
  transition-apple
  focus:outline-none
  focus:border-accent-blue
  focus:ring-2
  focus:ring-accent-blue/20
  disabled:opacity-40
  disabled:cursor-not-allowed
`;

export const inputSizeStyles = {
    sm: 'h-8 px-3 text-sm rounded-lg',
    md: 'h-10 px-4 text-base rounded-xl',
    lg: 'h-12 px-5 text-lg rounded-xl',
};

export const errorStyles = 'border-status-error focus:border-status-error focus:ring-status-error/20';

export const labelStyles = 'block text-sm font-medium text-text-secondary mb-1.5';

export const helperTextStyles = 'mt-1.5 text-xs text-text-tertiary';

export const errorTextStyles = 'mt-1.5 text-xs text-status-error';
