import React from 'react';
import { format, parseISO } from 'date-fns';
import { useStudySessions } from '../hooks/useStudySessions';
import { LoadingPage } from '../components/ui/LoadingSpinner';
import type { StudySession } from '../types';

function SessionRow({ session }: { session: StudySession }) {
  const duration = session.duration_minutes
    ? `${Math.floor(session.duration_minutes)}m`
    : session.ended_at ? '—' : '⏳ Active';

  return (
    <div className="surface rounded-xl p-4">
      <div className="flex items-start justify-between gap-4">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <span className={`text-xs px-2 py-0.5 rounded font-mono ${
              !session.ended_at ? 'text-green-400 bg-green-900/30' : 'text-gray-400 bg-gray-800/50'
            }`} style={{ border: !session.ended_at ? '1px solid rgba(63,185,80,0.3)' : '1px solid var(--border)' }}>
              {!session.ended_at ? '● ACTIVE' : '✓ Done'}
            </span>
            {session.subtopic_label && (
              <span className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>
                {session.subtopic_label}
              </span>
            )}
          </div>
          <div className="flex items-center gap-4 mt-2">
            <div className="text-xs" style={{ color: 'var(--text-muted)' }}>
              Started: {format(parseISO(session.started_at), 'MMM d, HH:mm')}
            </div>
            {session.ended_at && (
              <div className="text-xs" style={{ color: 'var(--text-muted)' }}>
                Ended: {format(parseISO(session.ended_at), 'HH:mm')}
              </div>
            )}
          </div>
          {session.notes && (
            <div className="text-xs mt-2 italic" style={{ color: 'var(--text-secondary)' }}>
              {session.notes}
            </div>
          )}
        </div>
        <div className="shrink-0 text-right">
          <div className="text-lg font-mono font-semibold" style={{ color: 'var(--accent)' }}>
            {duration}
          </div>
        </div>
      </div>
    </div>
  );
}

export function SessionsPage() {
  const { data: sessions, isLoading } = useStudySessions();

  if (isLoading) return <LoadingPage />;

  const all = sessions || [];
  const totalMinutes = all.reduce((acc, s) => acc + (s.duration_minutes || 0), 0);
  const totalHours = (totalMinutes / 60).toFixed(1);

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold" style={{ color: 'var(--text-primary)' }}>
          ⏱️ Study Sessions
        </h1>
        <div className="text-sm font-mono" style={{ color: 'var(--text-muted)' }}>
          {totalHours}h total · {all.length} sessions
        </div>
      </div>

      {all.length === 0 ? (
        <div className="surface rounded-xl p-12 text-center">
          <div className="text-4xl mb-3">⏱️</div>
          <div className="text-sm" style={{ color: 'var(--text-muted)' }}>
            No sessions yet. Start a timer on any subtopic.
          </div>
        </div>
      ) : (
        <div className="space-y-3">
          {all.map(s => (
            <SessionRow key={s.id} session={s} />
          ))}
        </div>
      )}
    </div>
  );
}
