import * as React from "react";
import { motion } from "framer-motion";
import { cn } from "../../../lib/utils";
import type { SpinnerProps } from "./Spinner.types";

const Spinner = React.forwardRef<HTMLDivElement, SpinnerProps>(
    ({ className, size = "md", color = "default", ...props }, ref) => {
        const sizeClasses = {
            sm: "h-4 w-4",
            md: "h-6 w-6",
            lg: "h-8 w-8",
        };

        const colorClasses = {
            default: "bg-text-secondary",
            white: "bg-white",
            blue: "bg-accent-blue",
        };

        return (
            <div
                ref={ref}
                className={cn("relative inline-block", sizeClasses[size], className)}
                {...props}
                role="status"
                aria-label="Loading"
            >
                {Array.from({ length: 12 }).map((_, i) => (
                    <div
                        key={i}
                        className="absolute inset-0"
                        style={{ transform: `rotate(${i * 30}deg)` }}
                    >
                        <motion.span
                            className={cn(
                                "absolute left-1/2 top-0 h-[25%] w-[8%] -translate-x-1/2 rounded-full",
                                colorClasses[color]
                            )}
                            initial={{ opacity: 0.2 }}
                            animate={{ opacity: [1, 0.2] }}
                            transition={{
                                duration: 1.2,
                                repeat: Infinity,
                                delay: i * 0.1,
                                ease: "linear",
                            }}
                        />
                    </div>
                ))}
            </div>
        );
    }
);
Spinner.displayName = "Spinner";

export { Spinner };