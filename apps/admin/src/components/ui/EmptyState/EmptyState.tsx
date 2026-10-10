import React from 'react';
import { EmptyStateProps } from './EmptyState.types';

export const EmptyState: React.FC<EmptyStateProps> = ({
  title,
  description,
  icon,
  action,
  className = '',
}) => {
  return (
    // CSS fade (not a framer spring) so the global reduced-motion guard covers it.
    <div
      className={`
        animate-fade-in
        flex flex-col items-center justify-center text-center px-6 py-8

        ${className}
      `.trim().replace(/\s+/g, ' ')}
    >
      {icon && (
        // Icon in a tinted rounded square — matches the NexusMind UI Kit's
        // empty-state icon treatment (44x44, 13px radius, accent-tinted).
        <div className="mb-4 w-11 h-11 rounded-xl bg-foreground/[0.04] flex items-center justify-center shrink-0">
          {React.isValidElement(icon) ? (
            React.cloneElement(icon as React.ReactElement, {
              size: 20,
              strokeWidth: 1.7,
              className: 'w-5 h-5 text-text-tertiary',
            } as Record<string, unknown>)
          ) : (
            icon
          )}
        </div>
      )}

      <h3 className="text-[15px] font-semibold tracking-[-0.2px] text-text-primary mb-1.5">
        {title}
      </h3>

      {description && (
        <p className="text-sm text-text-secondary max-w-sm leading-relaxed">
          {description}
        </p>
      )}

      {action && (
        <div className="mt-2">
          {action}
        </div>
      )}
    </div>
  );
};

EmptyState.displayName = 'EmptyState';

