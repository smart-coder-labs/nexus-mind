import React, { useState } from 'react';
import { format, parseISO } from 'date-fns';
import { useReminders, useDismissReminder, useSnoozeReminder, useDeleteReminder } from '../../hooks/useReminders';
import { ReminderForm } from './ReminderForm';
import { LoadingPage } from '../ui/LoadingSpinner';
import type { Reminder } from '../../types';

function ReminderItem({ reminder }: { reminder: Reminder }) {
  const dismiss = useDismissReminder();
  const snooze = useSnoozeReminder();
  const deleteR = useDeleteReminder();

  const isPast = new Date(reminder.remind_at) < new Date();
  const isSnoozed = reminder.snoozed_until && new Date(reminder.snoozed_until) > new Date();

  return (
    <div className="surface rounded-xl p-4">
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-start gap-3">
          <span className="text-lg mt-0.5">{isPast ? '⏰' : '🔔'}</span>
          <div>
            <div className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
              {reminder.message}
            </div>
            {reminder.subtopic_label && (
              <div className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
                Re: {reminder.subtopic_label}
              </div>
            )}
            <div className="flex items-center gap-3 mt-1.5">
              <span className={`text-xs font-mono ${isPast ? 'text-red-400' : ''}`}
                style={!isPast ? { color: 'var(--text-muted)' } : undefined}>
                {format(parseISO(reminder.remind_at), 'MMM d, yyyy HH:mm')}
              </span>
              {isSnoozed && reminder.snoozed_until && (
                <span className="text-xs" style={{ color: 'var(--warning)' }}>
                  Snoozed until {format(parseISO(reminder.snoozed_until), 'HH:mm')}
                </span>
              )}
            </div>
          </div>
        </div>

        <div className="flex items-center gap-1 shrink-0">
          <select
            onChange={e => {
              if (e.target.value) {
                snooze.mutate({ id: reminder.id, minutes: parseInt(e.target.value) });
                e.target.value = '';
              }
            }}
            className="text-xs rounded px-2 py-1 cursor-pointer"
            style={{
              backgroundColor: 'var(--bg-tertiary)',
              border: '1px solid var(--border)',
              color: 'var(--text-secondary)',
              outline: 'none',
            }}
            defaultValue=""
          >
            <option value="" disabled>Snooze</option>
            <option value="15">15 min</option>
            <option value="60">1 hour</option>
            <option value="1440">1 day</option>
          </select>

          <button
            onClick={() => dismiss.mutate(reminder.id)}
            disabled={dismiss.isPending}
            className="px-2 py-1 rounded text-xs transition-colors hover:bg-white/5"
            style={{ color: 'var(--success)', border: '1px solid rgba(63, 185, 80, 0.3)' }}
            title="Dismiss"
          >
            ✓
          </button>

          <button
            onClick={() => deleteR.mutate(reminder.id)}
            disabled={deleteR.isPending}
            className="px-2 py-1 rounded text-xs transition-colors hover:bg-white/5"
            style={{ color: 'var(--danger)', border: '1px solid rgba(248, 81, 73, 0.3)' }}
            title="Delete"
          >
            ✕
          </button>
        </div>
      </div>
    </div>
  );
}

export function ReminderList() {
  const { data: reminders, isLoading } = useReminders();
  const [showForm, setShowForm] = useState(false);

  if (isLoading) return <LoadingPage />;

  const active = (reminders || []).filter(r => !r.dismissed);

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold" style={{ color: 'var(--text-primary)' }}>
          🔔 Reminders
        </h1>
        <button
          onClick={() => setShowForm(true)}
          className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-colors"
          style={{ backgroundColor: 'var(--accent)', color: 'white' }}
        >
          + New Reminder
        </button>
      </div>

      {active.length === 0 ? (
        <div className="surface rounded-xl p-12 text-center">
          <div className="text-4xl mb-3">🔔</div>
          <div className="text-sm" style={{ color: 'var(--text-muted)' }}>
            No active reminders
          </div>
          <button
            onClick={() => setShowForm(true)}
            className="mt-4 text-sm"
            style={{ color: 'var(--accent)' }}
          >
            Create one
          </button>
        </div>
      ) : (
        <div className="space-y-3">
          {active.map(r => (
            <ReminderItem key={r.id} reminder={r} />
          ))}
        </div>
      )}

      <ReminderForm isOpen={showForm} onClose={() => setShowForm(false)} />
    </div>
  );
}
