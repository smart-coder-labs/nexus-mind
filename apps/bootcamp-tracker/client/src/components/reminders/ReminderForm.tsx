import React, { useState } from 'react';
import { Modal, ModalHeader, ModalTitle, ModalContent, ModalCloseButton } from '../ui/Modal';
import { Input } from '../ui/Input';
import { Button } from '../ui/Button';
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
    <Modal open={isOpen} onOpenChange={(open) => { if (!open) onClose(); }}>
      <ModalCloseButton />
      <ModalHeader>
        <ModalTitle>New Reminder</ModalTitle>
      </ModalHeader>
      <ModalContent>
        <form onSubmit={handleSubmit} className="space-y-4">
          <Input
            label="Message"
            type="text"
            value={message}
            onChange={e => setMessage(e.target.value)}
            placeholder="What do you want to be reminded about?"
            required
          />
          <Input
            label="Remind at"
            type="datetime-local"
            value={remindAt}
            onChange={e => setRemindAt(e.target.value)}
            required
          />
          <div className="flex gap-3 justify-end pt-2">
            <Button type="button" variant="outline" size="sm" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" variant="primary" size="sm" loading={create.isPending}>
              {create.isPending ? 'Creating...' : 'Create Reminder'}
            </Button>
          </div>
        </form>
      </ModalContent>
    </Modal>
  );
}
