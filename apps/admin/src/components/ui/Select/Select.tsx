import * as React from 'react';
import { Check, ChevronDown } from 'lucide-react';
import { cn } from '@/lib/utils';
import * as Primitive from '../shadcn/select';
import { Popover, PopoverTrigger, PopoverContent } from '../shadcn/popover';
const FOCUS_RING = 'focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] outline-none';
const encode = (value: string) => `nexus:${value}`;
export function Select({ value, defaultValue, onValueChange, ...props }: React.ComponentProps<typeof Primitive.Select>) {
  return <Primitive.Select {...props} value={value === undefined ? undefined : encode(value)} defaultValue={defaultValue === undefined ? undefined : encode(defaultValue)} onValueChange={v => onValueChange?.(v.slice(6))} />;
}
export function SelectItem({ value, ...props }: React.ComponentProps<typeof Primitive.SelectItem>) {
  return <Primitive.SelectItem {...props} value={encode(value)} />;
}
export const SelectGroup = Primitive.SelectGroup;
export const SelectValue = Primitive.SelectValue;
export const SelectTrigger = Primitive.SelectTrigger;
export const SelectContent = Primitive.SelectContent;
export const SelectLabel = Primitive.SelectLabel;
export const SelectSeparator = Primitive.SelectSeparator;
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
        <Popover open={isOpen} onOpenChange={setIsOpen}><PopoverTrigger asChild>
            <button
                type="button"
                className={cn(
                    "inline-flex h-9 items-center gap-2 px-3 py-2 rounded-md text-sm shadow-xs", className,
                    "border transition-all",
                    FOCUS_RING,
                    activeLabel
                        ? "bg-action-primary/10 border-accent-blue/30 text-accent-blue"
                        : "bg-foreground/[0.06] border-border-primary text-text-primary hover:bg-foreground/[0.10]"
                )}
            >
                {icon || <ChevronDown className="w-4 h-4" />}
                <span className="text-xs font-semibold">
                    {activeLabel || label}
                </span>
                <ChevronDown className={cn(
                    "w-4 h-4 transition-transform",
                    isOpen && "rotate-180"
                )} />
            </button>

            </PopoverTrigger><PopoverContent align="start" className="w-64 p-1">
                        <div className="max-h-80 overflow-y-auto">
                            {options.map((option) => {
                                const isActive = isOptionActive(option.value);

                                return (
                                    <button
                                        key={option.id}
                                        onClick={() => handleOptionClick(option)}
                                        className={cn(
                                            "w-full flex items-center justify-between px-2 py-1.5 rounded-sm",
                                            "text-sm transition-colors",
                                            FOCUS_RING,
                                            isActive
                                                ? "bg-action-primary/10 text-accent-blue"
                                                : "text-text-primary hover:bg-foreground/[0.05]"
                                        )}
                                    >
                                        <div className="flex items-center gap-2">
                                            {multiselect && (
                                                <div className={cn(
                                                    "w-4 h-4 rounded border flex items-center justify-center",
                                                    isActive
                                                        ? "bg-action-primary border-accent-blue"
                                                        : "border-border-primary"
                                                )}>
                                                    {isActive && <Check className="w-3 h-3 text-action-foreground" />}
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
                    </PopoverContent>
        </Popover>
    );
};
