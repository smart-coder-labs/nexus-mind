export interface DateRangeValue {
    /** ISO date-only string (`YYYY-MM-DD`), or `''` for "no bound". */
    from: string;
    /** ISO date-only string (`YYYY-MM-DD`), or `''` for "no bound". */
    to: string;
}

export interface DateRangePickerProps {
    from: string;
    to: string;
    onChange: (next: DateRangeValue) => void;
    /** Shown on the trigger pill when both `from` and `to` are empty. */
    placeholder?: string;
    className?: string;
}
