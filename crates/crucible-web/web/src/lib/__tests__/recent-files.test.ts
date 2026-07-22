import { describe, it, expect, beforeEach, vi } from 'vitest';

// Server round-trips are covered by the Rust /api/recents tests; here the
// module's local ring semantics are under test.
vi.mock('@/lib/api', () => ({
  fetchRecents: vi.fn().mockResolvedValue([]),
  recordRecent: vi.fn().mockResolvedValue(undefined),
}));

import { recentFiles, recordRecentFile } from '../recent-files';

describe('recent-files', () => {
  // The store is a module-level signal (no reset API) — tests below use
  // saturating writes / relative assertions so shared state can't skew them.
  beforeEach(() => {
    localStorage.clear();
  });

  it('records most-recent-first, dedupes by path, and caps the ring', () => {
    for (let i = 0; i < 25; i++) {
      recordRecentFile(`/k/f${i}.md`, `f${i}.md`);
    }
    // Capped at 20 (matches the server-side MAX_RECENTS), newest first.
    expect(recentFiles().length).toBe(20);
    expect(recentFiles()[0].absPath).toBe('/k/f24.md');

    // Re-opening an older file moves it to the front without duplication.
    recordRecentFile('/k/f10.md', 'f10.md');
    expect(recentFiles()[0].absPath).toBe('/k/f10.md');
    expect(recentFiles().filter((r) => r.absPath === '/k/f10.md')).toHaveLength(1);
    expect(recentFiles().length).toBe(20);
  });

  it('persists to localStorage', () => {
    recordRecentFile('/k/persisted.md', 'persisted.md');
    const raw = JSON.parse(localStorage.getItem('crucible:recentFiles') ?? '[]');
    expect(raw[0]).toEqual({ absPath: '/k/persisted.md', name: 'persisted.md' });
  });
});
