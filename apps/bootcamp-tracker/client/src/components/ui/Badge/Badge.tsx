import React from 'react';
import { motion } from 'framer-motion';

import type { BadgeProps, BadgeVariant, BadgeSize } from './Badge.types';

/* ========================================
   STYLES
   ======================================== */

const baseStyles = `
  inline-flex items-center justify-center gap-1.5
  font-medium
  rounded-full
  transition-apple
`;

const variantStyles: Record<BadgeVariant, string> = {
  default: `
    bg-surface-secondary
    text-text-primary
    border border-border-primary
  `,
  primary: `
    bg-accent-blue
    text-white
  `,
  success: `
    bg-status-success/10
    text-status-success
    border border-status-success/20
  `,
  warning: `
    bg-status-warning/10
    text-status-warning
    border border-status-warning/20
  `,
  error: `
    bg-status-error/10
    text-status-error
    border border-status-error/20
  `,
  info: `
    bg-status-info/10
    text-status-info
    border border-status-info/20
  `,
};

const sizeStyles: Record<BadgeSize, string> = {
  sm: 'h-5 px-2 text-xs',
  md: 'h-6 px-2.5 text-sm',
  lg: 'h-7 px-3 text-base',
};

/* ========================================
   COMPONENT
   ======================================== */

export const Badge = React.forwardRef<HTMLSpanElement, BadgeProps>(
  (
    {
      variant = 'default',
      size = 'md',
      dot = false,
      children,
      className = '',
      ...props
    },
    ref
  ) => {
    const combinedClassName = `
      ${baseStyles}
      ${variantStyles[variant]}
      ${sizeStyles[size]}
      ${className}
    `.trim().replace(/\s+/g, ' ');

    const dotSizeMap = {
      sm: 'w-1.5 h-1.5',
      md: 'w-2 h-2',
      lg: 'w-2.5 h-2.5',
    };

    const dotColorMap: Record<BadgeVariant, string> = {
      default: 'bg-text-primary',
      primary: 'bg-white',
      success: 'bg-status-success',
      warning: 'bg-status-warning',
      error: 'bg-status-error',
      info: 'bg-status-info',
    };

    return (
      <motion.span
        ref={ref}
        className={combinedClassName}
        role="status"
        initial={{ opacity: 0, scale: 0.9 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{
          duration: 0.16,
          ease: [0.16, 1, 0.3, 1],
        }}
        {...props}
      >
        {dot && (
          <span
            className={`
              ${dotSizeMap[size]}
              ${dotColorMap[variant]}
              rounded-full
            `}
          />
        )}
        {children}
      </motion.span>
    );
  }
);

Badge.displayName = 'Badge';

/* ========================================
   NOTIFICATION BADGE (Dot only)
   ======================================== */

export interface NotificationBadgeProps {
  count?: number;
  max?: number;
  showZero?: boolean;
  dot?: boolean;
  children: React.ReactNode;
  className?: string;
}

export const NotificationBadge: React.FC<NotificationBadgeProps> = ({
  count = 0,
  max = 99,
  showZero = false,
  dot = false,
  children,
  className = '',
}) => {
  const displayCount = count > max ? `${max}+` : count;
  const shouldShow = count > 0 || showZero;

  return (
    <div className={`relative inline-flex ${className}`}>
      {children}
      {shouldShow && (
        <motion.span
          className={`
            absolute -top-1 -right-1
            ${dot
              ? 'w-2 h-2'
              : 'min-w-[18px] h-[18px] px-1'
            }
            flex items-center justify-center
            bg-status-error
            text-white
            text-xs
            font-semibold
            rounded-full
            border-2 border-background-primary
          `}
          initial={{ scale: 0 }}
          animate={{ scale: 1 }}
          transition={{
            type: 'spring',
            stiffness: 500,
            damping: 25,
          }}
        >
          {!dot && displayCount}
        </motion.span>
      )}
    </div>
  );
};

/* ========================================
   USAGE EXAMPLES
   ======================================== */

/*
// Basic badges
<Badge>Default</Badge>
<Badge variant="primary">Primary</Badge>
<Badge variant="success">Success</Badge>
<Badge variant="warning">Warning</Badge>
<Badge variant="error">Error</Badge>
<Badge variant="info">Info</Badge>

// Badges with dot
<Badge variant="success" dot>Active</Badge>
<Badge variant="error" dot>Offline</Badge>

// Different sizes
<Badge size="sm">Small</Badge>
<Badge size="md">Medium</Badge>
<Badge size="lg">Large</Badge>

// Notification badge
<NotificationBadge count={5}>
  <Button>Messages</Button>
</NotificationBadge>

// Notification badge with dot
<NotificationBadge count={3} dot>
  <IconButton icon={<BellIcon />} />
</NotificationBadge>

// Notification badge with max
<NotificationBadge count={150} max={99}>
  <Button>Notifications</Button>
</NotificationBadge>
*/
