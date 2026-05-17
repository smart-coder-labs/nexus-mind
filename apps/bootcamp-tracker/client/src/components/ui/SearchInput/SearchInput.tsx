import React, { useRef, useState, useEffect, useCallback, createContext, useContext } from 'react';
import { Search, X } from 'lucide-react';
import { cn } from '../../../lib/utils';
import { motion, AnimatePresence } from 'framer-motion';
import { Spinner } from '../Spinner';
import { Label } from '../Label';
import {
    searchInputVariants,
    SearchInputProps,
    SearchInputContextValue,
    SearchInputInputProps,
    SearchInputDropdownProps,
    SearchInputSectionProps,
    SearchInputItemProps,
    SearchInputItemContentProps,
    SearchInputItemIconProps,
    SearchInputTrailingBadgeProps,
} from './SearchInput.types';

/* ========================================
   CONTEXT
   ======================================== */

const SearchInputContext = createContext<SearchInputContextValue | null>(null);

function useSearchInputContext() {
    const context = useContext(SearchInputContext);
    if (!context) {
        throw new Error("SearchInput compound components must be used within SearchInput");
    }
    return context;
}

/* ========================================
   HOOKS
   ======================================== */

function useDebounce<T extends (...args: Parameters<T>) => ReturnType<T>>(
    callback: T,
    delay: number
): T {
    const timeoutRef = React.useRef<NodeJS.Timeout | null>(null);

    useEffect(() => {
        return () => {
            if (timeoutRef.current) {
                clearTimeout(timeoutRef.current);
            }
        };
    }, []);

    return useCallback(
        ((...args: Parameters<T>) => {
            if (timeoutRef.current) {
                clearTimeout(timeoutRef.current);
            }
            timeoutRef.current = setTimeout(() => {
                callback(...args);
            }, delay);
        }) as T,
        [callback, delay]
    );
}

/* ========================================
   COMPOUND COMPONENTS
   ======================================== */

/* -------- SearchInput.Input -------- */

export const SearchInputInput = React.forwardRef<HTMLInputElement, SearchInputInputProps>(
    ({ value, onChange, onSearch, onClear, isLoading = false, placeholder, disabled, id, onFocus, onBlur, onKeyDown, className, ...props }, ref) => {
        const { isFocused, setIsFocused } = useSearchInputContext();

        const handleClear = () => {
            onChange('');
            onClear?.();
            (ref as React.RefObject<HTMLInputElement>)?.current?.focus();
        };

        const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
            if (e.key === 'Enter') {
                onSearch?.(String(value));
            }
            onKeyDown?.(e);
        };

        return (
            <div className="relative">
                <div className="absolute left-3 top-1/2 -translate-y-1/2 text-text-tertiary pointer-events-none">
                    <Search className={cn("w-4 h-4 transition-colors", isFocused && "text-accent-blue")} />
                </div>

                <input
                    ref={ref}
                    type="text"
                    value={value}
                    onChange={(e) => onChange(e.target.value)}
                    onKeyDown={handleKeyDown}
                    onFocus={(e) => {
                        setIsFocused(true);
                        onFocus?.(e);
                    }}
                    onBlur={(e) => {
                        setIsFocused(false);
                        onBlur?.(e);
                    }}
                    disabled={disabled}
                    placeholder={placeholder}
                    id={id}
                    className={cn(
                        "w-full h-10 pl-9 pr-10 rounded-xl border bg-surface-primary text-text-primary transition-all",
                        "placeholder:text-text-tertiary",
                        "focus:outline-none focus:ring-2 focus:ring-accent-blue/20 focus:border-accent-blue",
                        "disabled:opacity-50 disabled:cursor-not-allowed",
                        "border-border-primary",
                        className
                    )}
                    {...props}
                />

                <div className="absolute right-3 top-1/2 -translate-y-1/2 flex items-center">
                    <AnimatePresence mode="wait">
                        {isLoading ? (
                            <motion.div
                                key="loader"
                                initial={{ opacity: 0, scale: 0.8 }}
                                animate={{ opacity: 1, scale: 1 }}
                                exit={{ opacity: 0, scale: 0.8 }}
                            >
                                <Spinner size="sm" color="blue" />
                            </motion.div>
                        ) : value && String(value).length > 0 ? (
                            <motion.button
                                key="clear"
                                type="button"
                                initial={{ opacity: 0, scale: 0.8 }}
                                animate={{ opacity: 1, scale: 1 }}
                                exit={{ opacity: 0, scale: 0.8 }}
                                onClick={handleClear}
                                disabled={disabled}
                                className="p-0.5 rounded-full hover:bg-surface-tertiary text-text-tertiary hover:text-text-primary transition-colors focus:outline-none"
                                aria-label="Clear search"
                            >
                                <X className="w-3.5 h-3.5" />
                            </motion.button>
                        ) : null}
                    </AnimatePresence>
                </div>
            </div>
        );
    }
);
SearchInputInput.displayName = 'SearchInputInput';

