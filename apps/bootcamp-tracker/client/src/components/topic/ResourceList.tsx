import React, { useState } from 'react';
import type { Resource } from '../../types';

interface ResourceListProps {
  resources: Resource[];
}

const typeConfig: Record<Resource['type'], { emoji: string; label: string }> = {
  paper: { emoji: '📄', label: 'Papers' },
  book: { emoji: '📚', label: 'Books' },
  course: { emoji: '🎓', label: 'Courses' },
  repo: { emoji: '💻', label: 'Repos' },
};

export function ResourceList({ resources }: ResourceListProps) {
  const [open, setOpen] = useState(false);

  if (resources.length === 0) return null;

  const grouped = resources.reduce<Record<Resource['type'], Resource[]>>(
    (acc, r) => {
      if (!acc[r.type]) acc[r.type] = [];
      acc[r.type].push(r);
      return acc;
    },
    {} as Record<Resource['type'], Resource[]>
  );

  return (
    <div className="mt-3 rounded-lg overflow-hidden" style={{ border: '1px solid var(--border-subtle)' }}>
      <button
        onClick={() => setOpen(!open)}
        className="w-full flex items-center justify-between px-3 py-2 text-sm hover:bg-white/5 transition-colors"
        style={{ color: 'var(--text-secondary)' }}
      >
        <span className="flex items-center gap-2">
          <span>📖</span>
          <span>Resources ({resources.length})</span>
        </span>
        <svg
          width="14" height="14" viewBox="0 0 16 16" fill="currentColor"
          className={`transition-transform ${open ? 'rotate-180' : ''}`}
        >
          <path d="M4.427 7.427l3.396 3.396a.25.25 0 00.354 0l3.396-3.396A.25.25 0 0011.396 7H4.604a.25.25 0 00-.177.427z" />
        </svg>
      </button>

      {open && (
        <div className="px-3 pb-3 pt-1 space-y-3" style={{ borderTop: '1px solid var(--border-subtle)' }}>
          {(Object.entries(grouped) as [Resource['type'], Resource[]][]).map(([type, items]) => (
            <div key={type}>
              <div className="text-xs font-medium mb-1.5 flex items-center gap-1.5"
                style={{ color: 'var(--text-muted)' }}>
                <span>{typeConfig[type].emoji}</span>
                {typeConfig[type].label}
              </div>
              <ul className="space-y-1">
                {items.map(r => (
                  <li key={r.id} className="text-sm">
                    {r.url ? (
                      <a
                        href={r.url}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="flex items-start gap-1.5 hover:underline"
                        style={{ color: 'var(--accent)' }}
                      >
                        <span className="mt-0.5 shrink-0">↗</span>
                        <span>{r.label}</span>
                      </a>
                    ) : (
                      <span className="flex items-start gap-1.5" style={{ color: 'var(--text-secondary)' }}>
                        <span className="mt-0.5">•</span>
                        <span>{r.label}</span>
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
