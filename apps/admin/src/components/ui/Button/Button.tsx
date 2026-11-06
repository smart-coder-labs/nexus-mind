import React from 'react';
import { motion } from 'framer-motion';

import type { ButtonProps, ButtonSize } from './Button.types';

import { buttonVariants } from '../shadcn/button';
import { cn } from '../../../lib/utils';

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


        const combinedClassName = cn(
            buttonVariants({variant: variant === 'primary' ? 'default' : variant === 'subtle' ? 'secondary' : variant, size: size === 'md' ? 'default' : size}),
            fullWidth && 'w-full',
            className,
        );

        // Detect icon-only button (no visible text children)
        const hasTextChildren = children && !(typeof children === 'string' && children.trim() === '');
        const iconOnly = !hasTextChildren && (!!leftIcon || !!rightIcon);

        if (import.meta.env.DEV && iconOnly && !props['aria-label']) {
            console.warn('[Button] Icon-only button is missing an aria-label. Screen readers will not be able to describe this control.');
        }

        return (
            <motion.button
                type="button"
                data-slot="button"
                ref={ref}
                className={combinedClassName}
                disabled={disabled || loading}
                aria-busy={loading || undefined}

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
