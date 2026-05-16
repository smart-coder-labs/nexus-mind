import React from 'react';
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
      style={isToday ? { ringColor: 'var(--accent)', boxShadow: '0 0 0 2px var(--accent)' } : undefined}
    >
      {/* Header */}
      <div className="flex items-start gap-3">
        <input
          type="checkbox"
          className="custom-checkbox mt-0.5"
          checked={!!day.completed}
          onChange={handleToggle}
          disabled={toggle.isPending}
        />
        <div className="flex-1">
          <div className="flex items-center gap-2 flex-wrap">
            <span
              className="text-xs font-mono px-2 py-0.5 rounded"
              style={{
                backgroundColor: isToday ? 'rgba(88, 166, 255, 0.15)' : 'var(--bg-tertiary)',
                color: isToday ? 'var(--accent)' : 'var(--text-muted)',
                border: isToday ? '1px solid rgba(88, 166, 255, 0.3)' : '1px solid var(--border)',
              }}
            >
              W{day.week}D{day.day}
            </span>
            {isToday && (
              <span className="text-xs px-2 py-0.5 rounded font-medium" style={{
                backgroundColor: 'rgba(88, 166, 255, 0.15)',
                color: 'var(--accent)',
                border: '1px solid rgba(88, 166, 255, 0.3)',
              }}>
                Today
              </span>
            )}
          </div>
          <div
            className={`text-sm font-medium mt-1 ${day.completed ? 'line-through' : ''}`}
            style={{ color: 'var(--text-primary)' }}
          >
            {day.title}
          </div>
        </div>
      </div>

      {/* Blocks */}
      {day.blocks.map((block, i) => (
        <div key={i}>
          <div className="text-xs font-medium mb-1.5" style={{ color: 'var(--text-muted)' }}>
            {block.time}
          </div>
          <ul className="space-y-1">
            {block.activities.map((activity, j) => (
              <li key={j} className="flex items-start gap-2 text-xs" style={{ color: 'var(--text-secondary)' }}>
                <span className="mt-0.5 shrink-0" style={{ color: 'var(--text-muted)' }}>•</span>
                <span>{activity}</span>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </div>
  );
}
