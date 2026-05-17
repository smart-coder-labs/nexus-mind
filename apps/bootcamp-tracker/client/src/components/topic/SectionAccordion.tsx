import React from 'react';
import { Accordion, AccordionItem, AccordionTrigger, AccordionContent } from '../ui/Accordion';
import { Progress } from '../ui/Progress';
import { SubtopicRow } from './SubtopicRow';
import { ResourceList } from './ResourceList';
import type { Section } from '../../types';

interface SectionAccordionProps {
  section: Section;
  defaultOpen?: boolean;
}

export function SectionAccordion({ section, defaultOpen = false }: SectionAccordionProps) {
  const completed = section.subtopics.filter(s => s.completed).length;
  const total = section.subtopics.length;

  return (
    <Accordion type="single" collapsible defaultValue={defaultOpen ? 'section' : undefined}>
      <AccordionItem value="section">
        <AccordionTrigger>
          <span className="flex-1 text-left text-sm font-medium">{section.title}</span>
          <div className="flex items-center gap-3 shrink-0">
            <div className="hidden sm:flex items-center gap-2 w-24">
              <Progress value={section.progress} />
            </div>
            <span className="text-xs font-mono tabular-nums" style={{ color: 'var(--color-text-tertiary)' }}>
              {completed}/{total}
            </span>
          </div>
        </AccordionTrigger>
        <AccordionContent>
          <div className="space-y-0.5 mt-2">
            {section.subtopics.map(subtopic => (
              <SubtopicRow key={subtopic.id} subtopic={subtopic} />
            ))}
          </div>
          <div className="mt-2 px-1">
            <ResourceList resources={section.resources} />
          </div>
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}
