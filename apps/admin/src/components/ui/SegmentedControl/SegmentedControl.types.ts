import type { ReactNode } from 'react';

export interface SegmentedControlOption<T extends string = string> {
    value: T;
    label?: ReactNode;
    icon?: ReactNode;
    /** Accessible name — required when `label` is omitted (icon-only segments). */
    'aria-label'?: string;
}

export interface SegmentedControlProps<T extends string = string> {
    options: SegmentedControlOption<T>[];
    value: T;
    onChange: (value: T) => void;
    size?: 'sm' | 'md';
    className?: string;
}
