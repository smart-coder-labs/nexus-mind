'use client';

import React from 'react';
import { motion } from 'framer-motion';

import type { BadgeProps } from './Badge.types';
import {
  badgeBaseStyles as baseStyles,
  badgeVariantStyles as variantStyles,
  badgeSizeStyles as sizeStyles,
  badgeDotSizeStyles as dotSizeMap,
  badgeDotColorStyles as dotColorMap,
} from './Badge.styles';

/* Styles now live in Badge.styles.ts (previously duplicated inline here and
   never imported — see Button.tsx for the same fix and rationale). */

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
