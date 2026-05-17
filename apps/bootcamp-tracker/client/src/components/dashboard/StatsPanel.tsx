import React from 'react';
import { KPIBlock, KPIGroup } from '../ui/KPIBlock';
import type { Stats } from '../../types';

interface StatsPanelProps {
  stats: Stats;
}

export function StatsPanel({ stats }: StatsPanelProps) {
  const remaining = stats.totalEstimatedHours - stats.completedHours;
  const percentage = stats.totalSubtopics > 0
    ? Math.round((stats.completedSubtopics / stats.totalSubtopics) * 100)
    : 0;

  return (
    <KPIGroup columns={4} gap="sm">
      <KPIBlock
        label="Overall Progress"
        value={`${percentage}%`}
        description={`${stats.completedSubtopics}/${stats.totalSubtopics} subtopics`}
        variant="bordered"
        size="sm"
      />
      <KPIBlock
        label="Hours Left"
        value={`${remaining.toFixed(1)}h`}
        description={`${stats.completedHours.toFixed(1)}h done of ${stats.totalEstimatedHours}h`}
        variant="bordered"
        size="sm"
      />
      <KPIBlock
        label="P0 Pending"
        value={stats.pendingP0}
        description="critical subtopics"
        trend={stats.pendingP0 > 0 ? 'down' : 'up'}
        variant="bordered"
        size="sm"
      />
      <KPIBlock
        label="Study Streak"
        value={`${stats.studyStreak}d`}
        description={stats.studyStreak > 0 ? 'days in a row' : 'no sessions yet'}
        trend={stats.studyStreak > 2 ? 'up' : 'neutral'}
        variant="bordered"
        size="sm"
      />
    </KPIGroup>
  );
}
