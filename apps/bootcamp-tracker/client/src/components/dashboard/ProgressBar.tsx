import React from 'react';

interface ProgressBarProps {
  value: number; // 0-100
  color?: string;
  height?: number;
  showLabel?: boolean;
  animated?: boolean;
}

export function ProgressBar({
  value,
  color = 'var(--accent)',
  height = 6,
  showLabel = false,
  animated = true,
}: ProgressBarProps) {
  return (
    <div className="flex items-center gap-2">
      <div
        className="flex-1 rounded-full overflow-hidden"
        style={{ height: `${height}px`, backgroundColor: 'var(--bg-tertiary)' }}
      >
        <div
          className={animated ? 'progress-bar-fill h-full rounded-full' : 'h-full rounded-full'}
          style={{
            width: `${Math.min(100, Math.max(0, value))}%`,
            backgroundColor: color,
          }}
        />
      </div>
      {showLabel && (
        <span className="text-xs font-mono tabular-nums" style={{ color: 'var(--text-muted)', minWidth: '32px' }}>
          {value}%
        </span>
      )}
    </div>
  );
}