/* -------- SearchInput.Dropdown -------- */

export const SearchInputDropdown: React.FC<SearchInputDropdownProps> = ({
    show,
    hasResults = false,
    query,
    children,
    className,
    maxHeight = '50vh'
}) => {
    return (
        <AnimatePresence>
            {show && (
                <motion.div
                    initial={{ opacity: 0, y: -4 }}
                    animate={{ opacity: 1, y: 0 }}
                    exit={{ opacity: 0, y: -4 }}
                    transition={{ duration: 0.15 }}
                    className={cn(
                        "absolute top-full left-0 right-0 mt-1 rounded-lg border shadow-2xl z-50 overflow-y-auto",
                        className
                    )}
                    style={{
                        backgroundColor: 'var(--color-bg-secondary)',
                        borderColor: 'var(--color-border-primary)',
                        maxHeight,
                    }}
                >
                    {!hasResults && query && (
                        <div className="px-4 py-3 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                            No results for "{query}"
                        </div>
                    )}
                    {children}
                </motion.div>
            )}
        </AnimatePresence>
    );
};
SearchInputDropdown.displayName = 'SearchInputDropdown';

/* -------- SearchInput.Section -------- */

export const SearchInputSection: React.FC<SearchInputSectionProps> = ({
    title,
    children,
    className
}) => {
    return (
        <div className={cn(className)}>
            <div
                className="px-3 py-1.5 text-xs font-medium uppercase tracking-wider"
                style={{
                    color: 'var(--color-text-tertiary)',
                    borderBottom: '1px solid var(--color-border-primary)'
                }}
            >
                {title}
            </div>
            {children}
        </div>
    );
};
SearchInputSection.displayName = 'SearchInputSection';

/* -------- SearchInput.Item -------- */

export const SearchInputItem: React.FC<SearchInputItemProps> = ({
    children,
    onClick,
    className
}) => {
    const { disabled } = useSearchInputContext();

    if (onClick) {
        return (
            <button
                type="button"
                onClick={onClick}
                disabled={disabled}
                className={cn(
                    "w-full flex items-center gap-3 px-3 py-2.5 hover:bg-white/5 text-left transition-colors",
                    disabled && "opacity-50 cursor-not-allowed",
                    className
                )}
            >
                {children}
            </button>
        );
    }

    return (
        <div className={cn("flex items-center gap-3 px-3 py-2.5", className)}>
            {children}
        </div>
    );
};
SearchInputItem.displayName = 'SearchInputItem';

/* -------- SearchInput.ItemContent -------- */

export const SearchInputItemContent: React.FC<SearchInputItemContentProps> = ({
    label,
    subtitle,
    className
}) => {
    return (
        <div className={cn("flex-1 min-w-0", className)}>
            <div className="text-sm truncate" style={{ color: 'var(--color-text-primary)' }}>
                {label}
            </div>
            {subtitle && (
                <div className="text-xs truncate" style={{ color: 'var(--color-text-tertiary)' }}>
                    {subtitle}
                </div>
            )}
        </div>
    );
};
SearchInputItemContent.displayName = 'SearchInputItemContent';

