import { describe, it, expect } from 'vitest';
import { INBOX_SIZE, inboxSessions } from '@/lib/session-inbox';
import type { Session } from '@/lib/types';

const now = Date.parse('2026-09-11T12:00:00Z');
const session = (over: Partial<Session>): Session =>
  ({
    id: 'x',
    session_type: 'chat',
    started_at: new Date(now).toISOString(),
    last_activity: new Date(now).toISOString(),
    archived: false,
    kilns: [],
    ...over,
  }) as Session;

const minutesAgo = (m: number) => new Date(now - m * 60_000).toISOString();

describe('inboxSessions', () => {
  it('lists the sessions the user touched last, freshest first', () => {
    const older = session({ session_id: 'older', last_activity: minutesAgo(1) });
    const newer = session({ session_id: 'newer' });
    expect(inboxSessions([older, newer]).map((s) => s.session_id)).toEqual(['newer', 'older']);
  });

  // Recency alone decides. A session that does nothing is still the one the
  // user was in a minute ago, and that is what an inbox of "where was I"
  // must show.
  it('keeps an idle session', () => {
    expect(inboxSessions([session({ session_id: 'idle' })]).map((s) => s.session_id)).toEqual(['idle']);
  });

  it(`stops at ${INBOX_SIZE}`, () => {
    const many = Array.from({ length: INBOX_SIZE + 3 }, (_, i) =>
      session({ session_id: `s${i}`, last_activity: minutesAgo(i) }),
    );
    const got = inboxSessions(many).map((s) => s.session_id);
    expect(got).toHaveLength(INBOX_SIZE);
    expect(got[0]).toBe('s0');
    expect(got).not.toContain(`s${INBOX_SIZE}`);
  });

  it('leaves out an archived session', () => {
    expect(inboxSessions([session({ session_id: 'gone', archived: true })])).toEqual([]);
  });

  // A pass a plugin ran for itself has its own section; see
  // `session-reflections.ts`.
  it('leaves out a plugin session', () => {
    expect(inboxSessions([session({ session_id: 'pass', type: 'plugin' })])).toEqual([]);
  });

  it('puts a session whose timestamps make no sense last', () => {
    const broken = session({ session_id: 'broken', last_activity: 'not a date' });
    const fine = session({ session_id: 'fine', last_activity: minutesAgo(600) });
    expect(inboxSessions([broken, fine]).map((s) => s.session_id)).toEqual(['fine', 'broken']);
  });
});
