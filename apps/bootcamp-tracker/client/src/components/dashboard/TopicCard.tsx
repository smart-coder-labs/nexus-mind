import React from 'react';
import { Link } from 'react-router-dom';
import { ProgressBar } from './ProgressBar';
import type { Topic } from '../../types';

interface TopicCardProps {
  topic: Topic;
}

export function TopicCard({ topic }: TopicCardProps) {
  return (
    <Link
      to={`/topics/${topic.id}`}
      className="surface surface-hover rounded-xl p-4 flex flex-col gap-3 transition-all hover:shadow-lg hover:-translate-y-0.5 animate-fade-in"
      style={{ '--tw-translate-y': '-2px' } as React.CSSProperties}
    >
      {/* Header */}
      <div className="flex items-start gap-3">
        <div
          className="w-10 h-10 rounded-lg flex items-center justify-center text-lg shrink-0"
          style={{ backgroundColor: `${topic.color}20`, border: `1px solid ${topic.color}40` }}
        >
          {topic.icon}
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-xs font-mono" style={{ color: 'var(--text-muted)' }}>
              TEMA {topic.number}
            </span>
          </div>
          <div className="text-sm font-medium leading-tight mt-0.5 truncate" style={{ color: 'var(--text-primary)' }}>
            {topic.title}
          </div>
        </div>
      </div>

      {/* Progress */}
      <div>
        <div className="flex items-center justify-between mb-1.5">
          <span className="text-xs" style={{ color: 'var(--text-muted)' }}>
            {topic.completedSubtopics}/{topic.totalSubtopics} subtopics
          </span>
          <span className="text-xs font-mono" style={{ color: 'var(--text-secondary)' }}>
            {topic.progress}%
          </span>
        </div>
        <ProgressBar value={topic.progress} color={topic.color} height={4} />
      </div>
    </Link>
  );
}
