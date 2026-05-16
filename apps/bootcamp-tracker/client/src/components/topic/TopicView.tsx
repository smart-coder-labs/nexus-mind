import React from 'react';
import { Breadcrumbs } from '../layout/Breadcrumbs';
import { SectionAccordion } from './SectionAccordion';
import { ProgressBar } from '../dashboard/ProgressBar';
import { LoadingPage } from '../ui/LoadingSpinner';
import { useTopic } from '../../hooks/useTopics';

interface TopicViewProps {
  topicId: number;
}

export function TopicView({ topicId }: TopicViewProps) {
  const { data: topic, isLoading, error } = useTopic(topicId);

  if (isLoading) return <LoadingPage />;
  if (error || !topic) return (
    <div className="text-center py-16" style={{ color: 'var(--text-muted)' }}>
      Topic not found
    </div>
  );

  const totalSubtopics = topic.sections.reduce((a, s) => a + s.subtopics.length, 0);
  const completedSubtopics = topic.sections.reduce((a, s) => a + s.subtopics.filter(st => st.completed).length, 0);
  const progress = totalSubtopics > 0 ? Math.round((completedSubtopics / totalSubtopics) * 100) : 0;

  return (
    <div className="space-y-5">
      <Breadcrumbs crumbs={[
        { label: 'Dashboard', to: '/' },
        { label: `${topic.icon} ${topic.title}` },
      ]} />

      {/* Topic header */}
      <div className="surface rounded-xl p-5">
        <div className="flex items-start gap-4">
          <div
            className="w-14 h-14 rounded-xl flex items-center justify-center text-2xl shrink-0"
            style={{ backgroundColor: `${topic.color}20`, border: `2px solid ${topic.color}40` }}
          >
            {topic.icon}
          </div>
          <div className="flex-1">
            <div className="text-xs font-mono mb-1" style={{ color: 'var(--text-muted)' }}>
              TEMA {topic.number}
            </div>
            <h1 className="text-xl font-semibold mb-1" style={{ color: 'var(--text-primary)' }}>
              {topic.title}
            </h1>
            <div className="flex items-center gap-3">
              <div className="flex-1 max-w-xs">
                <ProgressBar value={progress} color={topic.color} height={6} animated />
              </div>
              <span className="text-sm font-mono tabular-nums" style={{ color: 'var(--text-secondary)' }}>
                {completedSubtopics}/{totalSubtopics} · {progress}%
              </span>
            </div>
          </div>
        </div>
      </div>

      {/* Sections */}
      <div className="space-y-3">
        {topic.sections.map((section, i) => (
          <SectionAccordion
            key={section.id}
            section={section}
            defaultOpen={i === 0}
          />
        ))}
      </div>
    </div>
  );
}
