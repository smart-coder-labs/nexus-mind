import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '../api/client';
import type { RoadmapData } from '../types';

export function useRoadmap() {
  return useQuery<RoadmapData>({
    queryKey: ['roadmap'],
    queryFn: () => api.getRoadmap() as Promise<RoadmapData>,
    staleTime: 60_000,
  });
}

export function useToggleRoadmapDay() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, completed }: { id: number; completed: boolean }) =>
      api.patchRoadmapDay(id, { completed }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['roadmap'] });
    },
  });
}
