import React from 'react';
import { Link } from 'react-router-dom';
import { Card } from '../ui/Card';
import { Progress } from '../ui/Progress';
import type { Topic } from '../../types';

interface TopicCardProps {
  topic: Topic;
}

export function TopicCard({ topic }: TopicCardProps) {
  return (
    <Link
      to={`/topics/${topic.id}`}
      className="animate-fade-in"
    >
      <Card variant="outlined" hoverable padding="sm">
        <div className="flex items-start gap-3">
          <div
            className="w-10 h-10 rounded-lg flex items-center justify-center text-lg shrink-0"
            style={{ backgroundColor: `${topic.color}20`, border: `1px solid ${topic.color}40` }}
          >
            {topic.icon}
          </div>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <span className="text-xs font-mono" style={{ color: 'var(--color-text-tertiary)' }}>
                TEMA {topic.number}
              </span>
            </div>
            <div className="text-sm font-medium leading-tight mt-0.5 truncate" style={{ color: 'var(--color-text-primary)' }}>
              {topic.title}
            </div>
          </div>
        </div>

        <div className="mt-3">
          <div className="flex items-center justify-between mb-1.5">
            <span className="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
              {topic.completedSubtopics}/{topic.totalSubtopics} subtopics
            </span>
            <span className="text-xs font-mono" style={{ color: 'var(--color-text-secondary)' }}>
              {topic.progress}%
            </span>
          </div>
          <Progress value={topic.progress} />
        </div>
      </Card>
    </Link>
  );
}
