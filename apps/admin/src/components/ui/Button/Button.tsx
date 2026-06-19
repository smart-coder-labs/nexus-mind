import React from 'react';
import { motion } from 'framer-motion';

import type { ButtonProps, ButtonVariant, ButtonSize } from './Button.types';

/* ========================================
   STYLES
   ======================================== */

const baseStyles = `
  inline-flex items-center justify-center gap-2
  font-normal transition-apple
  focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue focus-visible:ring-offset-2
  cursor-pointer
  disabled:opacity-40 disabled:cursor-not-allowed disabled:pointer-events-none
  select-none
`;

const variantStyles: Record<ButtonVariant, string> = {
    primary: `
    bg-[#0066cc] text-white
    hover:bg-[#0071e3]
    active:bg-[#0058a8]
  `,
    secondary: `
    bg-transparent text-[#2997ff]
    border border-white/10
    hover:bg-white/5
    active:bg-white/10
  `,
    ghost: `
    bg-transparent text-[#2997ff]
    hover:bg-[rgba(0,102,204,0.12)]
    active:bg-[rgba(0,102,204,0.12)]
  `,
    subtle: `
    bg-surface-secondary text-text-primary
    hover:bg-surface-primary
    active:bg-surface-secondary
  `,
    outline: `
    bg-transparent text-[#2997ff]
    border border-white/10
    hover:bg-white/5
    active:bg-white/10
  `,
    destructive: `
    bg-status-error text-white
    hover:bg-red-600
    active:bg-red-700
  `,
};

const sizeStyles: Record<ButtonSize, string> = {
    sm: 'h-8 px-4 text-[14px] rounded-full',
    md: 'h-[44px] px-[22px] text-[17px] rounded-full',
    lg: 'h-[52px] px-[28px] text-[18px] font-light rounded-full',
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

        if (import.meta.env.DEV && iconOnly && !props['aria-label']) {
            console.warn('[Button] Icon-only button is missing an aria-label. Screen readers will not be able to describe this control.');
        }

        return (
            <motion.button
                ref={ref}
                className={combinedClassName}
                disabled={disabled || loading}
                aria-busy={loading || undefined}
                whileHover={{}}
                whileTap={{ scale: disabled || loading ? 1 : 0.95 }}
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
