import React, { useState } from 'react';
import { Modal } from '../ui/Modal';
import { useCreateReminder } from '../../hooks/useReminders';
import { format } from 'date-fns';

interface ReminderFormProps {
  isOpen: boolean;
  onClose: () => void;
}

export function ReminderForm({ isOpen, onClose }: ReminderFormProps) {
  const create = useCreateReminder();
  const [message, setMessage] = useState('');
  const [remindAt, setRemindAt] = useState(
    format(new Date(Date.now() + 3600_000), "yyyy-MM-dd'T'HH:mm")
  );

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!message.trim()) return;
    try {
      await create.mutateAsync({
        remind_at: new Date(remindAt).toISOString(),
        message: message.trim(),
      });
      setMessage('');
      onClose();
    } catch (err) {
      console.error(err);
    }
  };

  return (
    <Modal isOpen={isOpen} onClose={onClose} title="New Reminder">
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label className="block text-sm mb-1.5" style={{ color: 'var(--text-secondary)' }}>
            Message
          </label>
          <input
            type="text"
            value={message}
            onChange={e => setMessage(e.target.value)}
            placeholder="What do you want to be reminded about?"
            required
            className="w-full px-3 py-2 rounded-lg text-sm focus-ring"
            style={{
              backgroundColor: 'var(--bg-tertiary)',
              border: '1px solid var(--border)',
              color: 'var(--text-primary)',
              outline: 'none',
            }}
          />
        </div>
        <div>
          <label className="block text-sm mb-1.5" style={{ color: 'var(--text-secondary)' }}>
            Remind at
          </label>
          <input
            type="datetime-local"
            value={remindAt}
            onChange={e => setRemindAt(e.target.value)}
            required
            className="w-full px-3 py-2 rounded-lg text-sm focus-ring"
            style={{
              backgroundColor: 'var(--bg-tertiary)',
              border: '1px solid var(--border)',
              color: 'var(--text-primary)',
              outline: 'none',
            }}
          />
        </div>
        <div className="flex gap-3 justify-end pt-2">
          <button
            type="button"
            onClick={onClose}
            className="px-4 py-2 rounded-lg text-sm transition-colors hover:bg-white/5"
            style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={create.isPending}
            className="px-4 py-2 rounded-lg text-sm font-medium transition-colors"
            style={{ backgroundColor: 'var(--accent)', color: 'white' }}
          >
            {create.isPending ? 'Creating...' : 'Create Reminder'}
          </button>
        </div>
      </form>
    </Modal>
  );
}
