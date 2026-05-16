import React, { useState } from 'react';
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
        backgroundColor: expanded ? 'var(--bg-tertiary)' : undefined,
        border: expanded ? '1px solid var(--border)' : '1px solid transparent',
      }}
    >
      <div className="flex items-center gap-3 px-3 py-2.5">
        {/* Checkbox */}
        <input
          type="checkbox"
          className="custom-checkbox"
          checked={!!subtopic.completed}
          onChange={handleToggle}
          disabled={toggleSubtopic.isPending}
        />

        {/* Label */}
        <button
          onClick={() => setExpanded(!expanded)}
          className="flex-1 text-left text-sm"
          style={{
            color: subtopic.completed ? 'var(--text-muted)' : 'var(--text-primary)',
            textDecoration: subtopic.completed ? 'line-through' : undefined,
          }}
        >
          {subtopic.label}
        </button>

        {/* Badges */}
        <div className="flex items-center gap-2 shrink-0">
          <PriorityBadge priority={subtopic.priority} />
          <Badge variant="default">{subtopic.estimated_time}</Badge>
          <StudyTimer subtopicId={subtopic.id} label={subtopic.label} />
        </div>
      </div>

      {/* Expanded notes area */}
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
              backgroundColor: 'var(--bg-secondary)',
              border: '1px solid var(--border)',
              color: 'var(--text-primary)',
              outline: 'none',
            }}
          />
          {subtopic.completed_at && (
            <div className="text-xs mt-1" style={{ color: 'var(--text-muted)' }}>
              Completed: {new Date(subtopic.completed_at).toLocaleDateString()}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
