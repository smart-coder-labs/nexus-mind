import { Router, Request, Response } from 'express';
import { getDb } from '../db';

const router = Router();

// GET /api/topics — all topics with progress %
router.get('/', (_req: Request, res: Response) => {
  const db = getDb();

  const topics = db.prepare('SELECT * FROM topics ORDER BY number').all() as Array<{
    id: number; number: number; title: string; icon: string; color: string;
  }>;

  const result = topics.map(topic => {
    const stats = db.prepare(`
      SELECT
        COUNT(*) as total,
        SUM(s.completed) as completed
      FROM subtopics s
      JOIN sections sec ON s.section_id = sec.id
      WHERE sec.topic_id = ?
    `).get(topic.id) as { total: number; completed: number };

    return {
      ...topic,
      totalSubtopics: stats.total,
      completedSubtopics: stats.completed || 0,
      progress: stats.total > 0 ? Math.round(((stats.completed || 0) / stats.total) * 100) : 0,
    };
  });

  res.json(result);
});

// GET /api/topics/:id — topic with sections, subtopics, resources
router.get('/:id', (req: Request, res: Response) => {
  const db = getDb();
  const id = parseInt(req.params.id);

  const topic = db.prepare('SELECT * FROM topics WHERE id = ?').get(id) as {
    id: number; number: number; title: string; icon: string; color: string;
  } | undefined;

  if (!topic) {
    res.status(404).json({ error: 'Topic not found' });
    return;
  }

  const sections = db.prepare('SELECT * FROM sections WHERE topic_id = ? ORDER BY section_order').all(id) as Array<{
    id: number; topic_id: number; title: string; section_order: number;
  }>;

  const result = {
    ...topic,
    sections: sections.map(section => {
      const subtopics = db.prepare('SELECT * FROM subtopics WHERE section_id = ? ORDER BY id').all(section.id);
      const resources = db.prepare('SELECT * FROM resources WHERE section_id = ? ORDER BY id').all(section.id);
      const total = (subtopics as Array<{ completed: number }>).length;
      const completed = (subtopics as Array<{ completed: number }>).filter(s => s.completed).length;

      return {
        ...section,
        subtopics,
        resources,
        progress: total > 0 ? Math.round((completed / total) * 100) : 0,
      };
    }),
  };

  res.json(result);
});

export default router;
