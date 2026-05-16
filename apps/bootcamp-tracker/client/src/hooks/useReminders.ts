import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '../api/client';
import type { Reminder } from '../types';

export function useReminders() {
  return useQuery<Reminder[]>({
    queryKey: ['reminders'],
    queryFn: () => api.getReminders() as Promise<Reminder[]>,
    refetchInterval: 60_000, // Poll every 60s
    staleTime: 30_000,
  });
}

export function useCreateReminder() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: { subtopic_id?: number; roadmap_day_id?: number; remind_at: string; message: string }) =>
      api.createReminder(body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['reminders'] });
    },
  });
}

export function useDismissReminder() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => api.patchReminder(id, { dismissed: true }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['reminders'] });
    },
  });
}

export function useSnoozeReminder() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, minutes }: { id: number; minutes: number }) => {
      const snoozedUntil = new Date(Date.now() + minutes * 60_000).toISOString();
      return api.patchReminder(id, { snoozed_until: snoozedUntil });
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['reminders'] });
    },
  });
}

export function useDeleteReminder() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => api.deleteReminder(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['reminders'] });
    },
  });
}
