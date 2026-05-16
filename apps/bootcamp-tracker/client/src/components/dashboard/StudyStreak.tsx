import React from 'react';
import { format, parseISO, subDays } from 'date-fns';
import type { Stats } from '../../types';

interface StudyStreakProps {
  hoursPerDay: Stats['hoursPerDay'];
}

export function StudyStreak({ hoursPerDay }: StudyStreakProps) {
  if (hoursPerDay.length === 0) return null;

  const last14Days = Array.from({ length: 14 }, (_, i) => {
    const date = format(subDays(new Date(), 13 - i), 'yyyy-MM-dd');
    const found = hoursPerDay.find(d => d.date === date);
    return { date, hours: found?.hours ?? 0 };
  });

  const maxHours = Math.max(...last14Days.map(d => d.hours), 1);

  return (
    <div className="surface rounded-xl p-4">
      <div className="text-sm font-medium mb-3" style={{ color: 'var(--text-secondary)' }}>
        Study Activity — Last 14 Days
      </div>
      <div className="flex items-end gap-1 h-16">
        {last14Days.map(({ date, hours }) => (
          <div key={date} className="flex-1 flex flex-col items-center gap-1" title={`${date}: ${hours}h`}>
            <div
              className="w-full rounded-sm transition-all"
              style={{
                height: `${Math.max(2, (hours / maxHours) * 52)}px`,
                backgroundColor: hours > 0 ? 'var(--accent)' : 'var(--bg-tertiary)',
                opacity: hours > 0 ? Math.max(0.4, hours / maxHours) : 1,
              }}
            />
          </div>
        ))}
      </div>
      <div className="flex justify-between mt-1">
        <span className="text-xs" style={{ color: 'var(--text-muted)' }}>14d ago</span>
        <span className="text-xs" style={{ color: 'var(--text-muted)' }}>Today</span>
      </div>
    </div>
  );
}
