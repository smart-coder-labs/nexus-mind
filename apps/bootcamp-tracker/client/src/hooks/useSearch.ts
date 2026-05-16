import { useState, useEffect, useCallback } from 'react';
import { api } from '../api/client';
import type { SearchResult } from '../types';

export function useSearch() {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [debouncedQuery, setDebouncedQuery] = useState('');

  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedQuery(query);
    }, 300);
    return () => clearTimeout(timer);
  }, [query]);

  useEffect(() => {
    if (!debouncedQuery || debouncedQuery.length < 2) {
      setResults(null);
      return;
    }

    setLoading(true);
    api.search(debouncedQuery)
      .then(data => setResults(data as SearchResult))
      .catch(() => setResults(null))
      .finally(() => setLoading(false));
  }, [debouncedQuery]);

  const clear = useCallback(() => {
    setQuery('');
    setResults(null);
    setDebouncedQuery('');
  }, []);

  return { query, setQuery, results, loading, clear };
}
