import { Router, Request, Response } from 'express';
import { getDb } from '../db';

const router = Router();

// GET /api/roadmap
router.get('/', (_req: Request, res: Response) => {
  const db = getDb();
  const days = db.prepare('SELECT * FROM roadmap_days ORDER BY week, day').all() as Array<{
    id: number; week: number; day: number; title: string; blocks: string; completed: number;
  }>;

  const parsed = days.map(d => ({
    ...d,
    blocks: JSON.parse(d.blocks) as { time: string; activities: string[] }[],
  }));

  // Group by week
  const weeks: Record<number, typeof parsed> = {};
  for (const day of parsed) {
    if (!weeks[day.week]) weeks[day.week] = [];
    weeks[day.week].push(day);
  }

  res.json({ weeks, days: parsed });
});

// PATCH /api/roadmap-days/:id
router.patch('/:id', (req: Request, res: Response) => {
  const db = getDb();
  const id = parseInt(req.params.id);
  const { completed } = req.body as { completed: boolean };

  const day = db.prepare('SELECT * FROM roadmap_days WHERE id = ?').get(id);
  if (!day) {
    res.status(404).json({ error: 'Roadmap day not found' });
    return;
  }

  db.prepare('UPDATE roadmap_days SET completed = ? WHERE id = ?').run(completed ? 1 : 0, id);
  const updated = db.prepare('SELECT * FROM roadmap_days WHERE id = ?').get(id) as {
    id: number; week: number; day: number; title: string; blocks: string; completed: number;
  };

  res.json({ ...updated, blocks: JSON.parse(updated.blocks) });
});

export default router;
