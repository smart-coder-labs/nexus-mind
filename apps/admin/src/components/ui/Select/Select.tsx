import * as React from "react";
import { createPortal } from "react-dom";
import { Check, ChevronDown } from "lucide-react";
import { cn } from "../../../lib/utils";

type SelectContextValue = {
    open: boolean;
    setOpen: (open: boolean) => void;
    value?: string;
    setValue: (value: string) => void;
    registerItem: (value: string, label: string, disabled?: boolean) => void;
    unregisterItem: (value: string) => void;
    getLabel: (value?: string) => string | undefined;
    triggerRef: React.MutableRefObject<HTMLElement | null>;
    highlightedValue?: string;
    setHighlightedValue: (value?: string) => void;
    disabled?: boolean;
};

type SelectItemMeta = { value: string; label: string; disabled?: boolean };

const SelectContext = React.createContext<SelectContextValue | null>(null);

function useSelectContext() {
    const context = React.useContext(SelectContext);
    if (!context) {
        throw new Error("Select components must be used within Select");
    }
    return context;
}

type SelectProps = {
    value?: string;
    defaultValue?: string;
    onValueChange?: (value: string) => void;
    children: React.ReactNode;
    disabled?: boolean;
};

const Select = ({ value: controlledValue, defaultValue, onValueChange, children, disabled = false }: SelectProps) => {
    const [open, setOpen] = React.useState(false);
    const [uncontrolledValue, setUncontrolledValue] = React.useState<string | undefined>(defaultValue);
    const [items, setItems] = React.useState<Map<string, SelectItemMeta>>(new Map());
    const [highlightedValue, setHighlightedValue] = React.useState<string | undefined>();
    const [selectedLabel, setSelectedLabel] = React.useState<string | undefined>();
    const triggerRef = React.useRef<HTMLElement | null>(null);

    const currentValue = controlledValue ?? uncontrolledValue;

    // Keep selectedLabel in sync whenever items register or value changes externally
    React.useEffect(() => {
        if (currentValue && items.size > 0) {
            const label = items.get(currentValue)?.label;
            if (label) setSelectedLabel(label);
        }
    }, [items, currentValue]);

    const setValue = React.useCallback(
        (next: string) => {
            setSelectedLabel(items.get(next)?.label);
            if (controlledValue === undefined) {
                setUncontrolledValue(next);
            }
            onValueChange?.(next);
            setOpen(false);
        },
        [controlledValue, onValueChange, items]
    );

    const registerItem = React.useCallback((value: string, label: string, disabledItem?: boolean) => {
        setItems((prev) => {
            const next = new Map(prev);
            next.set(value, { value, label, disabled: disabledItem });
            return next;
        });
    }, []);

    const unregisterItem = React.useCallback((value: string) => {
        setItems((prev) => {
            const next = new Map(prev);
            next.delete(value);
            return next;
        });
    }, []);

    const getLabel = React.useCallback(
        (val?: string) => {
            if (!val) return undefined;
            return items.get(val)?.label ?? (val === currentValue ? selectedLabel : undefined);
        },
        [items, currentValue, selectedLabel]
    );

    React.useEffect(() => {
        if (open) {
            const enabledValues = Array.from(items.values())
                .filter((item) => !item.disabled)
                .map((item) => item.value);
            setHighlightedValue(currentValue ?? enabledValues[0]);
        }
    }, [open, currentValue, items]);

    const contextValue: SelectContextValue = React.useMemo(
        () => ({
            open,
            setOpen,
            value: currentValue,
            setValue,
            registerItem,
            unregisterItem,
            getLabel,
            triggerRef,
            highlightedValue,
            setHighlightedValue,
            disabled,
        }),
        [open, currentValue, setValue, registerItem, unregisterItem, getLabel, highlightedValue, disabled]
    );

    return <SelectContext.Provider value={contextValue}>{children}</SelectContext.Provider>;
};

type SelectTriggerProps = React.ButtonHTMLAttributes<HTMLButtonElement> & {
    asChild?: boolean;
    children: React.ReactNode;
};

