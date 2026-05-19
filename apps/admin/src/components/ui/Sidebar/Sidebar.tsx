import React from 'react';
import { motion, HTMLMotionProps } from 'framer-motion';
import { cn } from '../../../lib/utils';

/**
 * Sidebar component – vertical navigation list.
 *
 * Designed to follow the Apple‑style design system tokens.
 * Uses `bg-surface-secondary` as background, rounded corners, and subtle hover/active states.
 */
export interface SidebarItem {
    label: string;
    href?: string;
    icon?: React.ReactNode;
    active?: boolean;
    onClick?: () => void;
}

export interface SidebarProps extends Omit<HTMLMotionProps<'nav'>, 'children'> {
    /** List of navigation items */
    items: SidebarItem[];
    /** Optional className for custom styling */
    className?: string;
}

export const Sidebar = React.forwardRef<HTMLDivElement, SidebarProps>(
    ({ items, className = '', ...props }, ref) => {
        const baseStyles = `
      flex flex-col space-y-1 p-4 bg-surface-secondary rounded-lg shadow-sm`
            ;
        const itemBase = `
      flex items-center gap-2 px-3 py-2 rounded-md text-sm font-medium
      transition-apple focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue
    `;
        const itemActive = `text-text-primary bg-surface-primary`;
        const itemInactive = `text-text-secondary hover:bg-surface-primary/20`;

        return (
            <motion.nav
                ref={ref}
                className={cn(baseStyles, className)}
                initial={{ opacity: 0, x: -20 }}
                animate={{ opacity: 1, x: 0 }}
                transition={{ type: 'spring', stiffness: 300, damping: 30 }}
                {...props}
            >
                {items.map((item, idx) => {
                    const Component = item.href ? 'a' : 'button';
                    const isActive = !!item.active;
                    return (
                        <Component
                            key={idx}
                            href={item.href}
                            onClick={item.onClick}
                            className={cn(
                                itemBase,
                                isActive ? itemActive : itemInactive
                            )}
                        >
                            {item.icon && <span className="inline-flex">{item.icon}</span>}
                            {item.label}
                        </Component>
                    );
                })}
            </motion.nav>
        );
    }
);

Sidebar.displayName = 'Sidebar';
