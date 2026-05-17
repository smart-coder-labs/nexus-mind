import React, { forwardRef } from 'react';
import { motion } from 'framer-motion';
import { cn } from '../../../lib/utils';
import { TrendingUp, TrendingDown, Minus } from 'lucide-react';

/* ========================================
   TYPES
   ======================================== */

export type KPITrend = 'up' | 'down' | 'neutral';
export type KPIVariant = 'default' | 'bordered' | 'elevated' | 'minimal';
export type KPISize = 'sm' | 'md' | 'lg';

export interface KPIBlockProps extends React.HTMLAttributes<HTMLDivElement> {
    label: React.ReactNode;
    value: React.ReactNode;
    change?: string;
    trend?: KPITrend;
    icon?: React.ReactNode;
    variant?: KPIVariant;
    size?: KPISize;
    description?: React.ReactNode;
    loading?: boolean;
}

/* ========================================
   KPI BLOCK COMPONENT
   ======================================== */

export const KPIBlock = forwardRef<HTMLDivElement, KPIBlockProps>(
    (
        {
            label,
            value,
            change,
            trend,
            icon,
            variant = 'default',
            size = 'md',
            description,
            loading = false,
            className,
            ...props
        },
        ref
    ) => {
        const sizeClasses = {
            sm: {
                label: "text-xs",
                value: "text-2xl",
                change: "text-xs",
                description: "text-xs",
                padding: "p-4",
                iconSize: "w-8 h-8",
            },
            md: {
                label: "text-sm",
                value: "text-4xl",
                change: "text-sm",
                description: "text-sm",
                padding: "p-6",
                iconSize: "w-10 h-10",
            },
            lg: {
                label: "text-base",
                value: "text-5xl",
                change: "text-base",
                description: "text-base",
                padding: "p-8",
                iconSize: "w-12 h-12",
            },
        };

        const variantClasses = {
            default: "bg-surface-primary",
            bordered: "bg-surface-primary border border-border-primary shadow-sm",
            elevated: "bg-surface-primary shadow-md hover:shadow-lg transition-shadow",
            minimal: "bg-transparent",
        };

        const trendConfig = {
            up: {
                icon: TrendingUp,
                color: "text-status-success",
                bg: "bg-status-success/10",
            },
            down: {
                icon: TrendingDown,
                color: "text-status-error",
                bg: "bg-status-error/10",
            },
            neutral: {
                icon: Minus,
                color: "text-text-tertiary",
                bg: "bg-surface-secondary",
            },
        };

        const TrendIcon = trend ? trendConfig[trend].icon : null;

        const MotionDiv = motion.div as any;

        return (
            <MotionDiv
                ref={ref}
                className={cn(
                    "rounded-xl overflow-hidden",
                    variantClasses[variant],
                    sizeClasses[size].padding,
                    className
                )}
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{
                    duration: 0.22,
                    ease: [0.16, 1, 0.3, 1],
                }}
                {...props}
            >
                <div className="flex items-start justify-between gap-4">
                    <div className="flex-1 min-w-0">
                        {/* Label */}
                        <div className={cn(
                            "font-medium text-text-secondary mb-2",
                            sizeClasses[size].label
                        )}>
                            {label}
                        </div>

                        {/* Value */}
                        {loading ? (
                            <div className="space-y-3">
                                <div className={cn(
                                    "h-10 bg-surface-secondary rounded-lg animate-pulse",
                                    size === 'sm' && "h-8",
                                    size === 'lg' && "h-12"
                                )} />
                                {change && (
                                    <div className="h-5 w-20 bg-surface-secondary rounded animate-pulse" />
                                )}
                            </div>
                        ) : (
                            <>
                                <div className={cn(
                                    "font-bold text-text-primary mb-2 tracking-tight",
                                    sizeClasses[size].value
                                )}>
                                    {value}
                                </div>

                                {/* Change and Trend */}
                                {(change || trend) && (
                                    <div className="flex items-center gap-2">
                                        {trend && TrendIcon && (
                                            <div className={cn(
                                                "flex items-center gap-1 px-2 py-1 rounded-md",
                                                trendConfig[trend].bg
                                            )}>
                                                <TrendIcon className={cn(
                                                    "w-3.5 h-3.5",
                                                    trendConfig[trend].color
                                                )} />
                                                {change && (
                                                    <span className={cn(
                                                        "font-semibold",
                                                        trendConfig[trend].color,
                                                        sizeClasses[size].change
                                                    )}>
                                                        {change}
                                                    </span>
                                                )}
                                            </div>
                                        )}
                                        {change && !trend && (
                                            <span className={cn(
                                                "font-medium text-text-secondary",
                                                sizeClasses[size].change
                                            )}>
                                                {change}
                                            </span>
                                        )}
                                    </div>
                                )}

                                {/* Description */}
                                {description && (
                                    <div className={cn(
                                        "text-text-tertiary mt-2",
                                        sizeClasses[size].description
                                    )}>
                                        {description}
                                    </div>
                                )}
                            </>
                        )}
                    </div>

                    {/* Icon */}
                    {icon && (
                        <div className={cn(
                            "flex-shrink-0 flex items-center justify-center rounded-xl bg-accent-blue/10 text-accent-blue",
                            sizeClasses[size].iconSize
                        )}>
                            {icon}
                        </div>
                    )}
                </div>
            </MotionDiv>
        );
    }
);

KPIBlock.displayName = 'KPIBlock';

/* ========================================
   KPI GROUP COMPONENT
   ======================================== */

export interface KPIGroupProps extends React.HTMLAttributes<HTMLDivElement> {
    children: React.ReactNode;
    columns?: 1 | 2 | 3 | 4;
    gap?: 'sm' | 'md' | 'lg';
}

export const KPIGroup = forwardRef<HTMLDivElement, KPIGroupProps>(
    (
        {
            children,
            columns = 3,
            gap = 'md',
            className,
            ...props
        },
        ref
    ) => {
        const gapClasses = {
            sm: "gap-3",
            md: "gap-4",
            lg: "gap-6",
        };

        const columnClasses = {
            1: "grid-cols-1",
            2: "grid-cols-1 md:grid-cols-2",
            3: "grid-cols-1 md:grid-cols-2 xl:grid-cols-3",
            4: "grid-cols-1 md:grid-cols-2 xl:grid-cols-4",
        };

        return (
            <div
                ref={ref}
                className={cn(
                    "grid",
                    columnClasses[columns],
                    gapClasses[gap],
                    className
                )}
                {...props}
            >
                {children}
            </div>
        );
    }
);

KPIGroup.displayName = 'KPIGroup';
