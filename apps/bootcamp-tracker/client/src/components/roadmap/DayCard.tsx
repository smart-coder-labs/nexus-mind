import React from 'react';
import { Checkbox } from '../ui/Checkbox';
import { useToggleRoadmapDay } from '../../hooks/useRoadmap';
import type { RoadmapDay } from '../../types';

interface DayCardProps {
  day: RoadmapDay;
  isToday?: boolean;
}

export function DayCard({ day, isToday }: DayCardProps) {
  const toggle = useToggleRoadmapDay();

  const handleToggle = () => {
    toggle.mutate({ id: day.id, completed: !day.completed });
  };

  return (
    <div
      className={`surface rounded-xl p-4 flex flex-col gap-3 transition-all ${
        isToday ? 'ring-2' : ''
      } ${day.completed ? 'opacity-70' : ''}`}
      style={isToday ? { boxShadow: '0 0 0 2px var(--color-accent-blue)' } : undefined}
    >
      <div className="flex items-start gap-3">
        <Checkbox
          checked={!!day.completed}
          onCheckedChange={handleToggle}
          disabled={toggle.isPending}
        />
        <div className="flex-1">
          <div className="flex items-center gap-2 flex-wrap">
            <span
              className="text-xs font-mono px-2 py-0.5 rounded"
              style={{
                backgroundColor: isToday ? 'var(--color-accent-blue-tint)' : 'var(--color-bg-tertiary)',
                color: isToday ? 'var(--color-accent-blue)' : 'var(--color-text-tertiary)',
                border: isToday ? '1px solid var(--color-accent-blue-tint)' : '1px solid var(--color-border-primary)',
              }}
            >
              W{day.week}D{day.day}
            </span>
            {isToday && (
              <span className="text-xs px-2 py-0.5 rounded font-medium" style={{
                backgroundColor: 'var(--color-accent-blue-tint)',
                color: 'var(--color-accent-blue)',
                border: '1px solid var(--color-accent-blue-tint)',
              }}>
                Today
              </span>
            )}
          </div>
          <div
            className={`text-sm font-medium mt-1 ${day.completed ? 'line-through' : ''}`}
            style={{ color: 'var(--color-text-primary)' }}
          >
            {day.title}
          </div>
        </div>
      </div>

      {day.blocks.map((block, i) => (
        <div key={i}>
          <div className="text-xs font-medium mb-1.5" style={{ color: 'var(--color-text-tertiary)' }}>
            {block.time}
          </div>
          <ul className="space-y-1">
            {block.activities.map((activity, j) => (
              <li key={j} className="flex items-start gap-2 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                <span className="mt-0.5 shrink-0" style={{ color: 'var(--color-text-tertiary)' }}>•</span>
                <span>{activity}</span>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </div>
  );
}
