import React, { forwardRef } from 'react';
import { motion } from 'framer-motion';
import { cn } from '../../../lib/utils';
import { TrendingUp, TrendingDown, Minus } from 'lucide-react';
import { Sparkline as SparklineComponent } from '../Sparkline';

/* ========================================
   TYPES
   ======================================== */

export type StatisticTrend = 'up' | 'down' | 'neutral';
export type StatisticVariant = 'card' | 'soft' | 'bordered' | 'minimal' | 'glass';
export type StatisticSize = 'sm' | 'md' | 'lg';
export type StatisticAccent = 'blue' | 'green' | 'purple' | 'orange' | 'pink';

export interface StatisticGoal {
    label?: React.ReactNode;
    value?: React.ReactNode;
    progress?: number; // 0 to 1 range
}

export interface StatisticMetric {
    id?: string;
    label: React.ReactNode;
    value: React.ReactNode;
    description?: React.ReactNode;
    change?: string;
    trend?: StatisticTrend;
    icon?: React.ReactNode;
    badge?: React.ReactNode;
    target?: React.ReactNode;
    sparkline?: number[];
    sparklineAccent?: StatisticAccent;
    goal?: StatisticGoal;
    subtle?: boolean;
}

export interface StatisticDisplayProps extends React.HTMLAttributes<HTMLDivElement> {
    metrics: StatisticMetric[];
    variant?: StatisticVariant;
    size?: StatisticSize;
    columns?: 1 | 2 | 3 | 4;
    animate?: boolean;
    gap?: 'sm' | 'md' | 'lg';
}

export interface StatisticHighlightProps extends React.HTMLAttributes<HTMLDivElement> {
    label: React.ReactNode;
    value: React.ReactNode;
    change?: string;
    trend?: StatisticTrend;
    description?: React.ReactNode;
    badge?: React.ReactNode;
    sparkline?: number[];
    sparklineAccent?: StatisticAccent;
    size?: StatisticSize;
    progress?: number;
}

/* ========================================
   CONSTANTS
   ======================================== */

const MotionDiv = motion.div as any;

const sizeClasses: Record<StatisticSize, { label: string; value: string; meta: string; padding: string; gap: string; icon: string; sparkline: { width: number; height: number }; }> = {
    sm: {
        label: "text-[11px]",
        value: "text-2xl",
        meta: "text-xs",
        padding: "p-4",
        gap: "space-y-3",
        icon: "w-8 h-8",
        sparkline: { width: 100, height: 32 },
    },
    md: {
        label: "text-xs",
        value: "text-4xl",
        meta: "text-sm",
        padding: "p-5",
        gap: "space-y-4",
        icon: "w-10 h-10",
        sparkline: { width: 120, height: 38 },
    },
    lg: {
        label: "text-sm",
        value: "text-5xl",
        meta: "text-base",
        padding: "p-6",
        gap: "space-y-5",
        icon: "w-12 h-12",
        sparkline: { width: 140, height: 44 },
    },
};

const variantClasses: Record<StatisticVariant, string> = {
    card: "bg-surface-primary border border-border-primary rounded-2xl shadow-sm",
    soft: "bg-surface-secondary/70 border border-border-secondary/60 rounded-2xl",
    bordered: "bg-surface-primary border border-border-tertiary rounded-2xl",
    minimal: "rounded-xl",
    glass: "bg-white/5 border border-white/10 backdrop-blur-xl rounded-2xl",
};

const accentTokens: Record<StatisticAccent, { color: string; fillColor: string; text: string; chip: string; }> = {
    blue: {
        color: '#2563eb',
        fillColor: 'rgba(37,99,235,0.12)',
        text: 'text-accent-blue',
        chip: 'bg-accent-blue/10 text-accent-blue',
    },
    green: {
        color: '#16a34a',
        fillColor: 'rgba(22,163,74,0.12)',
        text: 'text-status-success',
        chip: 'bg-status-success/10 text-status-success',
    },
    purple: {
        color: '#7c3aed',
        fillColor: 'rgba(124,58,237,0.12)',
        text: 'text-accent-purple',
        chip: 'bg-accent-purple/10 text-accent-purple',
    },
    orange: {
        color: '#ea580c',
        fillColor: 'rgba(234,88,12,0.12)',
        text: 'text-status-warning',
        chip: 'bg-status-warning/15 text-status-warning',
    },
    pink: {
        color: '#db2777',
        fillColor: 'rgba(219,39,119,0.12)',
        text: 'text-accent-pink',
        chip: 'bg-accent-pink/15 text-accent-pink',
    },
};

