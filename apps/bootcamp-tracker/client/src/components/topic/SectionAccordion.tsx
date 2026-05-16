import React, { useState } from 'react';
import { SubtopicRow } from './SubtopicRow';
import { ResourceList } from './ResourceList';
import { ProgressBar } from '../dashboard/ProgressBar';
import type { Section } from '../../types';

interface SectionAccordionProps {
  section: Section;
  defaultOpen?: boolean;
}

export function SectionAccordion({ section, defaultOpen = false }: SectionAccordionProps) {
  const [open, setOpen] = useState(defaultOpen);

  const completed = section.subtopics.filter(s => s.completed).length;
  const total = section.subtopics.length;

  return (
    <div className="surface rounded-xl overflow-hidden">
      {/* Header */}
      <button
        onClick={() => setOpen(!open)}
        className="w-full flex items-center gap-3 px-4 py-3.5 hover:bg-white/5 transition-colors"
      >
        {/* Chevron */}
        <svg
          width="14" height="14" viewBox="0 0 16 16" fill="currentColor"
          className={`shrink-0 transition-transform ${open ? 'rotate-90' : ''}`}
          style={{ color: 'var(--text-muted)' }}
        >
          <path d="M6.22 3.22a.75.75 0 011.06 0l4.25 4.25a.75.75 0 010 1.06l-4.25 4.25a.75.75 0 01-1.06-1.06L9.94 8 6.22 4.28a.75.75 0 010-1.06z" />
        </svg>

        {/* Title */}
        <span className="flex-1 text-left text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
          {section.title}
        </span>

        {/* Progress */}
        <div className="flex items-center gap-3 shrink-0">
          <div className="hidden sm:flex items-center gap-2 w-24">
            <ProgressBar value={section.progress} height={4} animated={false} />
          </div>
          <span className="text-xs font-mono tabular-nums" style={{ color: 'var(--text-muted)' }}>
            {completed}/{total}
          </span>
        </div>
      </button>

      {/* Body */}
      {open && (
        <div className="px-2 pb-3 animate-fade-in" style={{ borderTop: '1px solid var(--border)' }}>
          <div className="space-y-0.5 mt-2">
            {section.subtopics.map(subtopic => (
              <SubtopicRow key={subtopic.id} subtopic={subtopic} />
            ))}
          </div>
          <div className="mt-2 px-1">
            <ResourceList resources={section.resources} />
          </div>
        </div>
      )}
    </div>
  );
}