/* -------- SearchInput.ItemIcon -------- */

const defaultIcons: Record<string, string> = {
    paper: '📄',
    book: '📚',
    course: '🎓',
    website: '💻'
};

export const SearchInputItemIcon: React.FC<SearchInputItemIconProps> = ({
    type,
    icon,
    className
}) => {
    return (
        <span className={cn("text-base", className)}>
            {icon ?? (type ? defaultIcons[type] : null)}
        </span>
    );
};
SearchInputItemIcon.displayName = 'SearchInputItemIcon';

/* -------- SearchInput.TrailingBadge -------- */

const badgeVariants = {
    default: "text-gray-400 bg-gray-800/50",
    primary: "text-blue-400 bg-blue-900/30",
    success: "text-green-400 bg-green-900/30",
    warning: "text-yellow-400 bg-yellow-900/30",
    error: "text-red-400 bg-red-900/30",
};

export const SearchInputTrailingBadge: React.FC<SearchInputTrailingBadgeProps> = ({
    children,
    variant = 'default',
    className
}) => {
    return (
        <span className={cn(
            "text-xs font-mono px-1.5 py-0.5 rounded",
            badgeVariants[variant],
            className
        )}>
            {children}
        </span>
    );
};
SearchInputTrailingBadge.displayName = 'SearchInputTrailingBadge';

/* ========================================
   MAIN COMPONENT
   ======================================== */

