/* ========================================
   BADGE - TYPES
   ======================================== */

import type { HTMLMotionProps } from 'framer-motion';

export type BadgeVariant = 'default' | 'primary' | 'success' | 'warning' | 'error' | 'info';
export type BadgeSize = 'sm' | 'md' | 'lg';

export interface BadgeProps extends Omit<HTMLMotionProps<'span'>, 'size'> {
    variant?: BadgeVariant;
    size?: BadgeSize;
    dot?: boolean;
    children?: React.ReactNode;
}