const trendTokens: Record<StatisticTrend, { icon: React.ComponentType<{ className?: string }>; text: string; bg: string; trend: 'up' | 'down' | 'neutral'; }> = {
    up: {
        icon: TrendingUp,
        text: 'text-status-success',
        bg: 'bg-status-success/10',
        trend: 'up',
    },
    down: {
        icon: TrendingDown,
        text: 'text-status-error',
        bg: 'bg-status-error/10',
        trend: 'down',
    },
    neutral: {
        icon: Minus,
        text: 'text-text-tertiary',
        bg: 'bg-surface-secondary/80',
        trend: 'neutral',
    },
};

const GoalMeter: React.FC<{ goal?: StatisticGoal }> = ({ goal }) => {
    if (!goal) return null;

    const progress = Math.min(Math.max(goal.progress ?? 0, 0), 1);

    return (
        <div className="mt-3">
            {(goal.label || goal.value) && (
                <div className="flex items-center justify-between text-[11px] text-text-tertiary mb-1">
                    <span>{goal.label ?? 'Target'}</span>
                    <span>{goal.value}</span>
                </div>
            )}
            <div className="h-1.5 w-full rounded-full bg-surface-secondary overflow-hidden">
                <div
                    className="h-full rounded-full bg-accent-blue transition-all"
                    style={{ width: `${progress * 100}%` }}
                />
            </div>
        </div>
    );
};

/* ========================================
   STATISTIC DISPLAY COMPONENT
   ======================================== */

const columnClasses: Record<1 | 2 | 3 | 4, string> = {
    1: 'grid-cols-1',
    2: 'grid-cols-1 md:grid-cols-2',
    3: 'grid-cols-1 md:grid-cols-2 xl:grid-cols-3',
    4: 'grid-cols-1 md:grid-cols-2 xl:grid-cols-4',
};

const gapClasses: Record<'sm' | 'md' | 'lg', string> = {
    sm: 'gap-3',
    md: 'gap-4',
    lg: 'gap-6',
};

export const StatisticDisplay = forwardRef<HTMLDivElement, StatisticDisplayProps>(
    (
        {
            metrics,
            variant = 'card',
            size = 'md',
            columns = 3,
            animate = true,
            gap = 'md',
            className,
            ...props
        },
        ref
    ) => {
        return (
            <div
                ref={ref}
                className={cn('grid', columnClasses[columns], gapClasses[gap], className)}
                {...props}
            >
                {metrics.map((metric, index) => {
                    const trend = metric.trend ?? 'neutral';
                    const TrendIcon = trendTokens[trend].icon;
                    const accent = metric.sparklineAccent ?? 'blue';
                    const currentSize = sizeClasses[size];

                    return (
                        <MotionDiv
                            key={metric.id ?? index}
                            className={cn(
                                'relative overflow-hidden group transition-all duration-300',
                                variantClasses[variant],
                                currentSize.padding,
                                metric.subtle && 'bg-surface-secondary/60',
                                'flex flex-col'
                            )}
                            whileHover={{ y: -4, scale: 1.01 }}
                            initial={animate ? { opacity: 0, y: 15 } : false}
                            animate={animate ? { opacity: 1, y: 0 } : undefined}
                            transition={{
                                delay: animate ? index * 0.08 : 0,
                                duration: 0.4,
                                ease: [0.23, 1, 0.32, 1],
                            }}
                        >
                            {/* Top row: Label & Icon */}
                            <div className="flex items-center justify-between mb-4">
                                <span className={cn(
                                    'uppercase tracking-widest text-text-tertiary font-bold',
                                    currentSize.label
                                )}>
                                    {metric.label}
                                </span>
                                {metric.icon && (
                                    <div className={cn(
                                        'flex items-center justify-center rounded-xl bg-white/5 border border-white/5 text-text-secondary group-hover:text-text-primary transition-colors',
                                        currentSize.icon
                                    )}>
                                        {metric.icon}
                                    </div>
                                )}
                            </div>

                            {/* Main Value Area */}
                            <div className="flex flex-col gap-1 z-10">
                                <h3 className={cn(
                                    'font-bold text-text-primary tracking-tight tabular-nums',
                                    currentSize.value
                                )}>
                                    {metric.value}
                                </h3>
                                
                                {(metric.change || metric.trend) && (
                                    <div className="flex items-center gap-2 mt-0.5">
                                        <div className={cn(
                                            'inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-[10px] font-extrabold uppercase tracking-widest transition-colors duration-300',
                                            trendTokens[trend].bg,
                                            trendTokens[trend].text
                                        )}>
                                            <TrendIcon className="w-3 h-3 stroke-[3]" />
                                            {metric.change || (trend === 'up' ? 'Increase' : trend === 'down' ? 'Decrease' : 'Stable')}
                                        </div>
                                    </div>
                                )}
                            </div>

                            {/* Description if any */}
                            {metric.description && (
                                <p className={cn(
                                    'text-text-tertiary mt-3 leading-relaxed max-w-[85%] font-medium opacity-70 group-hover:opacity-100 transition-opacity',
                                    currentSize.meta
                                )}>
                                    {metric.description}
                                </p>
                            )}

                            {/* Sparkline at the bottom integrated as area chart */}
                            {metric.sparkline && (
                                <div className="absolute bottom-0 left-0 right-0 h-16 pointer-events-none opacity-40 group-hover:opacity-70 transition-all duration-500 scale-105 group-hover:scale-100 origin-bottom">
                                    <SparklineComponent
                                        data={metric.sparkline}
                                        width={400}
                                        height={64}
                                        color={accentTokens[accent].color}
                                        fillColor={accentTokens[accent].fillColor}
                                        showArea={true}
                                        showLastDot={false}
                                        className="w-full h-full"
                                    />
                                </div>
                            )}

                            {/* Goal integration */}
                            <div className="mt-auto pt-4">
                                <GoalMeter goal={metric.goal} />
                            </div>

                            {/* Decorative Glow */}
                            <div 
                                className="absolute top-0 right-0 -mr-16 -mt-16 w-48 h-48 rounded-full blur-[80px] opacity-0 group-hover:opacity-[0.07] transition-opacity duration-700 pointer-events-none"
                                style={{ backgroundColor: accentTokens[accent].color }}
                            />
                        </MotionDiv>
                    );
                })}
            </div>
        );
    }
);

