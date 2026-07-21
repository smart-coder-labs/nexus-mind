import React from 'react';
import { motion } from 'framer-motion';

/* ========================================
   TYPES
   ======================================== */

export interface SwitchProps {
    checked?: boolean;
    onCheckedChange?: (checked: boolean) => void;
    disabled?: boolean;
    label?: string;
    description?: string;
    size?: 'sm' | 'md' | 'lg';
    className?: string;
    'aria-label'?: string;
}

/* ========================================
   STYLES
   ======================================== */

const sizeStyles = {
    // Matches the NexusMind UI Kit pill toggle exactly: 38x22 track, 16x16 knob,
    // knob resting 3px from each edge (left: 3px, right: 38-16-3=19px → translateX 16).
    sm: {
        root: 'w-[38px] h-[22px]',
        thumb: 'w-4 h-4',
        translateX: 16,
    },
    md: {
        root: 'w-11 h-6',
        thumb: 'w-5 h-5',
        // md: root w-11 (44px), thumb w-5 (20px). Translate = 44 - 20 - 4 = 20px.
        translateX: 20,
    },
    lg: {
        root: 'w-14 h-8',
        thumb: 'w-7 h-7',
        // lg: root w-14 (56px), thumb w-7 (28px). Translate = 56 - 28 - 4 = 24px.
        translateX: 24,
    },
};

/* ========================================
   COMPONENT
   ======================================== */

export const Switch: React.FC<SwitchProps> = ({
    checked = false,
    onCheckedChange,
    disabled = false,
    label,
    description,
    size = 'md',
    className = '',
    'aria-label': ariaLabel,
}) => {
    const sizes = sizeStyles[size];

    const handleClick = () => {
        if (!disabled && onCheckedChange) {
            onCheckedChange(!checked);
        }
    };

    const handleKeyDown = (event: React.KeyboardEvent) => {
        if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault();
            handleClick();
        }
    };

    const switchElement = (
        <button
            type="button"
            role="switch"
            aria-checked={checked}
            aria-label={ariaLabel}
            disabled={disabled}
            onClick={handleClick}
            onKeyDown={handleKeyDown}
            className={`
                ${sizes.root}
                inline-flex shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent transition-colors
                focus-visible:outline-none focus-visible:border-accent-blue/60
                disabled:cursor-not-allowed disabled:opacity-50
                ${checked ? 'bg-accent-blue' : 'bg-white/[0.12]'}
                ${className}
            `}
        >
            <motion.span
                className={`
                    ${sizes.thumb}
                    pointer-events-none block rounded-full bg-white ring-0
                `}
                animate={{
                    x: checked ? sizes.translateX : 0
                }}
                transition={{ type: 'spring', stiffness: 500, damping: 30 }}
            />
        </button>
    );

    if (label || description) {
        return (
            <label className={`flex items-start gap-3 ${disabled ? 'cursor-not-allowed opacity-70' : 'cursor-pointer'}`}>
                <div className="flex items-center h-full pt-0.5">
                   {switchElement}
                </div>
                <div
                    className="flex-1"
                    onClick={(_e) => {
                       // Prevent triggering the button twice if label click propagates
                    }}
                >
                    {label && (
                        <p className="text-xs font-normal text-text-primary">{label}</p>
                    )}
                    {description && (
                        <p className="text-xs text-text-secondary mt-0.5">{description}</p>
                    )}
                </div>
            </label>
        );
    }

    return switchElement;
};

/* ========================================
   USAGE EXAMPLES
   ======================================== */

/*
// Basic switch
const [enabled, setEnabled] = useState(false);

<Switch checked={enabled} onCheckedChange={setEnabled} />

// Switch with label
<Switch
  checked={enabled}
  onCheckedChange={setEnabled}
  label="Enable notifications"
/>

// Switch with label and description
<Switch
  checked={enabled}
  onCheckedChange={setEnabled}
  label="Dark Mode"
  description="Toggle between light and dark theme"
/>

// Different sizes
<Switch size="sm" checked={enabled} onCheckedChange={setEnabled} />
<Switch size="md" checked={enabled} onCheckedChange={setEnabled} />
<Switch size="lg" checked={enabled} onCheckedChange={setEnabled} />

// Disabled switch
<Switch
  checked={enabled}
  onCheckedChange={setEnabled}
  disabled
  label="Disabled switch"
/>
*/
