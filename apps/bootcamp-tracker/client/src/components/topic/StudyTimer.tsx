import React, { useState, useEffect, useRef } from 'react';
import { useStartSession, useStopSession } from '../../hooks/useStudySessions';

interface StudyTimerProps {
  subtopicId: number;
  label: string;
}

function formatTime(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
}

export function StudyTimer({ subtopicId, label }: StudyTimerProps) {
  const [running, setRunning] = useState(false);
  const [seconds, setSeconds] = useState(0);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const startSession = useStartSession();
  const stopSession = useStopSession();

  useEffect(() => {
    if (running) {
      intervalRef.current = setInterval(() => {
        setSeconds(s => s + 1);
      }, 1000);
    } else {
      if (intervalRef.current) clearInterval(intervalRef.current);
    }
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [running]);

  const handleStart = async () => {
    try {
      const session = await startSession.mutateAsync(subtopicId) as { id: number };
      setSessionId(session.id);
      setSeconds(0);
      setRunning(true);
    } catch (e) {
      console.error('Failed to start session', e);
    }
  };

  const handleStop = async () => {
    setRunning(false);
    if (sessionId) {
      try {
        await stopSession.mutateAsync(sessionId);
      } catch (e) {
        console.error('Failed to stop session', e);
      }
      setSessionId(null);
    }
    setSeconds(0);
  };

  return (
    <div className="flex items-center gap-2">
      {running && (
        <span className="text-xs font-mono tabular-nums" style={{ color: 'var(--color-accent-blue)' }}>
          {formatTime(seconds)}
        </span>
      )}
      {!running ? (
        <button
          onClick={handleStart}
          disabled={startSession.isPending}
          className="flex items-center gap-1 px-2 py-1 rounded text-xs transition-colors hover:bg-white/10"
          style={{ color: 'var(--color-text-tertiary)', border: '1px solid var(--color-border-primary)' }}
          title={`Start timer for: ${label}`}
        >
          <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 15A7 7 0 108 1a7 7 0 000 14zm-1.5-9.5l5 3-5 3v-6z"/>
          </svg>
          Timer
        </button>
      ) : (
        <button
          onClick={handleStop}
          className="flex items-center gap-1 px-2 py-1 rounded text-xs transition-colors"
          style={{ color: 'var(--color-status-error)', border: '1px solid rgba(255, 59, 48, 0.3)', backgroundColor: 'rgba(255, 59, 48, 0.1)' }}
          title="Stop timer"
        >
          <svg width="10" height="10" viewBox="0 0 16 16" fill="currentColor">
            <rect x="4" y="4" width="8" height="8" rx="1" />
          </svg>
          Stop
        </button>
      )}
    </div>
  );
}
