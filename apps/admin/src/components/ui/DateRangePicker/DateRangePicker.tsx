import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Calendar, ChevronDown, ChevronLeft, ChevronRight } from 'lucide-react';
import { cn } from '../../../lib/utils';
import type { DateRangePickerProps } from './DateRangePicker.types';

/* ========================================
   DATE RANGE PICKER — matches the NexusMind UI Kit:
   trigger pill ("Jul 6 – Jul 14") + calendar popover with month nav,
   range highlight (solid start/end, tinted in-range) and a quick
   presets row (Today / 7d / 30d / Month).

   State contract: `from`/`to` are `YYYY-MM-DD` strings (or '' for
   unbounded) — the same shape the native <input type="date"> pair it
   replaces used, so callers don't need to change how they store state.
   ======================================== */

const WEEKDAY_LABELS = ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su'];

function pad2(n: number): string {
    return n < 10 ? `0${n}` : String(n);
}

/** Parses a `YYYY-MM-DD` string as a LOCAL date (not UTC midnight) so the
 *  calendar never drifts a day off in timezones west of UTC. */
function fromISO(s: string): Date | null {
    if (!s) return null;
    const [y, m, d] = s.split('-').map(Number);
    if (!y || !m || !d) return null;
    return new Date(y, m - 1, d);
}

