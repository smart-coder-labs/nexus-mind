import React, { useEffect, useState } from 'react';
import { useReminders, useDismissReminder, useSnoozeReminder } from '../../hooks/useReminders';
import type { Reminder } from '../../types';

function ToastItem({ reminder, onDismiss, onSnooze }: {
  reminder: Reminder;
  onDismiss: () => void;
  onSnooze: (minutes: number) => void;
}) {
  return (
    <div
      className="w-80 surface rounded-xl p-4 shadow-2xl animate-slide-in"
      style={{ border: '1px solid var(--accent)', boxShadow: '0 0 20px rgba(88,166,255,0.1)' }}
    >
      <div className="flex items-start gap-2 mb-3">
        <span className="text-lg">🔔</span>
        <div className="flex-1">
          <div className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
            Reminder
          </div>
          <div className="text-xs mt-0.5" style={{ color: 'var(--text-secondary)' }}>
            {reminder.message}
          </div>
          {reminder.subtopic_label && (
            <div className="text-xs mt-1" style={{ color: 'var(--text-muted)' }}>
              Re: {reminder.subtopic_label}
            </div>
          )}
        </div>
        <button onClick={onDismiss} className="shrink-0 hover:opacity-70" style={{ color: 'var(--text-muted)' }}>
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M3.72 3.72a.75.75 0 011.06 0L8 6.94l3.22-3.22a.75.75 0 111.06 1.06L9.06 8l3.22 3.22a.75.75 0 11-1.06 1.06L8 9.06l-3.22 3.22a.75.75 0 01-1.06-1.06L6.94 8 3.72 4.78a.75.75 0 010-1.06z" />
          </svg>
        </button>
      </div>
      <div className="flex items-center gap-2">
        <button
          onClick={onDismiss}
          className="flex-1 py-1.5 rounded text-xs font-medium transition-colors hover:bg-white/10"
          style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
        >
          Dismiss
        </button>
        <button
          onClick={() => onSnooze(15)}
          className="flex-1 py-1.5 rounded text-xs font-medium transition-colors hover:bg-white/10"
          style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
        >
          15m
        </button>
        <button
          onClick={() => onSnooze(60)}
          className="flex-1 py-1.5 rounded text-xs font-medium transition-colors hover:bg-white/10"
          style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
        >
          1h
        </button>
      </div>
    </div>
  );
}

export function ReminderToast() {
  const { data: reminders } = useReminders();
  const dismiss = useDismissReminder();
  const snooze = useSnoozeReminder();
  const [shown, setShown] = useState<Set<number>>(new Set());

  const now = new Date();
  const due = (reminders || []).filter(r => {
    if (r.dismissed) return false;
    const remindAt = new Date(r.remind_at);
    if (remindAt > now) return false;
    if (r.snoozed_until && new Date(r.snoozed_until) > now) return false;
    return !shown.has(r.id);
  }).slice(0, 3);

  if (due.length === 0) return null;

  return (
    <div className="fixed top-4 right-4 z-50 flex flex-col gap-3">
      {due.map(r => (
        <ToastItem
          key={r.id}
          reminder={r}
          onDismiss={() => {
            dismiss.mutate(r.id);
            setShown(prev => new Set(prev).add(r.id));
          }}
          onSnooze={(minutes) => {
            snooze.mutate({ id: r.id, minutes });
            setShown(prev => new Set(prev).add(r.id));
          }}
        />
      ))}
    </div>
  );
}
