import React from 'react';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Sidebar } from './components/layout/Sidebar';
import { Header } from './components/layout/Header';
import { ReminderToast } from './components/reminders/ReminderToast';
import { DashboardPage } from './pages/DashboardPage';
import { TopicPage } from './pages/TopicPage';
import { RoadmapPage } from './pages/RoadmapPage';
import { RemindersPage } from './pages/RemindersPage';
import { SessionsPage } from './pages/SessionsPage';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <div className="flex h-screen overflow-hidden" style={{ backgroundColor: 'var(--bg-primary)' }}>
          <Sidebar />
          <div className="flex-1 flex flex-col overflow-hidden">
            <Header />
            <main className="flex-1 overflow-y-auto px-6 py-6">
              <Routes>
                <Route path="/" element={<DashboardPage />} />
                <Route path="/topics/:id" element={<TopicPage />} />
                <Route path="/roadmap" element={<RoadmapPage />} />
                <Route path="/reminders" element={<RemindersPage />} />
                <Route path="/sessions" element={<SessionsPage />} />
              </Routes>
            </main>
          </div>
        </div>
        <ReminderToast />
      </BrowserRouter>
    </QueryClientProvider>
  );
}
