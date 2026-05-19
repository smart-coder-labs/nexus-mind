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
        bg-surface-primary/50 rounded-2xl
        ${className}
      `.trim().replace(/\s+/g, ' ')}
    >
      {icon && (
        <div className="mb-6 text-text-tertiary">
          {React.isValidElement(icon) ? (
            React.cloneElement(icon as React.ReactElement, {
              size: 48,
              strokeWidth: 1.5,
              className: 'w-12 h-12',
            })
          ) : (
            icon
          )}
        </div>
      )}

      <h3 className="text-xl font-semibold text-text-primary mb-2">
        {title}
      </h3>

      {description && (
        <p className="text-text-secondary max-w-sm mb-8 leading-relaxed">
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

