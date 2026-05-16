import React from 'react';
import type { Stats } from '../../types';

interface StatsPanelProps {
  stats: Stats;
}

function StatCard({ label, value, sub, color }: { label: string; value: string | number; sub?: string; color?: string }) {
  return (
    <div className="surface rounded-xl p-4">
      <div className="text-xs mb-1" style={{ color: 'var(--text-muted)' }}>{label}</div>
      <div className="text-2xl font-mono font-semibold tabular-nums" style={{ color: color || 'var(--text-primary)' }}>
        {value}
      </div>
      {sub && <div className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>{sub}</div>}
    </div>
  );
}

export function StatsPanel({ stats }: StatsPanelProps) {
  const remaining = stats.totalEstimatedHours - stats.completedHours;

  return (
    <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
      <StatCard
        label="Overall Progress"
        value={`${stats.totalSubtopics > 0 ? Math.round((stats.completedSubtopics / stats.totalSubtopics) * 100) : 0}%`}
        sub={`${stats.completedSubtopics}/${stats.totalSubtopics} subtopics`}
        color="var(--accent)"
      />
      <StatCard
        label="Hours Left"
        value={`${remaining.toFixed(1)}h`}
        sub={`${stats.completedHours.toFixed(1)}h done of ${stats.totalEstimatedHours}h`}
      />
      <StatCard
        label="P0 Pending"
        value={stats.pendingP0}
        sub="critical subtopics"
        color={stats.pendingP0 > 0 ? 'var(--danger)' : 'var(--success)'}
      />
      <StatCard
        label="Study Streak"
        value={`${stats.studyStreak}d`}
        sub={stats.studyStreak > 0 ? 'days in a row 🔥' : 'no sessions yet'}
        color={stats.studyStreak > 2 ? 'var(--warning)' : undefined}
      />
    </div>
  );
}
