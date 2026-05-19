/* ========================================
   CARD - TYPES
   ======================================== */

import type { HTMLAttributes } from 'react';
import type { HTMLMotionProps } from 'framer-motion';

export type CardVariant = 'elevated' | 'glass' | 'outlined' | 'flat';

export interface CardProps extends HTMLMotionProps<'div'> {
    variant?: CardVariant;
    hoverable?: boolean;
    padding?: 'none' | 'sm' | 'md' | 'lg';
}

export interface CardHeaderProps extends HTMLAttributes<HTMLDivElement> {}
export interface CardContentProps extends HTMLAttributes<HTMLDivElement> {}
export interface CardFooterProps extends HTMLAttributes<HTMLDivElement> {}