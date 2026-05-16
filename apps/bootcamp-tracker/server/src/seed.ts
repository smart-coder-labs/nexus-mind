import path from 'path';
import { getDb } from './db';
import { parseMarkdown } from './parser';

const MD_PATH = path.resolve(__dirname, '..', '..', '..', '..', 'docs', 'CONCEPTUAL_BOOTCAMP.md');
const RESYNC = process.argv.includes('--resync');

function seed() {
  const db = getDb();
  const { topics, roadmapDays } = parseMarkdown(MD_PATH);

  const existingTopics = db.prepare('SELECT COUNT(*) as count FROM topics').get() as { count: number };

  if (existingTopics.count > 0 && !RESYNC) {
    console.log('Database already seeded. Use --resync to update structure while preserving progress.');
    return;
  }

  if (RESYNC) {
    console.log('Resyncing — preserving completed/notes/completed_at on subtopics...');
    // Save existing subtopic state keyed by label+section_title
    const existingState = new Map<string, { completed: number; completed_at: string | null; notes: string | null }>();
    const existing = db.prepare(`
      SELECT s.label, sec.title as sec_title, s.completed, s.completed_at, s.notes
      FROM subtopics s
      JOIN sections sec ON s.section_id = sec.id
    `).all() as Array<{ label: string; sec_title: string; completed: number; completed_at: string | null; notes: string | null }>;
    for (const row of existing) {
      existingState.set(`${row.sec_title}::${row.label}`, {
        completed: row.completed,
        completed_at: row.completed_at,
        notes: row.notes,
      });
    }

    // Save roadmap completion state
    const roadmapState = new Map<string, number>();
    const existingRoadmap = db.prepare('SELECT week, day, completed FROM roadmap_days').all() as Array<{ week: number; day: number; completed: number }>;
    for (const row of existingRoadmap) {
      roadmapState.set(`${row.week}:${row.day}`, row.completed);
    }

    // Clear tables
    db.exec('DELETE FROM resources');
    db.exec('DELETE FROM subtopics');
    db.exec('DELETE FROM sections');
    db.exec('DELETE FROM topics');
    db.exec('DELETE FROM roadmap_days');

    insertData(db, topics, roadmapDays, existingState, roadmapState);
  } else {
    insertData(db, topics, roadmapDays, new Map(), new Map());
  }

  console.log(`Seeded ${topics.length} topics, ${roadmapDays.length} roadmap days.`);
}

function insertData(
  db: ReturnType<typeof getDb>,
  topics: ReturnType<typeof import('./parser').parseMarkdown>['topics'],
  roadmapDays: ReturnType<typeof import('./parser').parseMarkdown>['roadmapDays'],
  existingSubtopicState: Map<string, { completed: number; completed_at: string | null; notes: string | null }>,
  existingRoadmapState: Map<string, number>
) {
  const insertTopic = db.prepare('INSERT INTO topics (number, title, icon, color) VALUES (?, ?, ?, ?)');
  const insertSection = db.prepare('INSERT INTO sections (topic_id, title, section_order) VALUES (?, ?, ?)');
  const insertSubtopic = db.prepare(
    'INSERT INTO subtopics (section_id, label, priority, estimated_time, completed, completed_at, notes) VALUES (?, ?, ?, ?, ?, ?, ?)'
  );
  const insertResource = db.prepare(
    'INSERT INTO resources (section_id, type, label, url) VALUES (?, ?, ?, ?)'
  );
  const insertRoadmapDay = db.prepare(
    'INSERT INTO roadmap_days (week, day, title, blocks, completed) VALUES (?, ?, ?, ?, ?)'
  );

  const runAll = db.transaction(() => {
    for (const topic of topics) {
      const topicResult = insertTopic.run(topic.number, topic.title, topic.icon, topic.color);
      const topicId = topicResult.lastInsertRowid;

      for (let si = 0; si < topic.sections.length; si++) {
        const section = topic.sections[si];
        const sectionResult = insertSection.run(topicId, section.title, si);
        const sectionId = sectionResult.lastInsertRowid;

        for (const subtopic of section.subtopics) {
          const key = `${section.title}::${subtopic.label}`;
          const saved = existingSubtopicState.get(key);
          insertSubtopic.run(
            sectionId,
            subtopic.label,
            subtopic.priority,
            subtopic.estimated_time,
            saved ? saved.completed : (subtopic.completed ? 1 : 0),
            saved ? saved.completed_at : null,
            saved ? saved.notes : null
          );
        }

        for (const resource of section.resources) {
          insertResource.run(sectionId, resource.type, resource.label, resource.url);
        }
      }
    }

    for (const day of roadmapDays) {
      const savedCompleted = existingRoadmapState.get(`${day.week}:${day.day}`) ?? 0;
      insertRoadmapDay.run(
        day.week,
        day.day,
        day.title,
        JSON.stringify(day.blocks),
        savedCompleted
      );
    }
  });

  runAll();
}

seed();
