import React from 'react';
import { motion } from 'framer-motion';

import type { ButtonProps, ButtonVariant, ButtonSize } from './Button.types';

/* ========================================
   STYLES
   ======================================== */

const baseStyles = `
  inline-flex items-center justify-center gap-2
  font-medium transition-apple
  focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue focus-visible:ring-offset-2
  cursor-pointer
  disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none
  select-none
`;

const variantStyles: Record<ButtonVariant, string> = {
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

const sizeStyles: Record<ButtonSize, string> = {
    sm: 'h-8 px-3 text-sm rounded-lg',
    md: 'h-10 px-4 text-base rounded-xl',
    lg: 'h-12 px-6 text-lg rounded-xl',
};

/* ========================================
   COMPONENT
   ======================================== */

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
    (
        {
            variant = 'primary',
            size = 'md',
            loading = false,
            leftIcon,
            rightIcon,
            fullWidth = false,
            children,
            className = '',
            disabled,
            ...props
        },
        ref
    ) => {
        const combinedClassName = `
      ${baseStyles}
      ${variantStyles[variant]}
      ${sizeStyles[size]}
      ${fullWidth ? 'w-full' : ''}
      ${className}
    `.trim().replace(/\s+/g, ' ');

        // Detect icon-only button (no visible text children)
        const hasTextChildren = children && !(typeof children === 'string' && children.trim() === '');
        const iconOnly = !hasTextChildren && (!!leftIcon || !!rightIcon);

        if (process.env.NODE_ENV !== 'production' && iconOnly && !props['aria-label']) {
            console.warn('[Button] Icon-only button is missing an aria-label. Screen readers will not be able to describe this control.');
        }

        return (
            <motion.button
                ref={ref}
                className={combinedClassName}
                disabled={disabled || loading}
                aria-busy={loading || undefined}
                whileHover={{ scale: disabled || loading ? 1 : 1.02 }}
                whileTap={{ scale: disabled || loading ? 1 : 0.98 }}
                transition={{
                    type: 'spring',
                    stiffness: 400,
                    damping: 25,
                    mass: 0.6,
                }}
                {...props}
            >
                {loading ? (
                    <LoadingSpinner size={size} />
                ) : (
                    <>
                        {leftIcon && <span className="inline-flex">{leftIcon}</span>}
                        {children}
                        {rightIcon && <span className="inline-flex">{rightIcon}</span>}
                    </>
                )}
            </motion.button>
        );
    }
);

Button.displayName = 'Button';

/* ========================================
   LOADING SPINNER
   ======================================== */

const LoadingSpinner: React.FC<{ size: ButtonSize }> = ({ size }) => {
    const sizeMap = {
        sm: 14,
        md: 16,
        lg: 18,
    };

    const spinnerSize = sizeMap[size];

    return (
        <motion.svg
            width={spinnerSize}
            height={spinnerSize}
            viewBox="0 0 24 24"
            fill="none"
            animate={{ rotate: 360 }}
            transition={{
                duration: 1,
                repeat: Infinity,
                ease: 'linear',
            }}
        >
            <circle
                cx="12"
                cy="12"
                r="10"
                stroke="currentColor"
                strokeWidth="3"
                strokeLinecap="round"
                strokeDasharray="60"
                strokeDashoffset="15"
                opacity="0.25"
            />
            <circle
                cx="12"
                cy="12"
                r="10"
                stroke="currentColor"
                strokeWidth="3"
                strokeLinecap="round"
                strokeDasharray="60"
                strokeDashoffset="45"
            />
        </motion.svg>
    );
};

/* ========================================
   USAGE EXAMPLES
   ======================================== */

/*
// Primary button
<Button variant="primary">
  Continue
</Button>

// Secondary with icon
<Button variant="secondary" leftIcon={<Icon />}>
  Back
</Button>

// Loading state
<Button variant="primary" loading>
  Processing...
</Button>

// Ghost button
<Button variant="ghost">
  Cancel
</Button>

// Full width
<Button variant="primary" fullWidth>
  Sign In
</Button>
*/
