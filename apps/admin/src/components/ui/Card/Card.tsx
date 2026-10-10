import React from 'react';
import { motion } from 'framer-motion';
import type { CardProps } from './Card.types';

import { cardBaseStyles, cardVariantStyles, cardPaddingStyles } from './Card.styles';
import { cn } from '../../../lib/utils';

export const Card = React.forwardRef<HTMLDivElement, CardProps>(
    (
        {
            variant = 'elevated',
            hoverable = false,
            padding = 'md',
            children,
            className = '',
            ...props
        },
        ref
    ) => {
        const combinedClassName = cn(
            cardBaseStyles,
            cardVariantStyles[variant],
            cardPaddingStyles[padding],
            className,
        );

        const hoverAnimation = hoverable
            ? {
                transition: {
                    type: 'spring' as const,
                    stiffness: 300,
                    damping: 30,
                    mass: 0.8,
                },
            }
            : {};

        // Assign role="region" and aria-label for interactive cards so screen readers
        // can navigate to them as landmarks.
        // Only use role="region" (a landmark) when an accessible name is available.
        // Without an accessible name, no role is better than role="group" —
        // unnamed group roles add noise without benefit for screen reader users.
        const accessibleName = props['aria-label'] || (typeof children === 'string' ? children : undefined);
        const interactiveAttrs = hoverable && accessibleName
            ? { role: 'region' as const, 'aria-label': accessibleName }
            : {};

        return (
            <motion.div
                data-slot="card"
                ref={ref}
                className={combinedClassName}
                role={interactiveAttrs.role}
                aria-label={interactiveAttrs['aria-label']}

                transition={{
                    duration: 0.22,
                    ease: [0.16, 1, 0.3, 1] as [number, number, number, number],
                }}
                {...hoverAnimation}
                {...props}
            >
                {children}
            </motion.div>
        );
    }
);

Card.displayName = 'Card';

/* ========================================
   SUB-COMPONENTS
   ======================================== */

export const CardHeader: React.FC<{
    children: React.ReactNode;
    className?: string;
}> = ({ children, className = '' }) => (
    <div className={`mb-4 ${className}`}>{children}</div>
);

export const CardTitle: React.FC<{
    children: React.ReactNode;
    className?: string;
}> = ({ children, className = '' }) => (
    <h3 className={`text-base font-semibold leading-none text-card-foreground mb-1 ${className}`}>
        {children}
    </h3>
);

export const CardDescription: React.FC<{
    children: React.ReactNode;
    className?: string;
}> = ({ children, className = '' }) => (
    <p className={`text-sm text-text-secondary ${className}`}>{children}</p>
);

export const CardContent: React.FC<{
    children: React.ReactNode;
    className?: string;
}> = ({ children, className = '' }) => (
    <div className={className}>{children}</div>
);

export const CardFooter: React.FC<{
    children: React.ReactNode;
    className?: string;
}> = ({ children, className = '' }) => (
    <div className={`mt-6 flex items-center gap-3 ${className}`}>{children}</div>
);

/* ========================================
   USAGE EXAMPLES
   ======================================== */

/*
// Basic elevated card
<Card>
  <CardHeader>
    <CardTitle>Card Title</CardTitle>
    <CardDescription>Card description goes here</CardDescription>
  </CardHeader>
  <CardContent>
    <p>Card content...</p>
  </CardContent>
  <CardFooter>
    <Button>Action</Button>
  </CardFooter>
</Card>

// Glass card with hover
<Card variant="glass" hoverable>
  <p>Hoverable glass card</p>
</Card>

// Outlined card with custom padding
<Card variant="outlined" padding="lg">
  <p>Large padding card</p>
</Card>
*/
