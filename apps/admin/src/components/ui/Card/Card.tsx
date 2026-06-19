import React from 'react';
import { motion } from 'framer-motion';
import type { CardProps, CardVariant } from './Card.types';

/* ========================================
   STYLES
   ======================================== */

const baseStyles = `
  rounded-[18px]
  transition-apple
`;

const variantStyles: Record<CardVariant, string> = {
    elevated: `
    bg-surface-primary
    border border-white/[0.08]
  `,
    glass: `
    glass
    border border-white/[0.08]
  `,
    outlined: `
    bg-surface-primary
    border border-white/[0.08]
  `,
    flat: `
    bg-surface-secondary
  `,
};

const paddingStyles = {
    none: '',
    sm: 'p-4',
    md: 'p-6',
    lg: 'p-8',
};

/* ========================================
   COMPONENT
   ======================================== */

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
        const combinedClassName = `
      ${baseStyles}
      ${variantStyles[variant]}
      ${paddingStyles[padding]}
      ${className}
    `.trim().replace(/\s+/g, ' ');

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
                ref={ref}
                className={combinedClassName}
                role={interactiveAttrs.role}
                aria-label={interactiveAttrs['aria-label']}
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
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
    <h3 className={`text-[21px] font-semibold text-text-primary tracking-[0.231px] mb-1 ${className}`}>
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
