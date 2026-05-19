/* ========================================
   INPUT - STYLES
   ======================================== */

export const baseInputStyles = `
  w-full
  bg-surface-primary
  border border-border-primary
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