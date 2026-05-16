const BASE = '/api';

async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json', ...options?.headers },
    ...options,
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`API error ${res.status}: ${text}`);
  }
  if (res.status === 204) return undefined as T;
  return res.json();
}

export const api = {
  // Topics
  getTopics: () => request('/topics'),
  getTopic: (id: number) => request(`/topics/${id}`),

  // Subtopics
  patchSubtopic: (id: number, body: { completed?: boolean; notes?: string }) =>
    request(`/subtopics/${id}`, { method: 'PATCH', body: JSON.stringify(body) }),

  // Roadmap
  getRoadmap: () => request('/roadmap'),
  patchRoadmapDay: (id: number, body: { completed: boolean }) =>
    request(`/roadmap-days/${id}`, { method: 'PATCH', body: JSON.stringify(body) }),

  // Reminders
  getReminders: () => request('/reminders'),
  createReminder: (body: { subtopic_id?: number; roadmap_day_id?: number; remind_at: string; message: string }) =>
    request('/reminders', { method: 'POST', body: JSON.stringify(body) }),
  patchReminder: (id: number, body: { dismissed?: boolean; snoozed_until?: string }) =>
    request(`/reminders/${id}`, { method: 'PATCH', body: JSON.stringify(body) }),
  deleteReminder: (id: number) =>
    request(`/reminders/${id}`, { method: 'DELETE' }),

  // Sessions
  getSessions: () => request('/sessions'),
  startSession: (body: { subtopic_id?: number }) =>
    request('/sessions', { method: 'POST', body: JSON.stringify(body) }),
  patchSession: (id: number, body: { stop?: boolean; notes?: string }) =>
    request(`/sessions/${id}`, { method: 'PATCH', body: JSON.stringify(body) }),

  // Stats
  getStats: () => request('/stats'),

  // Search
  search: (q: string) => request(`/search?q=${encodeURIComponent(q)}`),
};
