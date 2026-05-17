import React, { useState } from 'react';
import { Checkbox } from '../ui/Checkbox';
import { PriorityBadge, Badge } from '../ui/Badge';
import { StudyTimer } from './StudyTimer';
import { useToggleSubtopic, useUpdateSubtopicNotes } from '../../hooks/useTopics';
import type { Subtopic } from '../../types';

interface SubtopicRowProps {
  subtopic: Subtopic;
}

export function SubtopicRow({ subtopic }: SubtopicRowProps) {
  const [expanded, setExpanded] = useState(false);
  const [notes, setNotes] = useState(subtopic.notes || '');
  const toggleSubtopic = useToggleSubtopic();
  const updateNotes = useUpdateSubtopicNotes();

  const handleToggle = () => {
    toggleSubtopic.mutate({ id: subtopic.id, completed: !subtopic.completed });
  };

  const handleNotesBlur = () => {
    if (notes !== (subtopic.notes || '')) {
      updateNotes.mutate({ id: subtopic.id, notes });
    }
  };

  return (
    <div
      className={`group rounded-lg transition-colors ${subtopic.completed ? 'opacity-60' : ''}`}
      style={{
        backgroundColor: expanded ? 'var(--color-bg-tertiary)' : undefined,
        border: expanded ? '1px solid var(--color-border-primary)' : '1px solid transparent',
      }}
    >
      <div className="flex items-center gap-3 px-3 py-2.5">
        <Checkbox
          checked={!!subtopic.completed}
          onCheckedChange={handleToggle}
          disabled={toggleSubtopic.isPending}
        />

        <button
          onClick={() => setExpanded(!expanded)}
          className="flex-1 text-left text-sm"
          style={{
            color: subtopic.completed ? 'var(--color-text-tertiary)' : 'var(--color-text-primary)',
            textDecoration: subtopic.completed ? 'line-through' : undefined,
          }}
        >
          {subtopic.label}
        </button>

        <div className="flex items-center gap-2 shrink-0">
          <PriorityBadge priority={subtopic.priority} />
          <Badge variant="default">{subtopic.estimated_time}</Badge>
          <StudyTimer subtopicId={subtopic.id} label={subtopic.label} />
        </div>
      </div>

      {expanded && (
        <div className="px-3 pb-3 animate-fade-in">
          <textarea
            value={notes}
            onChange={e => setNotes(e.target.value)}
            onBlur={handleNotesBlur}
            placeholder="Add notes..."
            rows={3}
            className="w-full text-sm rounded-lg px-3 py-2 resize-none focus-ring transition-colors"
            style={{
              backgroundColor: 'var(--color-bg-secondary)',
              border: '1px solid var(--color-border-primary)',
              color: 'var(--color-text-primary)',
              outline: 'none',
            }}
          />
          {subtopic.completed_at && (
            <div className="text-xs mt-1" style={{ color: 'var(--color-text-tertiary)' }}>
              Completed: {new Date(subtopic.completed_at).toLocaleDateString()}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