function toISO(date: Date): string {
    return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}`;
}

function addDays(date: Date, delta: number): Date {
    const next = new Date(date);
    next.setDate(next.getDate() + delta);
    return next;
}

function addMonths(date: Date, delta: number): Date {
    return new Date(date.getFullYear(), date.getMonth() + delta, 1);
}

function startOfMonth(date: Date): Date {
    return new Date(date.getFullYear(), date.getMonth(), 1);
}

function daysInMonth(date: Date): number {
    return new Date(date.getFullYear(), date.getMonth() + 1, 0).getDate();
}

function sameDay(a: Date, b: Date): boolean {
    return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

const monthLabelFmt = new Intl.DateTimeFormat('en-US', { month: 'long', year: 'numeric' });
const shortLabelFmt = new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric' });

function buildDays(monthDate: Date): (Date | null)[] {
    const first = startOfMonth(monthDate);
    // Monday-first offset: getDay() is 0=Sun..6=Sat, so (day+6)%7 gives 0=Mon..6=Sun.
    const offset = (first.getDay() + 6) % 7;
    const total = daysInMonth(monthDate);
    const cells: (Date | null)[] = [];
    for (let i = 0; i < offset; i++) cells.push(null);
    for (let d = 1; d <= total; d++) cells.push(new Date(monthDate.getFullYear(), monthDate.getMonth(), d));
    return cells;
}

export const DateRangePicker: React.FC<DateRangePickerProps> = ({
    from,
    to,
    onChange,
    placeholder = 'All time',
    className = '',
}) => {
    const [open, setOpen] = useState(false);
    const fromDate = useMemo(() => fromISO(from), [from]);
    const toDate = useMemo(() => fromISO(to), [to]);
    const [viewMonth, setViewMonth] = useState<Date>(() => startOfMonth(fromDate ?? new Date()));
    const containerRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (open) setViewMonth(startOfMonth(fromDate ?? new Date()));
        // Only re-anchor the visible month when the popover opens, not on every
        // keystroke-driven `from` change — otherwise picking the range's second
        // day could jump the calendar out from under the user's cursor.
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [open]);

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

    const label = useMemo(() => {
        if (!fromDate && !toDate) return placeholder;
        if (fromDate && toDate && !sameDay(fromDate, toDate)) {
            return `${shortLabelFmt.format(fromDate)} – ${shortLabelFmt.format(toDate)}`;
        }
        if (fromDate) return shortLabelFmt.format(fromDate);
        if (toDate) return `Until ${shortLabelFmt.format(toDate)}`;
        return placeholder;
    }, [fromDate, toDate, placeholder]);

    const isRangeComplete = !!fromDate && !!toDate && !sameDay(fromDate, toDate);

    const handleDayClick = (day: Date) => {
        const iso = toISO(day);
        if (isRangeComplete || !fromDate) {
            // No selection yet, or a full range is already set — start fresh.
            onChange({ from: iso, to: iso });
            return;
        }
        // Single day selected: extend forward, or swap if picking an earlier day.
        if (iso >= from) {
            onChange({ from, to: iso });
            setOpen(false);
        } else {
            onChange({ from: iso, to: from });
            setOpen(false);
        }
    };

    const applyPreset = (next: { from: Date; to: Date }) => {
        onChange({ from: toISO(next.from), to: toISO(next.to) });
        setOpen(false);
    };

    const today = new Date();
    const presets: { name: string; onClick: () => void }[] = [
        { name: 'Today', onClick: () => applyPreset({ from: today, to: today }) },
        { name: '7d', onClick: () => applyPreset({ from: addDays(today, -6), to: today }) },
        { name: '30d', onClick: () => applyPreset({ from: addDays(today, -29), to: today }) },
        {
            name: 'Month',
            onClick: () =>
                applyPreset({
                    from: startOfMonth(today),
                    to: new Date(today.getFullYear(), today.getMonth(), daysInMonth(today)),
                }),
        },
    ];

    const days = useMemo(() => buildDays(viewMonth), [viewMonth]);
    const rangeStart = fromDate && toDate ? (fromDate <= toDate ? fromDate : toDate) : fromDate;
    const rangeEnd = fromDate && toDate ? (fromDate <= toDate ? toDate : fromDate) : toDate;

    return (
        <div ref={containerRef} className={cn('relative shrink-0', className)}>
            <button
                type="button"
                onClick={() => setOpen((v) => !v)}
                aria-haspopup="dialog"
                aria-expanded={open}
                aria-label="Filter by date range"
                className={cn(
                    'inline-flex items-center gap-2.5 h-9 rounded-full border px-3.5 text-[12.5px] font-semibold transition-colors',
                    'bg-[#0d0f14]/60 backdrop-blur-[12px]',
                    open
                        ? 'border-accent-blue/55 text-text-primary'
                        : 'border-white/[0.09] text-text-tertiary hover:border-white/20 hover:text-text-secondary'
                )}
            >
                <Calendar className="w-3.5 h-3.5 text-accent-blue shrink-0" />
                {label}
                <ChevronDown className={cn('w-3 h-3 text-text-quaternary transition-transform duration-200', open && 'rotate-180')} />
            </button>

            {open && (
                <div
                    role="dialog"
                    aria-label="Choose date range"
                    className="absolute left-0 top-[46px] z-30 w-[280px] rounded-[14px] border border-white/[0.10] bg-[#111319]/[0.98] backdrop-blur-[20px] shadow-[0_18px_50px_rgba(0,0,0,0.6)] p-3.5"
                >
                    {/* Month nav */}
                    <div className="flex items-center justify-between mb-2.5">
                        <button
                            type="button"
                            onClick={() => setViewMonth((m) => addMonths(m, -1))}
                            aria-label="Previous month"
                            className="w-[26px] h-[26px] rounded-[8px] flex items-center justify-center text-text-tertiary hover:bg-white/[0.07] hover:text-text-primary transition-colors"
                        >
                            <ChevronLeft className="w-3.5 h-3.5" />
                        </button>
                        <span className="text-[13px] font-bold text-text-primary">{monthLabelFmt.format(viewMonth)}</span>
                        <button
                            type="button"
                            onClick={() => setViewMonth((m) => addMonths(m, 1))}
                            aria-label="Next month"
                            className="w-[26px] h-[26px] rounded-[8px] flex items-center justify-center text-text-tertiary hover:bg-white/[0.07] hover:text-text-primary transition-colors"
                        >
                            <ChevronRight className="w-3.5 h-3.5" />
                        </button>
                    </div>

                    {/* Weekday header */}
                    <div className="grid grid-cols-7 gap-0.5 mb-1">
                        {WEEKDAY_LABELS.map((d) => (
                            <span key={d} className="text-center text-[10.5px] font-bold text-text-quaternary py-1">
                                {d}
                            </span>
                        ))}
                    </div>

                    {/* Day grid */}
                    <div className="grid grid-cols-7 gap-0.5">
                        {days.map((day, i) => {
                            if (!day) return <div key={`blank-${i}`} />;
                            const isStart = rangeStart && sameDay(day, rangeStart);
                            const isEnd = rangeEnd && sameDay(day, rangeEnd);
                            const isEdge = !!isStart || !!isEnd;
                            const inRange = !!rangeStart && !!rangeEnd && day > rangeStart && day < rangeEnd;
                            return (
                                <button
                                    key={toISO(day)}
                                    type="button"
                                    onClick={() => handleDayClick(day)}
                                    className={cn(
                                        'h-8 flex items-center justify-center text-[12.5px] tabular-nums transition-colors',
                                        isEdge
                                            ? 'rounded-[9px] bg-accent-blue text-white font-extrabold'
                                            : inRange
                                                ? 'rounded-none bg-accent-blue/[0.14] text-text-primary font-medium'
                                                : 'rounded-[8px] text-text-secondary font-medium hover:bg-accent-blue/[0.22]'
                                    )}
                                >
                                    {day.getDate()}
                                </button>
                            );
                        })}
                    </div>

                    {/* Presets */}
                    <div className="flex gap-1.5 mt-3 pt-3 border-t border-white/[0.06]">
                        {presets.map((p) => (
                            <button
                                key={p.name}
                                type="button"
                                onClick={p.onClick}
                                className="flex-1 text-center py-1.5 rounded-[8px] border border-white/[0.08] text-[11.5px] font-semibold text-text-tertiary hover:border-accent-blue/50 hover:text-text-primary transition-colors"
                            >
                                {p.name}
                            </button>
                        ))}
                    </div>
                </div>
            )}
        </div>
    );
};

DateRangePicker.displayName = 'DateRangePicker';
