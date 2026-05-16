import React, { useState } from 'react';
import { TopicCard } from './TopicCard';
import { StatsPanel } from './StatsPanel';
import { StudyStreak } from './StudyStreak';
import { ProgressBar } from './ProgressBar';
import { LoadingPage } from '../ui/LoadingSpinner';
import { useTopics } from '../../hooks/useTopics';
import { useStats } from '../../hooks/useStudySessions';
import type { Priority, StatusFilter } from '../../types';

export function Dashboard() {
  const { data: topics, isLoading: topicsLoading } = useTopics();
  const { data: stats, isLoading: statsLoading } = useStats();
  const [priority, setPriority] = useState<Priority>('all');
  const [status, setStatus] = useState<StatusFilter>('all');

  if (topicsLoading || statsLoading) return <LoadingPage />;
  if (!topics || !stats) return null;

  const statsTyped = stats as import('../../types').Stats;
  const overallProgress = statsTyped.totalSubtopics > 0
    ? Math.round((statsTyped.completedSubtopics / statsTyped.totalSubtopics) * 100)
    : 0;

  return (
    <div className="space-y-6">
      {/* Overall progress banner */}
      <div className="surface rounded-xl p-5">
        <div className="flex items-center justify-between mb-3">
          <div>
            <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
              NexusMind Conceptual Bootcamp
            </h1>
            <p className="text-sm mt-0.5" style={{ color: 'var(--text-secondary)' }}>
              12 topics · 120h · 4 weeks
            </p>
          </div>
          <div className="text-3xl font-mono font-bold tabular-nums" style={{ color: 'var(--accent)' }}>
            {overallProgress}%
          </div>
        </div>
        <ProgressBar value={overallProgress} height={8} animated />
      </div>

      {/* Stats */}
      <StatsPanel stats={statsTyped} />

      {/* Study streak chart */}
      {statsTyped.hoursPerDay.length > 0 && (
        <StudyStreak hoursPerDay={statsTyped.hoursPerDay} />
      )}

      {/* Filter bar */}
      <div className="flex items-center gap-2 flex-wrap">
        <div className="flex items-center gap-1 p-1 rounded-lg" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}>
          {(['all', 'P0', 'P1', 'P2'] as Priority[]).map(p => (
            <button
              key={p}
              onClick={() => setPriority(p)}
              className={`px-3 py-1 rounded-md text-sm font-medium transition-colors ${
                priority === p ? 'shadow-sm' : ''
              }`}
              style={{
                backgroundColor: priority === p ? 'var(--bg-tertiary)' : 'transparent',
                color: priority === p ? 'var(--text-primary)' : 'var(--text-muted)',
              }}
            >
              {p === 'all' ? 'All' : p}
            </button>
          ))}
        </div>

        <div className="flex items-center gap-1 p-1 rounded-lg" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}>
          {(['all', 'pending', 'completed'] as StatusFilter[]).map(s => (
            <button
              key={s}
              onClick={() => setStatus(s)}
              className={`px-3 py-1 rounded-md text-sm font-medium transition-colors capitalize ${
                status === s ? 'shadow-sm' : ''
              }`}
              style={{
                backgroundColor: status === s ? 'var(--bg-tertiary)' : 'transparent',
                color: status === s ? 'var(--text-primary)' : 'var(--text-muted)',
              }}
            >
              {s}
            </button>
          ))}
        </div>
      </div>

      {/* Topic grid */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3">
        {topics.map(topic => (
          <TopicCard key={topic.id} topic={topic} />
        ))}
      </div>
    </div>
  );
}
