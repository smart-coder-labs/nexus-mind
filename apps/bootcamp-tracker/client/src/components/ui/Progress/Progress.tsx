import * as React from "react";
import { motion } from "framer-motion";
import { cn } from '../../../lib/utils';;

interface ProgressProps extends React.HTMLAttributes<HTMLDivElement> {
    value?: number;
    max?: number;
    indicatorClassName?: string;
}

const Progress = React.forwardRef<HTMLDivElement, ProgressProps>(
    ({ className, value = 0, max = 100, indicatorClassName, ...props }, ref) => {
        const clampedMax = max <= 0 ? 100 : max;
        const percentage = Math.max(0, Math.min(100, (value / clampedMax) * 100));

        return (
            <div
                ref={ref}
                role="progressbar"
                aria-valuenow={value}
                aria-valuemin={0}
                aria-valuemax={clampedMax}
                className={cn(
                    "relative h-2 w-full overflow-hidden rounded-full bg-surface-secondary",
                    className
                )}
                {...props}
            >
                <motion.div
                    className={cn("h-full bg-accent-blue", indicatorClassName)}
                    initial={{ width: 0 }}
                    animate={{ width: `${percentage}%` }}
                    transition={{ type: "spring", stiffness: 120, damping: 18 }}
                />
            </div>
        );
    }
);
Progress.displayName = "Progress";

export { Progress };
