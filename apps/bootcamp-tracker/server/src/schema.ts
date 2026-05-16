export const SCHEMA_SQL = `
CREATE TABLE IF NOT EXISTS topics (
  id INTEGER PRIMARY KEY,
  number INTEGER NOT NULL,
  title TEXT NOT NULL,
  icon TEXT NOT NULL,
  color TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sections (
  id INTEGER PRIMARY KEY,
  topic_id INTEGER NOT NULL REFERENCES topics(id),
  title TEXT NOT NULL,
  section_order INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS subtopics (
  id INTEGER PRIMARY KEY,
  section_id INTEGER NOT NULL REFERENCES sections(id),
  label TEXT NOT NULL,
  priority TEXT NOT NULL CHECK(priority IN ('P0','P1','P2')),
  estimated_time TEXT NOT NULL,
  completed INTEGER NOT NULL DEFAULT 0,
  completed_at TEXT,
  notes TEXT
);

CREATE TABLE IF NOT EXISTS resources (
  id INTEGER PRIMARY KEY,
  section_id INTEGER NOT NULL REFERENCES sections(id),
  type TEXT NOT NULL CHECK(type IN ('paper','book','course','repo')),
  label TEXT NOT NULL,
  url TEXT
);

CREATE TABLE IF NOT EXISTS roadmap_days (
  id INTEGER PRIMARY KEY,
  week INTEGER NOT NULL,
  day INTEGER NOT NULL,
  title TEXT NOT NULL,
  blocks TEXT NOT NULL,
  completed INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS reminders (
  id INTEGER PRIMARY KEY,
  subtopic_id INTEGER REFERENCES subtopics(id),
  roadmap_day_id INTEGER REFERENCES roadmap_days(id),
  remind_at TEXT NOT NULL,
  message TEXT NOT NULL,
  dismissed INTEGER NOT NULL DEFAULT 0,
  snoozed_until TEXT
);

CREATE TABLE IF NOT EXISTS study_sessions (
  id INTEGER PRIMARY KEY,
  subtopic_id INTEGER REFERENCES subtopics(id),
  started_at TEXT NOT NULL,
  ended_at TEXT,
  duration_minutes REAL,
  notes TEXT
);
`;
