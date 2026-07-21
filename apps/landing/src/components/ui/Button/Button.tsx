import React from 'react';
import { motion } from 'framer-motion';

import type { ButtonProps, ButtonSize } from './Button.types';
import { buttonBaseStyles as baseStyles, buttonVariantStyles as variantStyles, buttonSizeStyles as sizeStyles } from './Button.styles';

/* Styles now live in Button.styles.ts (previously duplicated inline here and
   never imported, so editing Button.styles.ts had zero visual effect — see
   session report). Importing them makes that file the real source of truth
   again, matching what Input.tsx already does with Input.styles.ts. */

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
