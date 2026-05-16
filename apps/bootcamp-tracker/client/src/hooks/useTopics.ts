import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '../api/client';
import type { Topic, TopicDetail } from '../types';

export function useTopics() {
  return useQuery<Topic[]>({
    queryKey: ['topics'],
    queryFn: () => api.getTopics() as Promise<Topic[]>,
    staleTime: 30_000,
  });
}

export function useTopic(id: number) {
  return useQuery<TopicDetail>({
    queryKey: ['topic', id],
    queryFn: () => api.getTopic(id) as Promise<TopicDetail>,
    staleTime: 30_000,
  });
}

export function useToggleSubtopic() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, completed }: { id: number; completed: boolean }) =>
      api.patchSubtopic(id, { completed }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['topics'] });
      qc.invalidateQueries({ queryKey: ['topic'] });
      qc.invalidateQueries({ queryKey: ['stats'] });
    },
  });
}

export function useUpdateSubtopicNotes() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, notes }: { id: number; notes: string }) =>
      api.patchSubtopic(id, { notes }),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ['topic'] });
    },
  });
}
