/* ========================================
   INPUT - STYLES
   ======================================== */

export const baseInputStyles = `
  w-full
  bg-white/[0.04]
  border border-border-primary
  text-text-primary
  placeholder:text-text-quaternary
  transition-apple
  focus:outline-2 focus:outline-offset-2 focus:outline-focus-ring
  disabled:opacity-40
  disabled:cursor-not-allowed
`;

export const inputSizeStyles = {
    sm: 'h-8 px-3 text-[13px] rounded-[11px]',
    md: 'h-9 px-4 text-[13px] rounded-[11px]',
    lg: 'h-11 px-5 text-[13px] rounded-[11px]',
};

export const errorStyles = 'border-status-error';

export const labelStyles = 'block text-xs font-medium text-text-secondary mb-1.5';

export const helperTextStyles = 'mt-1.5 text-xs text-text-tertiary';

export const errorTextStyles = 'mt-1.5 text-xs text-status-error';