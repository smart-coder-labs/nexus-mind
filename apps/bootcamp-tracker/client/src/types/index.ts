export interface Topic {
  id: number;
  number: number;
  title: string;
  icon: string;
  color: string;
  totalSubtopics: number;
  completedSubtopics: number;
  progress: number;
}

export interface Section {
  id: number;
  topic_id: number;
  title: string;
  section_order: number;
  subtopics: Subtopic[];
  resources: Resource[];
  progress: number;
}

export interface TopicDetail extends Omit<Topic, 'totalSubtopics' | 'completedSubtopics' | 'progress'> {
  sections: Section[];
}

export interface Subtopic {
  id: number;
  section_id: number;
  label: string;
  priority: 'P0' | 'P1' | 'P2';
  estimated_time: string;
  completed: number;
  completed_at: string | null;
  notes: string | null;
}

export interface Resource {
  id: number;
  section_id: number;
  type: 'paper' | 'book' | 'course' | 'repo';
  label: string;
  url: string | null;
}

export interface RoadmapBlock {
  time: string;
  activities: string[];
}

export interface RoadmapDay {
  id: number;
  week: number;
  day: number;
  title: string;
  blocks: RoadmapBlock[];
  completed: number;
}

export interface RoadmapData {
  weeks: Record<number, RoadmapDay[]>;
  days: RoadmapDay[];
}

export interface Reminder {
  id: number;
  subtopic_id: number | null;
  roadmap_day_id: number | null;
  remind_at: string;
  message: string;
  dismissed: number;
  snoozed_until: string | null;
  subtopic_label?: string;
}

export interface StudySession {
  id: number;
  subtopic_id: number | null;
  started_at: string;
  ended_at: string | null;
  duration_minutes: number | null;
  notes: string | null;
  subtopic_label?: string;
}

export interface Stats {
  totalSubtopics: number;
  completedSubtopics: number;
  totalEstimatedHours: number;
  completedHours: number;
  pendingP0: number;
  studyStreak: number;
  hoursPerDay: { date: string; hours: number }[];
}

export interface SearchResult {
  subtopics: Array<{
    id: number;
    label: string;
    priority: 'P0' | 'P1' | 'P2';
    estimated_time: string;
    completed: number;
    section_title: string;
    topic_id: number;
    topic_title: string;
    topic_icon: string;
  }>;
  resources: Array<{
    id: number;
    type: string;
    label: string;
    url: string | null;
    section_title: string;
    topic_id: number;
    topic_title: string;
    topic_icon: string;
  }>;
}

export type Priority = 'P0' | 'P1' | 'P2' | 'all';
export type StatusFilter = 'all' | 'pending' | 'completed';
