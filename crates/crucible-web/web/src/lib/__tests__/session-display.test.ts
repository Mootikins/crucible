import { describe, it, expect } from 'vitest';
import { sessionDisplayTitle, sortByRecency } from '@/lib/session-display';
import type { Session } from '@/lib/types';

/**
 * `Session` (the generated `SessionSummary`) makes `title` and
 * `last_activity` optional — a session may have neither — but `started_at`
 * and `event_count` are required: every session record has a start the
 * moment it exists, `session.create` included. `sessionDisplayTitle` and
 * `sortByRecency` still fall back defensively when a caller hands them a
 * malformed or legacy payload; the tests that exercise that path build one
 * with `as Session`, spelling out that the shape is deliberately wrong.
 */
const row = (over: Partial<Session> = {}): Session => ({
  session_id: 's-1',
  type: 'chat',
  kilns: [],
  workspace: null,
  state: 'active',
  started_at: '2026-09-15T08:30:00Z',
  event_count: 0,
  archived: false,
  ...over,
});

/** A session missing `started_at` — unreachable through the typed API, but
 * still the shape a stale cache entry or a malformed daemon reply could
 * carry. `sessionDisplayTitle` and `sortByRecency` must not throw on it. */
function malformed(over: Partial<Session> & { session_id: string }): Session {
  const { started_at: _drop, ...rest } = row(over);
  return rest as Session;
}

describe('sessionDisplayTitle', () => {
  it('names a session whose title is an explicit null', () => {
    expect(sessionDisplayTitle(row({ title: null }))).toContain('Untitled');
  });

  it('names a session that carries no title field at all', () => {
    expect(sessionDisplayTitle(row())).toContain('Untitled');
  });

  // A malformed payload with neither title nor start has nothing for the
  // date fallback to format. It must still answer a sentence, not `Invalid
  // Date`.
  it('falls back to a plain phrase when there is no start either', () => {
    expect(sessionDisplayTitle(malformed({ session_id: 's-1' }))).toBe('Untitled session');
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
      malformed({ session_id: 'undated' }),
      row({ session_id: 'dated', started_at: '2026-09-14T00:00:00Z' }),
    ]);

    expect(sorted.map((s) => s.session_id)).toEqual(['dated', 'undated']);
  });
});
