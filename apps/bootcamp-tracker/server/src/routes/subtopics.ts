import { Router, Request, Response } from 'express';
import { getDb } from '../db';

const router = Router();

// PATCH /api/subtopics/:id
router.patch('/:id', (req: Request, res: Response) => {
  const db = getDb();
  const id = parseInt(req.params.id);
  const { completed, notes } = req.body as { completed?: boolean; notes?: string };

  const subtopic = db.prepare('SELECT * FROM subtopics WHERE id = ?').get(id);
  if (!subtopic) {
    res.status(404).json({ error: 'Subtopic not found' });
    return;
  }

  const updates: string[] = [];
  const params: (string | number | null)[] = [];

  if (completed !== undefined) {
    updates.push('completed = ?');
    params.push(completed ? 1 : 0);
    updates.push('completed_at = ?');
    params.push(completed ? new Date().toISOString() : null);
  }

  if (notes !== undefined) {
    updates.push('notes = ?');
    params.push(notes);
  }

  if (updates.length === 0) {
    res.status(400).json({ error: 'No fields to update' });
    return;
  }

  params.push(id);
  db.prepare(`UPDATE subtopics SET ${updates.join(', ')} WHERE id = ?`).run(...params);

  const updated = db.prepare('SELECT * FROM subtopics WHERE id = ?').get(id);
  res.json(updated);
});

export default router;
