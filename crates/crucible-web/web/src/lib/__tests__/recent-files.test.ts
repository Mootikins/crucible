import { describe, it, expect, beforeEach } from 'vitest';
import { recentFiles, recordRecentFile } from '../recent-files';

describe('recent-files', () => {
  // The store is a module-level signal (no reset API) — tests below use
  // saturating writes / relative assertions so shared state can't skew them.
  beforeEach(() => {
    localStorage.clear();
  });

  it('records most-recent-first, dedupes by path, and caps the ring', () => {
    for (let i = 0; i < 12; i++) {
      recordRecentFile(`/k/f${i}.md`, `f${i}.md`);
    }
    // Capped at 8, newest first.
    expect(recentFiles().length).toBe(8);
    expect(recentFiles()[0].absPath).toBe('/k/f11.md');

    // Re-opening an older file moves it to the front without duplication.
    recordRecentFile('/k/f5.md', 'f5.md');
    expect(recentFiles()[0].absPath).toBe('/k/f5.md');
    expect(recentFiles().filter((r) => r.absPath === '/k/f5.md')).toHaveLength(1);
    expect(recentFiles().length).toBe(8);
  });

  it('persists to localStorage', () => {
    recordRecentFile('/k/persisted.md', 'persisted.md');
    const raw = JSON.parse(localStorage.getItem('crucible:recentFiles') ?? '[]');
    expect(raw[0]).toEqual({ absPath: '/k/persisted.md', name: 'persisted.md' });
  });
});