const SelectTrigger = React.forwardRef<HTMLElement, SelectTriggerProps>(
    ({ className, children, asChild = false, disabled, ...props }, ref) => {
        const { open, setOpen, triggerRef, disabled: groupDisabled } = useSelectContext();
        const mergedDisabled = disabled || groupDisabled;
        const mergedRef = (node: HTMLElement | null) => {
            triggerRef.current = node;
            if (typeof ref === "function") ref(node as any);
            else if (ref) (ref as React.MutableRefObject<HTMLElement | null>).current = node;
        };

        const handleClick = (e: React.MouseEvent<HTMLElement>) => {
            if (mergedDisabled) return;
            props.onClick?.(e as React.MouseEvent<HTMLButtonElement>);
            if (!e.defaultPrevented) setOpen(!open);
        };

        if (asChild && React.isValidElement(children)) {
            return React.cloneElement(children as any, {
                ref: mergedRef,
                onClick: handleClick,
                className: cn((children as any).props?.className, className),
                "aria-haspopup": "listbox",
                "aria-expanded": open,
                "aria-disabled": mergedDisabled || undefined,
            });
        }

        return (
            <button
                type="button"
                ref={mergedRef as React.Ref<HTMLButtonElement>}
                onClick={(e) => handleClick(e)}
                aria-haspopup="listbox"
                aria-expanded={open}
                aria-disabled={mergedDisabled || undefined}
                disabled={mergedDisabled}
                className={cn(
                    "group flex h-10 w-full items-center justify-between rounded-lg border border-border-primary bg-surface-primary px-3 py-2 text-sm ring-offset-background placeholder:text-text-tertiary focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 [&>span]:line-clamp-1 hover:bg-surface-secondary/50 transition-colors",
                    className
                )}
                {...props}
            >
                {children}
                <ChevronDown className="h-4 w-4 opacity-50 group-data-[state=open]:rotate-180 transition-transform duration-200" />
            </button>
        );
    }
);
SelectTrigger.displayName = "SelectTrigger";

type SelectValueProps = {
    placeholder?: string;
    className?: string;
    children?: React.ReactNode;
};

const SelectValue: React.FC<SelectValueProps> = ({ placeholder = "Select an option", className, children }) => {
    const { value, getLabel } = useSelectContext();
    const label = getLabel(value);
    return <span className={cn("truncate", className)}>{label ?? children ?? placeholder}</span>;
};

type SelectContentProps = {
    className?: string;
    children: React.ReactNode;
};

const SelectContent = React.forwardRef<HTMLDivElement, SelectContentProps>(({ className, children }, ref) => {
    const { open, setOpen, triggerRef, highlightedValue, setHighlightedValue } = useSelectContext();
    const contentRef = React.useRef<HTMLDivElement | null>(null);
    const mergedRef = (node: HTMLDivElement | null) => {
        contentRef.current = node;
        if (typeof ref === "function") ref(node);
        else if (ref) (ref as React.MutableRefObject<HTMLDivElement | null>).current = node;
    };
    const [position, setPosition] = React.useState({ top: 0, left: 0, width: 0 });

    const updatePosition = React.useCallback(() => {
        const trigger = triggerRef.current;
        const content = contentRef.current;
        if (!trigger || !content) return;
        const rect = trigger.getBoundingClientRect();
        const width = rect.width;
        let left = rect.left;
        let top = rect.bottom + 4;
        const contentRect = content.getBoundingClientRect();
        if (left + contentRect.width > window.innerWidth - 8) {
            left = window.innerWidth - contentRect.width - 8;
        }
        if (left < 8) left = 8;
        if (top + contentRect.height > window.innerHeight - 8) {
            top = rect.top - contentRect.height - 4;
        }
        setPosition({ top, left, width });
    }, [triggerRef]);

    React.useLayoutEffect(() => {
        if (open) updatePosition();
    }, [open, updatePosition]);

    React.useEffect(() => {
        if (!open) return;
        const handleClick = (event: MouseEvent) => {
            const target = event.target as Node;
            if (contentRef.current?.contains(target) || triggerRef.current?.contains(target)) return;
            setOpen(false);
        };
        const handleKeyDown = (event: KeyboardEvent) => {
            if (!open) return;
            const items = contentRef.current?.querySelectorAll<HTMLButtonElement>("[data-select-item]:not([data-disabled=true])") ?? [];
            const enabledValues = Array.from(items).map((el) => el.dataset.value!).filter(Boolean);
            if (event.key === "Escape") {
                setOpen(false);
            }
            if (enabledValues.length === 0) return;
            const currentIndex = highlightedValue ? enabledValues.indexOf(highlightedValue) : -1;
            if (event.key === "ArrowDown") {
                event.preventDefault();
                const next = enabledValues[(currentIndex + 1) % enabledValues.length];
                setHighlightedValue(next);
            }
            if (event.key === "ArrowUp") {
                event.preventDefault();
                const prev = enabledValues[(currentIndex - 1 + enabledValues.length) % enabledValues.length];
                setHighlightedValue(prev);
            }
            if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                const targetValue = highlightedValue ?? enabledValues[0];
                if (targetValue) {
                    const el = contentRef.current?.querySelector<HTMLButtonElement>(`[data-select-item="${targetValue}"]`);
                    el?.click();
                }
            }
        };
        const handleResize = () => updatePosition();
        document.addEventListener("mousedown", handleClick);
        document.addEventListener("keydown", handleKeyDown);
        window.addEventListener("resize", handleResize);
        window.addEventListener("scroll", handleResize, true);
        return () => {
            document.removeEventListener("mousedown", handleClick);
            document.removeEventListener("keydown", handleKeyDown);
            window.removeEventListener("resize", handleResize);
            window.removeEventListener("scroll", handleResize, true);
        };
    }, [open, setOpen, highlightedValue, setHighlightedValue, updatePosition]);

    if (!open) return null;

    return createPortal(
        <div
            ref={mergedRef}
            role="listbox"
            aria-activedescendant={highlightedValue ? `select-item-${highlightedValue}` : undefined}
            className={cn(
                "relative z-50 min-w-[8rem] overflow-hidden rounded-[18px] border border-border-primary bg-surface-glass backdrop-blur-xl text-text-primary p-1",
                className
            )}
            style={{ position: "fixed", top: position.top, left: position.left, minWidth: position.width }}
            data-state={open ? "open" : "closed"}
        >
            {children}
        </div>,
        document.body
    );
});
SelectContent.displayName = "SelectContent";

