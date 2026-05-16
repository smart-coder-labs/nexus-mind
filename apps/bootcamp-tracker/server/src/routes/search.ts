import { Router, Request, Response } from 'express';
import { getDb } from '../db';

const router = Router();

// GET /api/search?q=
router.get('/', (req: Request, res: Response) => {
  const db = getDb();
  const q = (req.query.q as string || '').trim();

  if (!q || q.length < 2) {
    res.json({ subtopics: [], resources: [] });
    return;
  }

  const pattern = `%${q}%`;

  const subtopics = db.prepare(`
    SELECT
      s.id,
      s.label,
      s.priority,
      s.estimated_time,
      s.completed,
      sec.title as section_title,
      sec.topic_id,
      t.title as topic_title,
      t.icon as topic_icon
    FROM subtopics s
    JOIN sections sec ON s.section_id = sec.id
    JOIN topics t ON sec.topic_id = t.id
    WHERE s.label LIKE ? OR s.notes LIKE ?
    ORDER BY s.priority ASC, t.number ASC
    LIMIT 30
  `).all(pattern, pattern);

  const resources = db.prepare(`
    SELECT
      r.id,
      r.type,
      r.label,
      r.url,
      sec.title as section_title,
      sec.topic_id,
      t.title as topic_title,
      t.icon as topic_icon
    FROM resources r
    JOIN sections sec ON r.section_id = sec.id
    JOIN topics t ON sec.topic_id = t.id
    WHERE r.label LIKE ? OR r.url LIKE ?
    LIMIT 20
  `).all(pattern, pattern);

  res.json({ subtopics, resources });
});

export default router;
