import React from 'react';
import { DayCard } from './DayCard';
import type { RoadmapDay } from '../../types';

interface WeekTimelineProps {
  week: number;
  days: RoadmapDay[];
  todayKey?: string; // "week:day" format
}

const weekTitles: Record<number, string> = {
  1: 'Fundamentos — Rust + Databases',
  2: 'Core — Auth + Crypto + MCP',
  3: 'Avanzado — Vectors + Distributed + Sync',
  4: 'Empresa — Security + DevOps + Enterprise',
};

export function WeekTimeline({ week, days, todayKey }: WeekTimelineProps) {
  const completed = days.filter(d => d.completed).length;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-xs font-mono" style={{ color: 'var(--text-muted)' }}>Semana {week}</div>
          <div className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>
            {weekTitles[week] || `Week ${week}`}
          </div>
        </div>
        <div className="text-sm font-mono" style={{ color: 'var(--text-muted)' }}>
          {completed}/{days.length} days
        </div>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
        {days.map(day => (
          <DayCard
            key={day.id}
            day={day}
            isToday={todayKey === `${day.week}:${day.day}`}
          />
        ))}
      </div>
    </div>
  );
}
