import React from 'react';

interface BadgeProps {
  children: React.ReactNode;
  variant?: 'p0' | 'p1' | 'p2' | 'default' | 'success' | 'accent';
  className?: string;
}

const variantClasses: Record<string, string> = {
  p0: 'bg-red-900/30 text-red-400 border border-red-800/50',
  p1: 'bg-yellow-900/30 text-yellow-400 border border-yellow-800/50',
  p2: 'bg-gray-800/50 text-gray-400 border border-gray-700/50',
  default: 'bg-gray-800/50 text-gray-300 border border-gray-700',
  success: 'bg-green-900/30 text-green-400 border border-green-800/50',
  accent: 'bg-blue-900/30 text-blue-400 border border-blue-800/50',
};

export function Badge({ children, variant = 'default', className = '' }: BadgeProps) {
  return (
    <span
      className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-mono font-medium ${variantClasses[variant]} ${className}`}
    >
      {children}
    </span>
  );
}

export function PriorityBadge({ priority }: { priority: 'P0' | 'P1' | 'P2' }) {
  const variant = priority.toLowerCase() as 'p0' | 'p1' | 'p2';
  return <Badge variant={variant}>{priority}</Badge>;
}
