import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '../api/client';
import type { StudySession } from '../types';

export function useStudySessions() {
  return useQuery<StudySession[]>({
    queryKey: ['sessions'],
    queryFn: () => api.getSessions() as Promise<StudySession[]>,
    staleTime: 30_000,
  });
}

export function useStartSession() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (subtopic_id?: number) => api.startSession({ subtopic_id }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sessions'] });
      qc.invalidateQueries({ queryKey: ['stats'] });
    },
  });
}

export function useStopSession() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => api.patchSession(id, { stop: true }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sessions'] });
      qc.invalidateQueries({ queryKey: ['stats'] });
    },
  });
}

export function useStats() {
  return useQuery({
    queryKey: ['stats'],
    queryFn: () => api.getStats(),
    staleTime: 60_000,
  });
}