const SearchInputRoot = React.forwardRef<HTMLDivElement, SearchInputProps>(
    (
        {
            value,
            onChange,
            onSearch,
            onClear,
            isLoading = false,
            debounceTime = 0,
            className,
            containerClassName,
            placeholder = 'Search...',
            disabled,
            label,
            id,
            onFocus,
            onBlur,
            onKeyDown,
            children,
            size,
            variant,
        },
        ref
    ) => {
        const inputRef = useRef<HTMLInputElement>(null);
        const [isFocused, setIsFocused] = useState(false);
        const [localValue, setLocalValue] = useState(value);

        // Sync external value
        useEffect(() => {
            setLocalValue(value);
        }, [value]);

        // Debounced onSearch
        const debouncedSearch = useDebounce(
            (val: string) => {
                if (val.trim()) {
                    onSearch?.(val);
                }
            },
            debounceTime
        );

        const handleChange = (newValue: string) => {
            setLocalValue(newValue);
            onChange(newValue);
            if (debounceTime > 0) {
                debouncedSearch(newValue);
            }
        };

        const handleClear = () => {
            setLocalValue('');
            onChange('');
            onClear?.();
            inputRef.current?.focus();
        };

        const handleInputFocus = (e: React.FocusEvent<HTMLInputElement>) => {
            setIsFocused(true);
            onFocus?.(e);
        };

        const handleInputBlur = (e: React.FocusEvent<HTMLInputElement>) => {
            setIsFocused(false);
            onBlur?.(e);
        };

        const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
            if (e.key === 'Enter' && debounceTime === 0) {
                onSearch?.(localValue);
            }
            onKeyDown?.(e);
        };

        const contextValue: SearchInputContextValue = {
            isFocused,
            setIsFocused,
            isLoading,
            disabled,
            inputRef,
        };

        // Render with compound components
        if (children) {
            return (
                <SearchInputContext.Provider value={contextValue}>
                    <div ref={ref} className={cn("relative w-full space-y-2", containerClassName)}>
                        {label && (
                            <Label htmlFor={id} className="mb-2">
                                {label}
                            </Label>
                        )}
                        {/* Input is rendered by consumer via SearchInput.Input */}
                        {children}
                    </div>
                </SearchInputContext.Provider>
            );
        }

        // Default render (backwards compatible)
        return (
            <SearchInputContext.Provider value={contextValue}>
                <div ref={ref} className={cn("relative w-full space-y-2", containerClassName)}>
                    {label && (
                        <Label htmlFor={id} className="mb-2">
                            {label}
                        </Label>
                    )}
                    <div className="relative">
                        <div className="absolute left-3 top-1/2 -translate-y-1/2 text-text-tertiary pointer-events-none">
                            <Search className={cn("w-4 h-4 transition-colors", isFocused && "text-accent-blue")} />
                        </div>

                        <input
                            ref={inputRef}
                            type="text"
                            value={localValue}
                            onChange={(e) => handleChange(e.target.value)}
                            onKeyDown={handleKeyDown}
                            onFocus={handleInputFocus}
                            onBlur={handleInputBlur}
                            disabled={disabled}
                            placeholder={placeholder}
                            id={id}
                            className={cn(
                                searchInputVariants({ size, variant }),
                                className,
                                "placeholder:text-text-tertiary",
                                "focus:outline-none focus:ring-2 focus:ring-accent-blue/20 focus:border-accent-blue",
                                "border-border-primary"
                            )}
                        />

                        <div className="absolute right-3 top-1/2 -translate-y-1/2 flex items-center">
                            <AnimatePresence mode="wait">
                                {isLoading ? (
                                    <motion.div
                                        key="loader"
                                        initial={{ opacity: 0, scale: 0.8 }}
                                        animate={{ opacity: 1, scale: 1 }}
                                        exit={{ opacity: 0, scale: 0.8 }}
                                    >
                                        <Spinner size="sm" color="blue" />
                                    </motion.div>
                                ) : localValue && localValue.length > 0 ? (
                                    <motion.button
                                        key="clear"
                                        type="button"
                                        initial={{ opacity: 0, scale: 0.8 }}
                                        animate={{ opacity: 1, scale: 1 }}
                                        exit={{ opacity: 0, scale: 0.8 }}
                                        onClick={handleClear}
                                        disabled={disabled}
                                        className="p-0.5 rounded-full hover:bg-surface-tertiary text-text-tertiary hover:text-text-primary transition-colors focus:outline-none"
                                        aria-label="Clear search"
                                    >
                                        <X className="w-3.5 h-3.5" />
                                    </motion.button>
                                ) : null}
                            </AnimatePresence>
                        </div>
                    </div>
                </div>
            </SearchInputContext.Provider>
        );
    }
);

SearchInputRoot.displayName = 'SearchInput';

/* ========================================
   COMPOUND COMPONENT ATTACHMENTS
   ======================================== */

type SearchInputComponent = typeof SearchInputRoot & {
    Input: typeof SearchInputInput;
    Dropdown: typeof SearchInputDropdown;
    Section: typeof SearchInputSection;
    Item: typeof SearchInputItem;
    ItemContent: typeof SearchInputItemContent;
    ItemIcon: typeof SearchInputItemIcon;
    TrailingBadge: typeof SearchInputTrailingBadge;
};

const SearchInput = SearchInputRoot as SearchInputComponent;

SearchInput.Input = SearchInputInput;
SearchInput.Dropdown = SearchInputDropdown;
SearchInput.Section = SearchInputSection;
SearchInput.Item = SearchInputItem;
SearchInput.ItemContent = SearchInputItemContent;
SearchInput.ItemIcon = SearchInputItemIcon;
SearchInput.TrailingBadge = SearchInputTrailingBadge;

export { SearchInput };
export { searchInputVariants } from './SearchInput.types';
export type { SearchInputComponent, SearchInputProps };

// Export compound component types
export type {
    SearchInputInputProps,
    SearchInputDropdownProps,
    SearchInputSectionProps,
    SearchInputItemProps,
    SearchInputItemContentProps,
    SearchInputItemIconProps,
    SearchInputTrailingBadgeProps,
} from './SearchInput.types';