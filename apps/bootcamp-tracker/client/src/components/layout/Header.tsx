import React, { useRef, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { ThemeToggle } from '../ui/ThemeToggle';
import { useSearch } from '../../hooks/useSearch';
import { SearchInput } from '../ui/SearchInput';

export function Header() {
  const { query, setQuery, results, loading, clear } = useSearch();
  const inputRef = useRef<HTMLInputElement>(null);
  const navigate = useNavigate();
  const [showResults, setShowResults] = React.useState(false);

  // Keyboard shortcut: '/' to focus search
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === '/' && document.activeElement?.tagName !== 'INPUT' && document.activeElement?.tagName !== 'TEXTAREA') {
        e.preventDefault();
        inputRef.current?.focus();
      }
      if (e.key === 'Escape') {
        clear();
        inputRef.current?.blur();
        setShowResults(false);
      }
    };
    document.addEventListener('keydown', handleKey);
    return () => document.removeEventListener('keydown', handleKey);
  }, [clear]);

  useEffect(() => {
    setShowResults(!!results && query.length >= 2);
  }, [results, query]);

  const handleResultClick = (topicId: number) => {
    navigate(`/topics/${topicId}`);
    clear();
    setShowResults(false);
  };

  const hasResults = results && (results.subtopics.length > 0 || results.resources.length > 0);

  return (
    <header
      className="sticky top-0 z-40 flex justify-between gap-4 px-6 py-3 border-b"
      style={{
        backgroundColor: 'var(--color-bg-secondary)',
        borderColor: 'var(--color-border-primary)',
      }}
    >
      {/* Search */}
      <SearchInput />
      <div className="flex-1 relative max-w-lg">
        <div className="relative flex items-center">
          <svg
            className="absolute left-3 w-4 h-4 pointer-events-none"
            style={{ color: 'var(--color-text-tertiary)' }}
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path strokeLinecap="round" strokeLinejoin="round" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
          </svg>
          <input
            ref={inputRef}
            type="text"
            placeholder="Search subtopics and resources..."
            value={query}
            onChange={e => setQuery(e.target.value)}
            onFocus={() => setShowResults(!!results && query.length >= 2)}
            className="w-full pl-9 pr-16 py-2 rounded-lg text-sm border focus-ring transition-colors"
            style={{
              backgroundColor: 'var(--color-bg-tertiary)',
              borderColor: 'var(--color-border-primary)',
              color: 'var(--color-text-primary)',
            }}
          />
          <div className="absolute right-3 flex items-center gap-1">
            {loading && (
              <div className="w-3 h-3 rounded-full border border-transparent animate-spin"
                style={{ borderTopColor: 'var(--color-accent-blue)' }} />
            )}
            {!loading && !query && <kbd>/</kbd>}
            {query && (
              <button onClick={() => { clear(); setShowResults(false); }}
                style={{ color: 'var(--color-text-tertiary)' }}
                className="hover:text-white transition-colors">
                <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                  <path d="M3.72 3.72a.75.75 0 011.06 0L8 6.94l3.22-3.22a.75.75 0 111.06 1.06L9.06 8l3.22 3.22a.75.75 0 11-1.06 1.06L8 9.06l-3.22 3.22a.75.75 0 01-1.06-1.06L6.94 8 3.72 4.78a.75.75 0 010-1.06z" />
                </svg>
              </button>
            )}
          </div>
        </div>

        {/* Search results dropdown */}
        {showResults && (
          <div
            className="absolute top-full left-0 right-0 mt-1 rounded-lg border shadow-2xl z-50 overflow-hidden animate-fade-in"
            style={{
              backgroundColor: 'var(--color-bg-secondary)',
              borderColor: 'var(--color-border-primary)',
            }}
          >
            {!hasResults && (
              <div className="px-4 py-3 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                No results for "{query}"
              </div>
            )}
            {results && results.subtopics.length > 0 && (
              <div>
                <div className="px-3 py-1.5 text-xs font-medium uppercase tracking-wider"
                  style={{ color: 'var(--color-text-tertiary)', borderBottom: '1px solid var(--color-border-primary)' }}>
                  Subtopics
                </div>
                {results.subtopics.slice(0, 5).map(s => (
                  <button
                    key={s.id}
                    onClick={() => handleResultClick(s.topic_id)}
                    className="w-full flex items-center gap-3 px-3 py-2.5 hover:bg-white/5 text-left transition-colors"
                  >
                    <span className="text-base">{s.topic_icon}</span>
                    <div className="flex-1 min-w-0">
                      <div className="text-sm truncate" style={{ color: 'var(--color-text-primary)' }}>{s.label}</div>
                      <div className="text-xs truncate" style={{ color: 'var(--color-text-tertiary)' }}>
                        {s.topic_title} · {s.section_title}
                      </div>
                    </div>
                    <span className={`text-xs font-mono px-1.5 py-0.5 rounded ${
                      s.priority === 'P0' ? 'text-red-400 bg-red-900/30' :
                      s.priority === 'P1' ? 'text-yellow-400 bg-yellow-900/30' :
                      'text-gray-400 bg-gray-800/50'
                    }`}>{s.priority}</span>
                  </button>
                ))}
              </div>
            )}
            {results && results.resources.length > 0 && (
              <div>
                <div className="px-3 py-1.5 text-xs font-medium uppercase tracking-wider"
                  style={{ color: 'var(--color-text-tertiary)', borderTop: '1px solid var(--color-border-primary)', borderBottom: '1px solid var(--color-border-primary)' }}>
                  Resources
                </div>
                {results.resources.slice(0, 4).map(r => (
                  <div key={r.id} className="flex items-center gap-3 px-3 py-2.5">
                    <span className="text-base">
                      {r.type === 'paper' ? '📄' : r.type === 'book' ? '📚' : r.type === 'course' ? '🎓' : '💻'}
                    </span>
                    <div className="flex-1 min-w-0">
                      {r.url ? (
                        <a href={r.url} target="_blank" rel="noopener noreferrer"
                          className="text-sm hover:underline truncate block"
                          style={{ color: 'var(--color-accent-blue)' }}>
                          {r.label}
                        </a>
                      ) : (
                        <div className="text-sm truncate" style={{ color: 'var(--color-text-primary)' }}>{r.label}</div>
                      )}
                      <div className="text-xs truncate" style={{ color: 'var(--color-text-tertiary)' }}>
                        {r.topic_title} · {r.section_title}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      <ThemeToggle variant='inline' />
    </header>
  );
}
