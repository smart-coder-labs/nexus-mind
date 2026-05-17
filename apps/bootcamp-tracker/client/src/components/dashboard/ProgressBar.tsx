import React from 'react';
import { Progress } from '../ui/Progress';

interface ProgressBarProps {
  value: number;
  max?: number;
  color?: string;
  height?: number;
  animated?: boolean;
  className?: string;
}

export function ProgressBar({ value, max = 100, height = 8, className }: ProgressBarProps) {
  return (
    <Progress
      value={value}
      max={max}
      className={className}
      style={{ height: `${height}px` } as React.CSSProperties}
    />
  );
}
