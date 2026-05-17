import React, { useState } from 'react';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '../ui/Tabs';
import { WeekTimeline } from './WeekTimeline';
import { LoadingPage } from '../ui/LoadingSpinner';
import { useRoadmap } from '../../hooks/useRoadmap';

export function RoadmapView() {
  const { data, isLoading } = useRoadmap();
  const [activeWeek, setActiveWeek] = useState(1);

  if (isLoading) return <LoadingPage />;
  if (!data) return null;

  const weeks = Object.keys(data.weeks).map(Number).sort();

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-xl font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          🗺️ Study Roadmap
        </h1>
        <p className="text-sm mt-1" style={{ color: 'var(--color-text-secondary)' }}>
          4 weeks · 30h/week · 6 days/week · 5h/day
        </p>
      </div>

      <Tabs value={activeWeek.toString()} onValueChange={(v) => setActiveWeek(Number(v))}>
        <TabsList variant="segmented">
          {weeks.map(w => {
            const days = data.weeks[w] || [];
            const completed = days.filter(d => d.completed).length;
            return (
              <TabsTrigger key={w} value={w.toString()}>
                Week {w} <span className="text-xs ml-1">{completed}/{days.length}</span>
              </TabsTrigger>
            );
          })}
        </TabsList>
        {weeks.map(w => (
          <TabsContent key={w} value={w.toString()}>
            {data.weeks[w] && <WeekTimeline week={w} days={data.weeks[w]} />}
          </TabsContent>
        ))}
      </Tabs>
    </div>
  );
}
