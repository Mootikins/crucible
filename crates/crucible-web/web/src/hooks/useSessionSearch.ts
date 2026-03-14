import { createSignal, onCleanup, Accessor } from 'solid-js';
import type { Session } from '@/lib/types';
import { searchSessions } from '@/lib/api';

interface UseSessionSearchOptions {
  selectedKiln: Accessor<string>;
  sessions: Accessor<Session[]>;
  sessionFilter: Accessor<'active' | 'all' | 'archived'>;
}

export function useSessionSearch(options: UseSessionSearchOptions) {
  const [searchQuery, setSearchQuery] = createSignal('');
  const [searchResults, setSearchResults] = createSignal<Session[]>([]);
  const [isSearching, setIsSearching] = createSignal(false);
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let searchInputRef: HTMLInputElement | undefined;

  const performSearch = async (query: string) => {
    if (!query.trim()) {
      setSearchResults([]);
      setIsSearching(false);
      return;
    }
    setIsSearching(true);
    try {
      const results = await searchSessions(query, options.selectedKiln() || undefined, 20);
      setSearchResults(results);
    } catch (err) {
      console.error('Session search failed:', err);
      setSearchResults([]);
    } finally {
      setIsSearching(false);
    }
  };

  const handleSearchInput = (value: string) => {
    setSearchQuery(value);
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => performSearch(value), 300);
  };

  const clearSearch = () => {
    setSearchQuery('');
    setSearchResults([]);
    setIsSearching(false);
    if (searchTimer) clearTimeout(searchTimer);
  };

  const focusSearchInput = () => {
    searchInputRef?.focus();
  };

  // Expose focus method via custom event for command palette wiring
  const onFocusSearch = () => focusSearchInput();
  window.addEventListener('crucible:focus-session-search', onFocusSearch);

  onCleanup(() => {
    if (searchTimer) clearTimeout(searchTimer);
    window.removeEventListener('crucible:focus-session-search', onFocusSearch);
  });

  const displayedSessions = () => {
    const base = searchQuery().trim() ? searchResults() : options.sessions();
    const filter = options.sessionFilter();
    if (filter === 'active') {
      return base.filter(s => !s.archived && s.state !== 'ended');
    }
    if (filter === 'archived') {
      return base.filter(s => s.archived === true);
    }
    return base; // 'all'
  };

  return {
    searchQuery,
    searchResults,
    isSearching,
    handleSearchInput,
    clearSearch,
    displayedSessions,
    setSearchInputRef: (el: HTMLInputElement) => { searchInputRef = el; },
  };
}
