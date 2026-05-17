import React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';

/* ========================================
   VARIANTS (cva)
   ======================================== */

export const searchInputVariants = cva(
    "w-full rounded-xl border bg-surface-primary text-text-primary transition-all",
    {
        variants: {
            size: {
                sm: "h-8 text-sm px-7",
                md: "h-10 text-base pl-9 pr-10",
                lg: "h-12 text-lg pl-10 pr-12",
            },
            variant: {
                default: "border-border-primary focus:border-accent-blue",
                error: "border-status-error focus:border-status-error",
            },
        },
        defaultVariants: {
            size: "md",
            variant: "default",
        },
    }
);

/* ========================================
   MAIN COMPONENT TYPES
   ======================================== */

export interface SearchInputProps
    extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'size' | 'onChange' | 'value'>,
        VariantProps<typeof searchInputVariants> {
    value: string;
    onChange: (value: string) => void;
    onSearch?: (value: string) => void;
    onClear?: () => void;
    isLoading?: boolean;
    debounceTime?: number;
    containerClassName?: string;
    label?: string;
    children?: React.ReactNode;
}

/* ========================================
   COMPOUND COMPONENT TYPES
   ======================================== */

export interface SearchInputInputProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'onChange'> {
    onChange: (value: string) => void;
    onSearch?: (value: string) => void;
    onClear?: () => void;
    isLoading?: boolean;
    debounceTime?: number;
    placeholder?: string;
}

export interface SearchInputDropdownProps {
    show: boolean;
    hasResults?: boolean;
    query?: string;
    children: React.ReactNode;
    className?: string;
    maxHeight?: string;
}

export interface SearchInputSectionProps {
    title: string;
    children: React.ReactNode;
    className?: string;
}

export interface SearchInputItemProps {
    children: React.ReactNode;
    onClick?: () => void;
    className?: string;
}

export interface SearchInputItemContentProps {
    label: string;
    subtitle?: string;
    className?: string;
}

export interface SearchInputTrailingBadgeProps {
    /** Badge content - can be text, icon, or any React node */
    children: React.ReactNode;
    /** Visual variant for the badge */
    variant?: 'default' | 'primary' | 'success' | 'warning' | 'error';
    className?: string;
}

export interface SearchInputItemIconProps {
    /** Resource type - renders predefined icon */
    type?: 'paper' | 'book' | 'course' | 'website';
    /** Custom icon node (overrides type) */
    icon?: React.ReactNode;
    className?: string;
}

/* ========================================
   CONTEXT TYPES
   ======================================== */

export type SearchInputContextValue = {
    isFocused: boolean;
    setIsFocused: (focused: boolean) => void;
    isLoading: boolean;
    disabled?: boolean;
    inputRef: React.RefObject<HTMLInputElement | null>;
};