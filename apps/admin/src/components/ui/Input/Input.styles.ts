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
  focus:outline-none
  focus:border-accent-blue/60
  disabled:opacity-40
  disabled:cursor-not-allowed
`;

export const inputSizeStyles = {
    sm: 'h-8 px-3 text-xs rounded-[8px]',
    md: 'h-10 px-4 text-xs rounded-[8px]',
    lg: 'h-12 px-5 text-sm rounded-[8px]',
};

export const errorStyles = 'border-status-error focus:border-status-error/60';

export const labelStyles = 'block text-[10px] text-text-quaternary mb-1.5';

export const helperTextStyles = 'mt-1.5 text-xs text-text-tertiary';

export const errorTextStyles = 'mt-1.5 text-xs text-status-error';