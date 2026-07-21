import { cn } from '../../../lib/utils';
import type { SegmentedControlProps } from './SegmentedControl.types';

/* ========================================
   SEGMENTED CONTROL — matches the NexusMind UI Kit "Segmented" component:
   a glass pill container with a filled, blue-tinted active segment.
   Used for the Lista/Board/Timeline-style view toggles across the app
   (Tasks view, Memories search mode, Conventions raw/preview & sort).
   ======================================== */

const sizeStyles = {
    sm: { pad: 'px-2.5 py-1', text: 'text-xs' },
    md: { pad: 'px-4 py-[7px]', text: 'text-[13px]' },
};

export function SegmentedControl<T extends string = string>({
    options,
    value,
    onChange,
    size = 'md',
    className = '',
}: SegmentedControlProps<T>) {
    const sizes = sizeStyles[size];
    return (
        <div
            role="group"
            className={cn(
                'inline-flex items-center gap-0.5 rounded-[12px] border border-white/[0.08] bg-[#0d0f14]/60 backdrop-blur-[12px] p-1 w-max',
                className
            )}
        >
            {options.map((opt) => {
                const active = opt.value === value;
                return (
                    <button
                        key={opt.value}
                        type="button"
                        aria-pressed={active}
                        aria-label={opt['aria-label']}
                        title={opt['aria-label']}
                        onClick={() => onChange(opt.value)}
                        className={cn(
                            'inline-flex items-center gap-1.5 rounded-[9px] font-semibold transition-colors',
                            sizes.pad,
                            sizes.text,
                            active
                                ? 'bg-accent-blue/[0.18] text-accent-blue'
                                : 'text-text-quaternary hover:text-text-secondary'
                        )}
                    >
                        {opt.icon}
                        {opt.label}
                    </button>
                );
            })}
        </div>
    );
}

SegmentedControl.displayName = 'SegmentedControl';
