import { Router, Request, Response } from 'express';
import { getDb } from '../db';

const router = Router();

// GET /api/sessions
router.get('/', (_req: Request, res: Response) => {
  const db = getDb();
  const sessions = db.prepare(`
    SELECT ss.*, s.label as subtopic_label
    FROM study_sessions ss
    LEFT JOIN subtopics s ON ss.subtopic_id = s.id
    ORDER BY ss.started_at DESC
  `).all();
  res.json(sessions);
});

// POST /api/sessions — start session
router.post('/', (req: Request, res: Response) => {
  const db = getDb();
  const { subtopic_id } = req.body as { subtopic_id?: number };

  const now = new Date().toISOString();
  const result = db.prepare(
    'INSERT INTO study_sessions (subtopic_id, started_at) VALUES (?, ?)'
  ).run(subtopic_id ?? null, now);

  const created = db.prepare('SELECT * FROM study_sessions WHERE id = ?').get(result.lastInsertRowid);
  res.status(201).json(created);
});

// PATCH /api/sessions/:id — stop session or add notes
router.patch('/:id', (req: Request, res: Response) => {
  const db = getDb();
  const id = parseInt(req.params.id);
  const { notes, stop } = req.body as { notes?: string; stop?: boolean };

  const session = db.prepare('SELECT * FROM study_sessions WHERE id = ?').get(id) as {
    id: number; subtopic_id: number | null; started_at: string; ended_at: string | null; duration_minutes: number | null; notes: string | null;
  } | undefined;

  if (!session) {
    res.status(404).json({ error: 'Session not found' });
    return;
  }

  const updates: string[] = [];
  const params: (string | number | null)[] = [];

  if (stop && !session.ended_at) {
    const now = new Date();
    const started = new Date(session.started_at);
    const durationMinutes = (now.getTime() - started.getTime()) / 60000;

    updates.push('ended_at = ?');
    params.push(now.toISOString());
    updates.push('duration_minutes = ?');
    params.push(Math.round(durationMinutes * 100) / 100);
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
  db.prepare(`UPDATE study_sessions SET ${updates.join(', ')} WHERE id = ?`).run(...params);

  const updated = db.prepare('SELECT * FROM study_sessions WHERE id = ?').get(id);
  res.json(updated);
});

export default router;
