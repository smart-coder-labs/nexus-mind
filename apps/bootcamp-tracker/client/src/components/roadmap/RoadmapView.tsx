import React, { useState } from 'react';
import { WeekTimeline } from './WeekTimeline';
import { LoadingPage } from '../ui/LoadingSpinner';
import { useRoadmap } from '../../hooks/useRoadmap';

export function RoadmapView() {
  const { data, isLoading } = useRoadmap();
  const [activeWeek, setActiveWeek] = useState(1);

  if (isLoading) return <LoadingPage />;
  if (!data) return null;

  const weeks = Object.keys(data.weeks).map(Number).sort();

  return (
    <div className="space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-xl font-semibold" style={{ color: 'var(--text-primary)' }}>
          🗺️ Study Roadmap
        </h1>
        <p className="text-sm mt-1" style={{ color: 'var(--text-secondary)' }}>
          4 weeks · 30h/week · 6 days/week · 5h/day
        </p>
      </div>

      {/* Week tabs */}
      <div className="flex gap-1 p-1 rounded-xl" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}>
        {weeks.map(w => {
          const days = data.weeks[w] || [];
          const completed = days.filter(d => d.completed).length;
          return (
            <button
              key={w}
              onClick={() => setActiveWeek(w)}
              className={`flex-1 flex flex-col items-center py-2 px-3 rounded-lg text-sm transition-colors`}
              style={{
                backgroundColor: activeWeek === w ? 'var(--bg-tertiary)' : 'transparent',
                color: activeWeek === w ? 'var(--text-primary)' : 'var(--text-muted)',
              }}
            >
              <span className="font-medium">Week {w}</span>
              <span className="text-xs mt-0.5">{completed}/{days.length}</span>
            </button>
          );
        })}
      </div>

      {/* Active week */}
      {data.weeks[activeWeek] && (
        <WeekTimeline
          week={activeWeek}
          days={data.weeks[activeWeek]}
        />
      )}
    </div>
  );
}