type SelectLabelProps = React.HTMLAttributes<HTMLDivElement>;
const SelectLabel = React.forwardRef<HTMLDivElement, SelectLabelProps>(({ className, ...props }, ref) => (
    <div ref={ref} className={cn("py-1.5 pl-8 pr-2 text-sm font-semibold", className)} {...props} />
));
SelectLabel.displayName = "SelectLabel";

type SelectItemProps = {
    value: string;
    disabled?: boolean;
    children: React.ReactNode;
    className?: string;
};

const SelectItem = React.forwardRef<HTMLButtonElement, SelectItemProps>(
    ({ value, disabled = false, children, className, ...props }, ref) => {
        const { value: selectedValue, setValue, registerItem, unregisterItem, highlightedValue, setHighlightedValue } = useSelectContext();
        const label = React.useMemo(() => {
            if (typeof children === "string") return children;
            if (React.isValidElement(children)) {
                const element = children as React.ReactElement;
                return (element.props as any)?.children ?? value;
            }
            return value;
        }, [children, value]);

        React.useEffect(() => {
            registerItem(value, String(label), disabled);
            return () => unregisterItem(value);
        }, [value, label, disabled, registerItem, unregisterItem]);

        const isSelected = selectedValue === value;
        const isHighlighted = highlightedValue === value;

        return (
            <button
                ref={ref}
                id={`select-item-${value}`}
                role="option"
                type="button"
                data-select-item={value}
                data-value={value}
                data-disabled={disabled ? "true" : undefined}
                aria-selected={isSelected}
                disabled={disabled}
                onMouseEnter={() => setHighlightedValue(value)}
                onFocus={() => setHighlightedValue(value)}
                onClick={() => !disabled && setValue(value)}
                className={cn(
                    "relative flex w-full cursor-default select-none items-center rounded-md py-1.5 pl-8 pr-2 text-sm outline-none transition-colors",
                    isSelected
                        ? "bg-accent-blue text-white"
                        : isHighlighted
                            ? "bg-surface-secondary text-text-primary"
                            : "text-text-primary",
                    disabled && "pointer-events-none opacity-50",
                    className
                )}
                {...props}
            >
                <span className="absolute left-2 flex h-3.5 w-3.5 items-center justify-center">
                    {isSelected && <Check className="h-4 w-4" />}
                </span>
                <span className="truncate">{children}</span>
            </button>
        );
    }
);
SelectItem.displayName = "SelectItem";

type SelectSeparatorProps = React.HTMLAttributes<HTMLDivElement>;
const SelectSeparator = React.forwardRef<HTMLDivElement, SelectSeparatorProps>(({ className, ...props }, ref) => (
    <div ref={ref} className={cn("-mx-1 my-1 h-px bg-border-primary", className)} {...props} />
));
SelectSeparator.displayName = "SelectSeparator";

