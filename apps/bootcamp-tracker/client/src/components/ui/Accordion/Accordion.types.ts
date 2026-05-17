/* ========================================
   ACCORDION - TYPES
   ======================================== */

import type { HTMLAttributes, ButtonHTMLAttributes, ReactNode } from 'react';

type AccordionType = 'single' | 'multiple';
type AccordionVariant = 'default' | 'bordered' | 'separated';

type AccordionSingleProps = {
    type?: "single";
    value?: string;
    defaultValue?: string;
    onValueChange?: (value: string) => void;
    collapsible?: boolean;
};

type AccordionMultipleProps = {
    type: "multiple";
    value?: string[];
    defaultValue?: string[];
    onValueChange?: (value: string[]) => void;
    collapsible?: boolean;
};

export type AccordionProps = (AccordionSingleProps | AccordionMultipleProps) & {
    className?: string;
    children?: ReactNode;
    variant?: AccordionVariant;
};

export interface AccordionItemProps extends HTMLAttributes<HTMLDivElement> {
    value: string;
    disabled?: boolean;
    variant?: AccordionVariant;
}

export interface AccordionTriggerProps extends ButtonHTMLAttributes<HTMLButtonElement> {}

export interface AccordionContentProps extends HTMLAttributes<HTMLDivElement> {}

/* Context types */
export interface AccordionContextValue {
    type: AccordionType;
    openValues: string[];
    collapsible: boolean;
    toggleItem: (value: string) => void;
}

export interface AccordionItemContextValue {
    value: string;
    isOpen: boolean;
    disabled?: boolean;
    toggle: () => void;
    triggerId: string;
    contentId: string;
}