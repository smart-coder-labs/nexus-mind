/* ========================================
   INPUT - STYLES
   ======================================== */

export const baseInputStyles = `
  w-full
  bg-transparent
  border border-border-primary
  text-text-primary
  placeholder:text-text-quaternary
  transition-apple
  focus:outline-none
  focus:border-accent-blue/60
  disabled:opacity-40
  disabled:cursor-not-allowed
`;

export const inputSizeStyles = {
    sm: 'h-8 px-3 text-sm rounded-[11px]',
    md: 'h-10 px-4 text-base rounded-[11px]',
    lg: 'h-12 px-5 text-lg rounded-[11px]',
};

export const errorStyles = 'border-status-error focus:border-status-error/60';

export const labelStyles = 'block text-sm font-normal text-text-secondary mb-1.5';

export const helperTextStyles = 'mt-1.5 text-xs text-text-tertiary';

export const errorTextStyles = 'mt-1.5 text-xs text-status-error';