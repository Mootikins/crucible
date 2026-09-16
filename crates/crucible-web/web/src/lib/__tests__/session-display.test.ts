import { describe, it, expect } from 'vitest';
import { sessionDisplayTitle, sortByRecency } from '@/lib/session-display';
import type { Session } from '@/lib/types';

/**
 * The generated `SessionRow` makes `title`, `started_at` and `last_activity`
 * optional AND nullable, because the routes disagree about which of them they
 * send: the create route omits the title, every other route sends an explicit
 * null, and only `session.list` carries a last activity. The client used to
 * fold all of that into one shape through `mapSession`, so nothing here ever
 * met an absent field. It does now.
 */
const row = (over: Partial<Session> = {}): Session => ({
  session_id: 's-1',
  type: 'chat',
  kilns: [],
  workspace: null,
  state: 'active',
  ...over,
});

describe('sessionDisplayTitle', () => {
  it('names a session whose title is an explicit null', () => {
    expect(sessionDisplayTitle(row({ title: null, started_at: '2026-09-15T08:30:00Z' }))).toContain(
      'Untitled',
    );
  });

  it('names a session that carries no title field at all', () => {
    expect(sessionDisplayTitle(row({ started_at: '2026-09-15T08:30:00Z' }))).toContain('Untitled');
  });

  // A create reply carries neither a title nor a start, so the date fallback
  // has nothing to format. It must still answer a sentence, not `Invalid Date`.
  it('falls back to a plain phrase when there is no start either', () => {
    expect(sessionDisplayTitle(row())).toBe('Untitled session');
  });

  it('prefers the title the daemon sent', () => {
    expect(sessionDisplayTitle(row({ title: 'Trust work' }))).toBe('Trust work');
  });
});

describe('sortByRecency', () => {
  // `last_activity` is null on a session that has never run, and absent on one
  // read through a route that does not send it. Both sort by the start.
  it('sorts on the start when the last activity is null or absent', () => {
    const sorted = sortByRecency([
      row({ session_id: 'old', started_at: '2026-09-01T00:00:00Z', last_activity: null }),
      row({ session_id: 'new', started_at: '2026-09-14T00:00:00Z' }),
    ]);

    expect(sorted.map((s) => s.session_id)).toEqual(['new', 'old']);
  });

  it('puts a session with no times at all last', () => {
    const sorted = sortByRecency([
      row({ session_id: 'undated' }),
      row({ session_id: 'dated', started_at: '2026-09-14T00:00:00Z' }),
    ]);

    expect(sorted.map((s) => s.session_id)).toEqual(['dated', 'undated']);
  });
});
