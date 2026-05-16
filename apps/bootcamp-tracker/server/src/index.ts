import express from 'express';
import cors from 'cors';
import dotenv from 'dotenv';

import topicsRouter from './routes/topics';
import subtopicsRouter from './routes/subtopics';
import roadmapRouter from './routes/roadmap';
import remindersRouter from './routes/reminders';
import sessionsRouter from './routes/sessions';
import statsRouter from './routes/stats';
import searchRouter from './routes/search';

dotenv.config();

const app = express();
const PORT = process.env.PORT || 3001;

app.use(cors({ origin: ['http://localhost:5173', 'http://127.0.0.1:5173'] }));
app.use(express.json());

app.use('/api/topics', topicsRouter);
app.use('/api/subtopics', subtopicsRouter);
app.use('/api/roadmap', roadmapRouter);
app.use('/api/roadmap-days', roadmapRouter);
app.use('/api/reminders', remindersRouter);
app.use('/api/sessions', sessionsRouter);
app.use('/api/stats', statsRouter);
app.use('/api/search', searchRouter);

app.get('/api/health', (_req, res) => {
  res.json({ status: 'ok', timestamp: new Date().toISOString() });
});

app.listen(PORT, () => {
  console.log(`Bootcamp Tracker server running on http://localhost:${PORT}`);
});
