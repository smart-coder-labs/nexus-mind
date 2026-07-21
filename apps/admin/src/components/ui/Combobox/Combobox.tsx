import React, { useEffect, useRef, useState } from 'react';
import { Search } from 'lucide-react';
import { cn } from '../../../lib/utils';
import type { ComboboxProps } from './Combobox.types';

/* ========================================
   COMBOBOX — matches the NexusMind UI Kit search combobox: a glass
   input with a leading magnifier icon and a dropdown of results (each
   with an optional leading color dot), plus a "no results" state.
   ======================================== */

export const Combobox: React.FC<ComboboxProps> = ({
    value,
    onChange,
    options,
    onSelect,
    placeholder = 'Search…',
    noResultsLabel = 'No results',
    className = '',
}) => {
    const [open, setOpen] = useState(false);
    const containerRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (!open) return;
        const handleClick = (e: MouseEvent) => {
            if (containerRef.current && !containerRef.current.contains(e.target as Node)) setOpen(false);
        };
        const handleKey = (e: KeyboardEvent) => {
            if (e.key === 'Escape') setOpen(false);
        };
        document.addEventListener('mousedown', handleClick);
        document.addEventListener('keydown', handleKey);
        return () => {
            document.removeEventListener('mousedown', handleClick);
            document.removeEventListener('keydown', handleKey);
        };
    }, [open]);

    return (
        <div ref={containerRef} className={cn('relative', className)}>
            <div
                className={cn(
                    'flex items-center gap-2.5 h-10 rounded-[11px] border bg-[#0d0f14]/60 backdrop-blur-[12px] px-3.5 transition-colors',
                    open ? 'border-accent-blue/55' : 'border-white/[0.09]'
                )}
            >
                <Search className="w-3.5 h-3.5 text-text-quaternary shrink-0" />
                <input
                    type="text"
                    value={value}
                    onChange={(e) => {
                        onChange(e.target.value);
                        setOpen(true);
                    }}
                    onFocus={() => setOpen(true)}
                    placeholder={placeholder}
                    className="flex-1 min-w-0 bg-transparent border-none outline-none text-[13px] text-text-primary placeholder:text-text-quaternary"
                />
            </div>

            {open && (
                <div className="absolute left-0 right-0 top-[46px] z-30 rounded-[12px] border border-white/[0.10] bg-[#111319]/[0.98] backdrop-blur-[20px] shadow-[0_18px_50px_rgba(0,0,0,0.6)] p-1 max-h-[210px] overflow-y-auto">
                    {options.length === 0 ? (
                        <div className="py-3 text-center text-[12.5px] text-text-quaternary">{noResultsLabel}</div>
                    ) : (
                        options.map((opt) => (
                            <button
                                key={opt.id}
                                type="button"
                                onClick={() => {
                                    onSelect(opt);
                                    setOpen(false);
                                }}
                                className="w-full flex items-center gap-2.5 rounded-[8px] px-2.5 py-2 text-[13px] text-text-secondary hover:bg-white/[0.06] transition-colors text-left"
                            >
                                {opt.dotColor && (
                                    <span
                                        className="w-[7px] h-[7px] rounded-full shrink-0"
                                        style={{ backgroundColor: opt.dotColor }}
                                    />
                                )}
                                {opt.label}
                            </button>
                        ))
                    )}
                </div>
            )}
        </div>
    );
};

Combobox.displayName = 'Combobox';
