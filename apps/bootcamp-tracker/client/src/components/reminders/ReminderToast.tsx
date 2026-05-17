import React, { useState } from 'react';
import { Snackbar } from '../ui/Snackbar';
import { useReminders, useDismissReminder, useSnoozeReminder } from '../../hooks/useReminders';

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
        <Snackbar
          key={r.id}
          show
          variant="info"
          message={r.message}
          duration={0}
          action={{
            label: 'Snooze 15m',
            onClick: () => {
              snooze.mutate({ id: r.id, minutes: 15 });
              setShown(prev => new Set(prev).add(r.id));
            },
          }}
          onClose={() => {
            dismiss.mutate(r.id);
            setShown(prev => new Set(prev).add(r.id));
          }}
        />
      ))}
    </div>
  );
}
