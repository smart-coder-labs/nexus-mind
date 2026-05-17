import * as React from "react";
import { Check, Minus } from "lucide-react";
import { cn } from "../../../lib/utils";
import type { CheckboxProps, CheckedState } from "./Checkbox.types";

const Checkbox = React.forwardRef<HTMLInputElement, CheckboxProps>(
    ({ className, checked, defaultChecked = false, onCheckedChange, disabled, ...props }, ref) => {
        const inputRef = React.useRef<HTMLInputElement | null>(null);
        const setRefs = React.useCallback(
            (node: HTMLInputElement | null) => {
                inputRef.current = node;
                if (typeof ref === "function") ref(node);
                else if (ref) (ref as React.MutableRefObject<HTMLInputElement | null>).current = node;
            },
            [ref]
        );

        const [internalChecked, setInternalChecked] = React.useState<CheckedState>(defaultChecked);
        const currentChecked = checked !== undefined ? checked : internalChecked;

        React.useEffect(() => {
            if (inputRef.current) {
                inputRef.current.indeterminate = currentChecked === "indeterminate";
            }
        }, [currentChecked]);

        const dataState = currentChecked === "indeterminate" ? "indeterminate" : currentChecked ? "checked" : "unchecked";

        const handleChange = (event: React.ChangeEvent<HTMLInputElement>) => {
            // If it was indeterminate, a user action should move to checked.
            const nextChecked: CheckedState = currentChecked === "indeterminate" ? true : event.target.checked;

            if (checked === undefined) {
                setInternalChecked(nextChecked);
            }

            onCheckedChange?.(nextChecked);
        };

        return (
            <span
                className="inline-flex items-center align-middle"
                data-state={dataState}
                aria-disabled={disabled ? true : undefined}
            >
                <input
                    ref={setRefs}
                    type="checkbox"
                    className={cn(
                        "peer h-5 w-5 shrink-0 rounded-md border border-border-primary shadow-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-accent-blue data-[state=checked]:text-white data-[state=checked]:border-accent-blue data-[state=indeterminate]:bg-accent-blue data-[state=indeterminate]:text-white data-[state=indeterminate]:border-accent-blue transition-all duration-200 appearance-none",
                        className
                    )}
                    role="checkbox"
                    aria-checked={dataState === "indeterminate" ? "mixed" : currentChecked as boolean}
                    data-state={dataState}
                    checked={currentChecked === true}
                    onChange={handleChange}
                    disabled={disabled}
                    {...props}
                />
                <span
                    className={cn(
                        "pointer-events-none -ml-5 flex h-5 w-5 items-center justify-center text-current align-middle",
                        "[input[data-state=unchecked]+&]:opacity-0"
                    )}
                    aria-hidden
                >
                    {dataState === "indeterminate" ? (
                        <Minus className="h-3 w-3 stroke-3" />
                    ) : (
                        <Check className="h-3.5 w-3.5 stroke-3" />
                    )}
                </span>
            </span>
        );
    }
);
Checkbox.displayName = "Checkbox";

export { Checkbox };