StatisticDisplay.displayName = 'StatisticDisplay';

/* ========================================
   STATISTIC HIGHLIGHT COMPONENT
   ======================================== */

export const StatisticHighlight = forwardRef<HTMLDivElement, StatisticHighlightProps>(
    (
        {
            label,
            value,
            change,
            trend,
            description,
            badge,
            sparkline,
            sparklineAccent = 'blue',
            size = 'md',
            progress,
            className,
            ...props
        },
        ref
    ) => {
        const resolvedTrend = trend ?? 'neutral';
        const TrendIcon = trendTokens[resolvedTrend].icon;

        return (
            <MotionDiv
                ref={ref}
                className={cn(
                    'w-full rounded-3xl bg-surface-primary border border-border-primary shadow-[0_30px_80px_-50px_rgba(15,23,42,0.65)] p-6 md:p-8 relative overflow-hidden',
                    className
                )}
                initial={{ opacity: 0, scale: 0.98 }}
                animate={{ opacity: 1, scale: 1 }}
                transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
                {...props}
            >
                <div className="absolute inset-0 bg-gradient-to-br from-accent-blue/5 via-transparent to-transparent pointer-events-none" />

                <div className="relative flex flex-col gap-4">
                    <div className="flex items-center justify-between gap-3">
                        <div>
                            <p className={cn('uppercase tracking-[0.18em] text-text-tertiary font-semibold', sizeClasses[size].label)}>
                                {label}
                            </p>
                            {description && (
                                <p className={cn('text-text-secondary mt-1', sizeClasses[size].meta)}>
                                    {description}
                                </p>
                            )}
                        </div>
                        {badge && (
                            <div className="text-xs font-semibold text-text-secondary">
                                {badge}
                            </div>
                        )}
                    </div>

                    <div className="flex flex-wrap items-end gap-3">
                        <span className={cn('font-semibold text-text-primary tracking-tight leading-none', sizeClasses[size].value)}>
                            {value}
                        </span>
                        {(change || trend) && TrendIcon && (
                            <span className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-surface-secondary/80 text-[11px] font-semibold">
                                <TrendIcon className={cn('w-4 h-4', trendTokens[resolvedTrend].text)} />
                                {change && (
                                    <span className={trendTokens[resolvedTrend].text}>{change}</span>
                                )}
                            </span>
                        )}
                    </div>

                    {sparkline && (
                        <div className="mt-2">
                            <SparklineComponent
                                data={sparkline}
                                width={160}
                                height={64}
                                color={accentTokens[sparklineAccent].color}
                                fillColor={accentTokens[sparklineAccent].fillColor}
                                showArea={true}
                            />
                        </div>
                    )}

                    {typeof progress === 'number' && (
                        <div className="mt-2">
                            <div className="flex items-center justify-between text-[11px] text-text-tertiary mb-1">
                                <span>Progress</span>
                                <span>{Math.round(progress * 100)}%</span>
                            </div>
                            <div className="h-1.5 bg-surface-secondary rounded-full overflow-hidden">
                                <div
                                    className="h-full bg-accent-blue rounded-full"
                                    style={{ width: `${Math.min(Math.max(progress, 0), 1) * 100}%` }}
                                />
                            </div>
                        </div>
                    )}
                </div>
            </MotionDiv>
        );
    }
);

StatisticHighlight.displayName = 'StatisticHighlight';
