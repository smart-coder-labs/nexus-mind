import React, { useRef, useState } from 'react';
import { Search, X } from 'lucide-react';
import { cn } from '../../../lib/utils';
import { motion, AnimatePresence } from 'framer-motion';
import { Spinner } from '../Spinner';
import { Label } from '../Label';

/* ========================================
   TYPES
   ======================================== */

export interface SearchInputProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'onChange'> {
    value: string;
    onChange: (value: string) => void;
    onSearch?: (value: string) => void;
    onClear?: () => void;
    isLoading?: boolean;
    debounceTime?: number; // Optional debounce for onSearch
    containerClassName?: string;
    label?: string;
}

/* ========================================
   SEARCH INPUT COMPONENT
   ======================================== */

export const SearchInput: React.FC<SearchInputProps> = ({
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
    ...props
}) => {
    const inputRef = useRef<HTMLInputElement>(null);
    const [isFocused, setIsFocused] = useState(false);

    const handleClear = () => {
        onChange('');
        onClear?.();
        inputRef.current?.focus();
    };

    const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
        if (e.key === 'Enter') {
            onSearch?.(value);
        }
        props.onKeyDown?.(e);
    };

    return (
        <div className={cn("relative w-full space-y-2", containerClassName)}>
            {label && (
                <Label htmlFor={props.id} className="mb-2">
                    {label}
                </Label>
            )}
            <div className="relative">
                <div className="absolute left-3 top-1/2 -translate-y-1/2 text-text-tertiary pointer-events-none">
                    <Search className={cn(
                        "w-4 h-4 transition-colors",
                        isFocused && "text-accent-blue"
                    )} />
                </div>

                <input
                    ref={inputRef}
                    type="text"
                    value={value}
                    onChange={(e) => onChange(e.target.value)}
                    onKeyDown={handleKeyDown}
                    onFocus={(e) => {
                        setIsFocused(true);
                        props.onFocus?.(e);
                    }}
                    onBlur={(e) => {
                        setIsFocused(false);
                        props.onBlur?.(e);
                    }}
                    disabled={disabled}
                    placeholder={placeholder}
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
                                className="flex items-center justify-center"
                            >
                                <Spinner size="sm" color="blue" />
                            </motion.div>
                        ) : value.length > 0 ? (
                            <motion.button
                                key="clear"
                                initial={{ opacity: 0, scale: 0.8 }}
                                animate={{ opacity: 1, scale: 1 }}
                                exit={{ opacity: 0, scale: 0.8 }}
                                onClick={handleClear}
                                disabled={disabled}
                                className="p-0.5 rounded-full hover:bg-surface-tertiary text-text-tertiary hover:text-text-primary transition-colors focus:outline-none"
                                type="button"
                                aria-label="Clear search"
                            >
                                <X className="w-3.5 h-3.5" />
                            </motion.button>
                        ) : null}
                    </AnimatePresence>
                </div>
            </div>
        </div>
    );
};

SearchInput.displayName = 'SearchInput';
