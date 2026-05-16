import { Router, Request, Response } from 'express';
import { getDb } from '../db';

const router = Router();

// GET /api/stats
router.get('/', (_req: Request, res: Response) => {
  const db = getDb();

  const totals = db.prepare(`
    SELECT
      COUNT(*) as totalSubtopics,
      SUM(completed) as completedSubtopics,
      SUM(CASE WHEN priority = 'P0' AND completed = 0 THEN 1 ELSE 0 END) as pendingP0
    FROM subtopics
  `).get() as { totalSubtopics: number; completedSubtopics: number; pendingP0: number };

  // Estimate total hours from estimated_time field (e.g. "2h" -> 2)
  const allSubtopics = db.prepare('SELECT estimated_time, completed FROM subtopics').all() as Array<{
    estimated_time: string; completed: number;
  }>;

  let totalEstimatedHours = 0;
  let completedHours = 0;

  for (const s of allSubtopics) {
    const match = s.estimated_time.match(/(\d+(?:\.\d+)?)/);
    if (match) {
      const hours = parseFloat(match[1]);
      totalEstimatedHours += hours;
      if (s.completed) completedHours += hours;
    }
  }

  // Study streak: consecutive days (up to today) with at least one completed session
  const sessionDates = db.prepare(`
    SELECT DISTINCT DATE(started_at) as date
    FROM study_sessions
    WHERE ended_at IS NOT NULL
    ORDER BY date DESC
  `).all() as Array<{ date: string }>;

  let studyStreak = 0;
  if (sessionDates.length > 0) {
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    let checkDate = new Date(today);

    for (const { date } of sessionDates) {
      const d = new Date(date + 'T00:00:00');
      const diff = Math.round((checkDate.getTime() - d.getTime()) / 86400000);
      if (diff === 0 || diff === 1) {
        studyStreak++;
        checkDate = d;
      } else {
        break;
      }
    }
  }

  // Hours per day — last 30 days
  const thirtyDaysAgo = new Date();
  thirtyDaysAgo.setDate(thirtyDaysAgo.getDate() - 30);

  const hoursPerDay = db.prepare(`
    SELECT
      DATE(started_at) as date,
      ROUND(SUM(COALESCE(duration_minutes, 0)) / 60.0, 2) as hours
    FROM study_sessions
    WHERE started_at >= ? AND ended_at IS NOT NULL
    GROUP BY DATE(started_at)
    ORDER BY date ASC
  `).all(thirtyDaysAgo.toISOString()) as Array<{ date: string; hours: number }>;

  res.json({
    totalSubtopics: totals.totalSubtopics,
    completedSubtopics: totals.completedSubtopics || 0,
    totalEstimatedHours: Math.round(totalEstimatedHours * 10) / 10,
    completedHours: Math.round(completedHours * 10) / 10,
    pendingP0: totals.pendingP0 || 0,
    studyStreak,
    hoursPerDay,
  });
});

export default router;
