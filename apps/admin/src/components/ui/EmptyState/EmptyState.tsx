import React from 'react';
import { motion } from 'framer-motion';
import { EmptyStateProps } from './EmptyState.types';

export const EmptyState: React.FC<EmptyStateProps> = ({
  title,
  description,
  icon,
  action,
  className = '',
}) => {
  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      transition={{
        duration: 0.4,
        ease: [0.16, 1, 0.3, 1],
      }}
      className={`
        flex flex-col items-center justify-center text-center p-8
        border border-white/[0.07] bg-[#0d0f14]/60 backdrop-blur-[12px] rounded-[18px]
        ${className}
      `.trim().replace(/\s+/g, ' ')}
    >
      {icon && (
        // Icon in a tinted rounded square — matches the NexusMind UI Kit's
        // empty-state icon treatment (44x44, 13px radius, accent-tinted).
        <div className="mb-6 w-11 h-11 rounded-[13px] bg-accent-blue/10 flex items-center justify-center shrink-0">
          {React.isValidElement(icon) ? (
            React.cloneElement(icon as React.ReactElement, {
              size: 20,
              strokeWidth: 1.7,
              className: 'w-5 h-5 text-accent-blue',
            } as Record<string, unknown>)
          ) : (
            icon
          )}
        </div>
      )}

      <h3 className="text-xs font-semibold text-text-secondary mb-2">
        {title}
      </h3>

      {description && (
        <p className="text-xs text-text-quaternary max-w-sm mb-8 leading-relaxed">
          {description}
        </p>
      )}

      {action && (
        <div className="mt-2">
          {action}
        </div>
      )}
    </motion.div>
  );
};

EmptyState.displayName = 'EmptyState';

