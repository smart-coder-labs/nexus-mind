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
      <div className="flex-1 relative max-w-lg">
        <SearchInput value={query} onChange={setQuery} isLoading={loading}>
          <SearchInput.Input
            value={query}
            onChange={setQuery}
            isLoading={loading}
            placeholder="Search topics, subtopics..."
            onClear={clear}
          />
          <SearchInput.Dropdown
            show={showResults}
            hasResults={!!hasResults}
            query={query}
          >
            {results && results.subtopics.length > 0 && (
              <SearchInput.Section title="Subtopics">
                {results.subtopics.map((s) => (
                  <SearchInput.Item key={s.id} onClick={() => handleResultClick(s.topic_id)}>
                    <SearchInput.ItemIcon type={s.topic_icon as any} />
                    <SearchInput.ItemContent
                      label={s.label}
                      subtitle={`${s.topic_title} · ${s.section_title}`}
                    />
                    <SearchInput.TrailingBadge variant="warning">
                      {s.priority}
                    </SearchInput.TrailingBadge>
                  </SearchInput.Item>
                ))}
              </SearchInput.Section>
            )}

            {results && results.resources.length > 0 && (
              <SearchInput.Section title="Resources">
                {results.resources.map((r) => (
                  <SearchInput.Item key={r.id} onClick={r.url ? () => window.open(r.url!, '_blank') : undefined}>
                    <SearchInput.ItemIcon type={r.type as any} />
                    <SearchInput.ItemContent
                      label={r.label}
                      subtitle={`${r.topic_title} · ${r.section_title}`}
                    />
                  </SearchInput.Item>
                ))}
              </SearchInput.Section>
            )}
          </SearchInput.Dropdown>
        </SearchInput>
      </div>

      <ThemeToggle variant='inline' />
    </header>
  );
}
