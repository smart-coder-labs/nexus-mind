import React from 'react';
import { cn } from '../../../lib/utils';
import type { RadioProps, RadioGroupProps } from './Radio.types';

/* ========================================
   RADIO — matches the NexusMind UI Kit radio exactly:
   18x18 circle, 1.5px border (accent-blue when selected,
   white/25% otherwise), 9x9 filled dot when selected.

   Built on a real <input type="radio"> (appearance-none) so form
   semantics / keyboard / screen-reader behavior stay native — the
   circle and dot are drawn via a sibling span reacting to :checked.
   ======================================== */

export const Radio: React.FC<RadioProps> = ({
    name,
    value,
    checked,
    onChange,
    label,
    disabled = false,
    className = '',
}) => {
    return (
        <label
            className={cn(
                'group flex items-center gap-[11px] select-none',
                disabled ? 'cursor-not-allowed opacity-70' : 'cursor-pointer',
                className
            )}
        >
            <span className="relative inline-flex h-[18px] w-[18px] shrink-0 items-center justify-center">
                <input
                    type="radio"
                    name={name}
                    value={value}
                    checked={checked}
                    disabled={disabled}
                    onChange={() => onChange(value)}
                    className={cn(
                        'peer appearance-none h-[18px] w-[18px] rounded-full border-[1.5px] transition-colors m-0',
                        'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring',
                        checked ? 'border-accent-blue' : 'border-white/[0.25]',
                        disabled ? 'cursor-not-allowed' : 'cursor-pointer'
                    )}
                />
                <span className="pointer-events-none absolute h-[9px] w-[9px] rounded-full bg-accent-blue opacity-0 peer-checked:opacity-100 transition-opacity" />
            </span>
            {label && <span className="text-[13px] text-text-secondary">{label}</span>}
        </label>
    );
};

Radio.displayName = 'Radio';

export const RadioGroup: React.FC<RadioGroupProps> = ({
    name,
    value,
    onChange,
    options,
    disabled = false,
    className = '',
}) => {
    return (
        <div role="radiogroup" className={cn('flex flex-col gap-2.5', className)}>
            {options.map((opt) => (
                <Radio
                    key={opt.value}
                    name={name}
                    value={opt.value}
                    checked={value === opt.value}
                    onChange={onChange}
                    label={opt.label}
                    disabled={disabled || opt.disabled}
                />
            ))}
        </div>
    );
};

RadioGroup.displayName = 'RadioGroup';
