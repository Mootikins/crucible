import { describe, it, expect } from 'vitest';
import { INBOX_MAX_AGE_MS, inboxSessions } from '@/lib/session-inbox';
import type { Session } from '@/lib/types';
import type { SessionStatus } from '@/lib/session-status';

const now = Date.parse('2026-09-11T12:00:00Z');
const session = (over: Partial<Session>): Session =>
  ({
    id: 'x',
    started_at: new Date(now).toISOString(),
    last_activity: new Date(now).toISOString(),
    archived: false,
    kilns: [],
    ...over,
  }) as Session;

/** Status comes from the attention store in the app; a test states it. */
const statusOf = (busy: string[]) => (s: Session): SessionStatus =>
  busy.includes(s.id) ? 'working' : 'idle';

describe('inboxSessions', () => {
  it('keeps a session that is doing something', () => {
    const busy = session({ id: 'busy' });
    expect(inboxSessions([busy], now, statusOf(['busy'])).map((s) => s.id)).toEqual(['busy']);
  });

  it('leaves out an idle session', () => {
    expect(inboxSessions([session({ id: 'idle' })], now, statusOf([]))).toEqual([]);
  });

  // An agent blocked since last week is not news. It stays in the tree below.
  it('leaves out a session nobody has touched for a day', () => {
    const stale = session({
      id: 'stale',
      last_activity: new Date(now - INBOX_MAX_AGE_MS - 1000).toISOString(),
    });
    expect(inboxSessions([stale], now, statusOf(['stale']))).toEqual([]);
  });

  it('puts the freshest first', () => {
    const older = session({ id: 'older', last_activity: new Date(now - 60_000).toISOString() });
    const newer = session({ id: 'newer' });
    const got = inboxSessions([older, newer], now, statusOf(['older', 'newer']));
    expect(got.map((s) => s.id)).toEqual(['newer', 'older']);
  });

  it('ignores a session whose timestamps make no sense', () => {
    const broken = session({ id: 'broken', last_activity: 'not a date' });
    expect(inboxSessions([broken], now, statusOf(['broken']))).toEqual([]);
  });
});