const SelectGroup: React.FC<{ className?: string; children: React.ReactNode }> = ({ className, children }) => (
    <div className={cn("py-1", className)}>{children}</div>
);


/* ========================================
   FILTER SELECT VARIANT
   ======================================== */

export interface FilterSelectOption {
    id: string;
    label: string;
    value: string;
    count?: number;
}

export interface FilterSelectProps {
    label: string;
    options: FilterSelectOption[];
    value?: string | string[];
    onChange?: (value: string | string[]) => void;
    icon?: React.ReactNode;
    multiselect?: boolean;
    className?: string;
}

export const FilterSelect: React.FC<FilterSelectProps> = ({
    label,
    options,
    value,
    onChange,
    icon,
    multiselect = false,
    className = '',
}) => {
    const [isOpen, setIsOpen] = React.useState(false);

    const handleOptionClick = (option: FilterSelectOption) => {
        if (multiselect) {
            const currentValues = Array.isArray(value) ? value : [];
            const newValues = currentValues.includes(option.value)
                ? currentValues.filter(v => v !== option.value)
                : [...currentValues, option.value];
            onChange?.(newValues);
        } else {
            onChange?.(option.value);
            setIsOpen(false);
        }
    };

    const isOptionActive = (optionValue: string) => {
        if (Array.isArray(value)) {
            return value.includes(optionValue);
        }
        return value === optionValue;
    };

    const getActiveLabel = () => {
        if (!value) return null;

        if (Array.isArray(value)) {
            if (value.length === 0) return null;
            if (value.length === 1) {
                const option = options.find(o => o.value === value[0]);
                return option?.label;
            }
            return `${value.length} selected`;
        }

        const option = options.find(o => o.value === value);
        return option?.label;
    };

    const activeLabel = getActiveLabel();

    return (
        <div className={cn("relative", className)}>
            <button
                onClick={() => setIsOpen(!isOpen)}
                className={cn(
                    "inline-flex items-center gap-2 px-4 py-2 rounded-lg",
                    "border transition-all",
                    activeLabel
                        ? "bg-accent-blue/10 border-accent-blue/30 text-accent-blue"
                        : "bg-surface-secondary border-border-primary text-text-primary hover:bg-surface-tertiary"
                )}
            >
                {icon || <ChevronDown className="w-4 h-4" />}
                <span className="text-sm font-semibold">
                    {activeLabel || label}
                </span>
                <ChevronDown className={cn(
                    "w-4 h-4 transition-transform",
                    isOpen && "rotate-180"
                )} />
            </button>

            {isOpen && (
                <>
                    <div
                        className="fixed inset-0 z-10"
                        onClick={() => setIsOpen(false)}
                    />
                    <div className="absolute top-full left-0 mt-2 w-64 bg-surface-primary border border-border-primary rounded-[18px] z-20 overflow-hidden">
                        <div className="max-h-80 overflow-y-auto p-2">
                            {options.map((option) => {
                                const isActive = isOptionActive(option.value);

                                return (
                                    <button
                                        key={option.id}
                                        onClick={() => handleOptionClick(option)}
                                        className={cn(
                                            "w-full flex items-center justify-between px-3 py-2 rounded-lg",
                                            "text-sm transition-colors",
                                            isActive
                                                ? "bg-accent-blue/10 text-accent-blue"
                                                : "text-text-primary hover:bg-surface-secondary"
                                        )}
                                    >
                                        <div className="flex items-center gap-2">
                                            {multiselect && (
                                                <div className={cn(
                                                    "w-4 h-4 rounded border flex items-center justify-center",
                                                    isActive
                                                        ? "bg-accent-blue border-accent-blue"
                                                        : "border-border-primary"
                                                )}>
                                                    {isActive && <Check className="w-3 h-3 text-white" />}
                                                </div>
                                            )}
                                            <span>{option.label}</span>
                                        </div>
                                        {option.count !== undefined && (
                                            <span className="text-xs text-text-tertiary">
                                                {option.count}
                                            </span>
                                        )}
                                    </button>
                                );
                            })}
                        </div>
                    </div>
                </>
            )}
        </div>
    );
};

export {
    Select,
    SelectGroup,
    SelectValue,
    SelectTrigger,
    SelectContent,
    SelectLabel,
    SelectItem,
    SelectSeparator,
};
