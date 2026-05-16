import { Router, Request, Response } from 'express';
import { getDb } from '../db';

const router = Router();

// GET /api/reminders
router.get('/', (_req: Request, res: Response) => {
  const db = getDb();
  const now = new Date().toISOString();
  const reminders = db.prepare(`
    SELECT r.*, s.label as subtopic_label
    FROM reminders r
    LEFT JOIN subtopics s ON r.subtopic_id = s.id
    WHERE r.dismissed = 0
    ORDER BY r.remind_at ASC
  `).all();
  res.json(reminders);
});

// POST /api/reminders
router.post('/', (req: Request, res: Response) => {
  const db = getDb();
  const { subtopic_id, roadmap_day_id, remind_at, message } = req.body as {
    subtopic_id?: number;
    roadmap_day_id?: number;
    remind_at: string;
    message: string;
  };

  if (!remind_at || !message) {
    res.status(400).json({ error: 'remind_at and message are required' });
    return;
  }

  const result = db.prepare(
    'INSERT INTO reminders (subtopic_id, roadmap_day_id, remind_at, message) VALUES (?, ?, ?, ?)'
  ).run(subtopic_id ?? null, roadmap_day_id ?? null, remind_at, message);

  const created = db.prepare('SELECT * FROM reminders WHERE id = ?').get(result.lastInsertRowid);
  res.status(201).json(created);
});

// PATCH /api/reminders/:id
router.patch('/:id', (req: Request, res: Response) => {
  const db = getDb();
  const id = parseInt(req.params.id);
  const { dismissed, snoozed_until } = req.body as { dismissed?: boolean; snoozed_until?: string };

  const reminder = db.prepare('SELECT * FROM reminders WHERE id = ?').get(id);
  if (!reminder) {
    res.status(404).json({ error: 'Reminder not found' });
    return;
  }

  const updates: string[] = [];
  const params: (string | number | null)[] = [];

  if (dismissed !== undefined) {
    updates.push('dismissed = ?');
    params.push(dismissed ? 1 : 0);
  }

  if (snoozed_until !== undefined) {
    updates.push('snoozed_until = ?');
    params.push(snoozed_until);
  }

  if (updates.length === 0) {
    res.status(400).json({ error: 'No fields to update' });
    return;
  }

  params.push(id);
  db.prepare(`UPDATE reminders SET ${updates.join(', ')} WHERE id = ?`).run(...params);

  const updated = db.prepare('SELECT * FROM reminders WHERE id = ?').get(id);
  res.json(updated);
});

// DELETE /api/reminders/:id
router.delete('/:id', (req: Request, res: Response) => {
  const db = getDb();
  const id = parseInt(req.params.id);

  const reminder = db.prepare('SELECT * FROM reminders WHERE id = ?').get(id);
  if (!reminder) {
    res.status(404).json({ error: 'Reminder not found' });
    return;
  }

  db.prepare('DELETE FROM reminders WHERE id = ?').run(id);
  res.status(204).send();
});

export default router;
