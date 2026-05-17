import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { cn } from '../../../lib/utils';

/* ========================================
   TYPES
   ======================================== */

export interface TabsProps {
    defaultValue?: string;
    value?: string;
    onValueChange?: (value: string) => void;
    children: React.ReactNode;
    className?: string;
}

export interface TabsListProps {
    children: React.ReactNode;
    className?: string;
    variant?: 'default' | 'segmented';
}

export interface TabsTriggerProps {
    value: string;
    children: React.ReactNode;
    disabled?: boolean;
    className?: string;
}

export interface TabsContentProps {
    value: string;
    children: React.ReactNode;
    className?: string;
}

/* ========================================
   CONTEXT
   ======================================== */

const TabsContext = React.createContext<{
    activeValue?: string;
    setActiveValue: (value: string) => void;
    variant?: 'default' | 'segmented';
}>({
    setActiveValue: () => { },
});

/* ========================================
   COMPONENTS
   ======================================== */

export const Tabs: React.FC<TabsProps> = ({
    defaultValue,
    value,
    onValueChange,
    children,
    className = '',
}) => {
    // Manage state to support animations
    const [internalValue, setInternalValue] = React.useState<string | undefined>(
        defaultValue
    );

    const isControlled = value !== undefined;
    const activeValue = isControlled ? value : internalValue;

    const setActiveValue = React.useCallback((newValue: string) => {
        if (!isControlled) {
            setInternalValue(newValue);
        }
        onValueChange?.(newValue);
    }, [isControlled, onValueChange]);

    return (
        <TabsContext.Provider value={{ activeValue, setActiveValue }}>
            <div className={className}>
                {children}
            </div>
        </TabsContext.Provider>
    );
};

export const TabsList: React.FC<TabsListProps> = ({
    children,
    className = '',
    variant = 'default',
}) => {
    // We need to merge with existing context (activeValue) but passing variant down
    const context = React.useContext(TabsContext);

    const variantStyles = {
        default: 'inline-flex items-center gap-1 border-b border-border-primary w-full justify-start',
        segmented: 'inline-flex items-center gap-1 p-1 bg-surface-secondary rounded-xl',
    };

    return (
        <TabsContext.Provider value={{ ...context, variant }}>
            <div
                role="tablist"
                aria-orientation="horizontal"
                className={cn(variantStyles[variant], className)}
            >
                {children}
            </div>
        </TabsContext.Provider>
    );
};

export const TabsTrigger: React.FC<TabsTriggerProps> = ({
    value,
    children,
    disabled = false,
    className = '',
}) => {
    const { activeValue, setActiveValue, variant } = React.useContext(TabsContext);
    const isSelected = activeValue === value;

    return (
        <button
            type="button"
            role="tab"
            aria-selected={isSelected}
            disabled={disabled}
            onClick={() => !disabled && setActiveValue(value)}
            className={cn(
                "relative px-4 py-2 text-sm font-medium transition-colors outline-none disabled:opacity-40 disabled:cursor-not-allowed z-10 focus-visible:ring-2 focus-visible:ring-accent-blue focus-visible:rounded-md",
                variant === 'default' && "hover:text-text-primary",
                variant === 'segmented' && "flex-1 text-center",
                className
            )}
        >
            <span
                className={cn(
                    "relative z-20 transition-colors duration-200",
                    isSelected ? "text-text-primary" : "text-text-secondary hover:text-text-primary"
                )}
            >
                {children}
            </span>

            {/* Indicador animado para variant default */}
            {isSelected && variant === 'default' && (
                <motion.div
                    layoutId="tab-indicator"
                    className="absolute bottom-0 left-0 right-0 h-0.5 bg-accent-blue rounded-full z-10"
                    initial={false}
                    transition={{
                        type: 'spring',
                        stiffness: 500,
                        damping: 30,
                        mass: 0.5,
                    }}
                />
            )}

            {/* Background animado para variant segmented */}
            {isSelected && variant === 'segmented' && (
                <motion.div
                    layoutId="tab-background"
                    className="absolute inset-0 bg-surface-primary rounded-lg shadow-sm z-0"
                    initial={false}
                    transition={{
                        type: 'spring',
                        stiffness: 500,
                        damping: 30,
                        mass: 0.5,
                    }}
                />
            )}
        </button>
    );
};

export const TabsContent: React.FC<TabsContentProps> = ({
    value,
    children,
    className = '',
}) => {
    const { activeValue } = React.useContext(TabsContext);
    const isSelected = activeValue === value;

    if (!isSelected) return null;

    return (
        <div
            role="tabpanel"
            className={cn(
                "outline-none focus-visible:ring-2 focus-visible:ring-accent-blue focus-visible:ring-offset-2 rounded-xl mt-4",
                className
            )}
        >
            <motion.div
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: 8 }}
                transition={{
                    duration: 0.2,
                    ease: "easeOut",
                }}
            >
                {children}
            </motion.div>
        </div>
    );
};

/* ========================================
   USAGE EXAMPLES
   ======================================== */

/*
// Default tabs (underline style)
<Tabs defaultValue="tab1">
  <TabsList>
    <TabsTrigger value="tab1">Tab 1</TabsTrigger>
    <TabsTrigger value="tab2">Tab 2</TabsTrigger>
    <TabsTrigger value="tab3">Tab 3</TabsTrigger>
  </TabsList>
  
  <TabsContent value="tab1">
    <p>Content for tab 1</p>
  </TabsContent>
  ...
</Tabs>
*/
