/* ========================================
   BUTTON - TYPES
   ======================================== */

import type { HTMLMotionProps } from 'framer-motion';
import type { ReactNode } from 'react';

export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'subtle' | 'outline' | 'destructive';
export type ButtonSize = 'sm' | 'md' | 'lg';

export interface ButtonProps extends Omit<HTMLMotionProps<'button'>, 'size'> {
    variant?: ButtonVariant;
    size?: ButtonSize;
    loading?: boolean;
    leftIcon?: ReactNode;
    rightIcon?: ReactNode;
    fullWidth?: boolean;
    'aria-label'?: string;
    children?: ReactNode;
}

export interface ButtonGroupProps {
    buttons?: ButtonProps[];
    children?: ReactNode;
    orientation?: 'horizontal' | 'vertical';
}