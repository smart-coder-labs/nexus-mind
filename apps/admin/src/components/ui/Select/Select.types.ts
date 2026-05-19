/* ========================================
   SELECT - TYPES
   ======================================== */

import type { HTMLAttributes } from 'react';

export interface SelectProps extends HTMLAttributes<HTMLDivElement> {
    value?: string;
    onValueChange?: (value: string) => void;
    disabled?: boolean;
    defaultValue?: string;
}

export interface SelectTriggerProps extends HTMLAttributes<HTMLButtonElement> {
    placeholder?: string;
}

export interface SelectContentProps extends HTMLAttributes<HTMLDivElement> {}

export interface SelectItemProps extends HTMLAttributes<HTMLDivElement> {
    value: string;
    disabled?: boolean;
}

export interface SelectLabelProps extends HTMLAttributes<HTMLDivElement> {}

export interface SelectSeparatorProps extends HTMLAttributes<HTMLDivElement> {}