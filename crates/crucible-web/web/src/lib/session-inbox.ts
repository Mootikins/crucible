import type { Session } from '@/lib/types';
import { sessionStatus, type SessionStatus } from '@/lib/session-status';

/** How long a session stays in the Inbox after its last message. */
export const INBOX_MAX_AGE_MS = 24 * 60 * 60 * 1000;

/**
 * The Inbox: sessions doing something, freshest first.
 *
 * Membership is "not idle AND touched in the last day". The staleness rule is
 * what keeps it an inbox rather than a second session list — an agent that has
 * been blocked on a question since last week is not news, and left in, it would
 * sit at the top forever. It stays reachable in the tree below.
 *
 * One definition, because both shells draw an Inbox and two copies would drift.
 * `statusOf` is a parameter so a test can state the status the attention store
 * would otherwise supply.
 */
export function inboxSessions(
  sessions: readonly Session[],
  now: number = Date.now(),
  statusOf: (session: Session) => SessionStatus = sessionStatus,
): Session[] {
  return sessions
    .filter((s) => {
      if (s.archived) return false;
      if (statusOf(s) === 'idle') return false;
      const touched = Date.parse(s.last_activity ?? s.started_at);
      return !Number.isNaN(touched) && now - touched < INBOX_MAX_AGE_MS;
    })
    .sort(
      (a, b) =>
        (Date.parse(b.last_activity ?? b.started_at) || 0) -
        (Date.parse(a.last_activity ?? a.started_at) || 0),
    );
}
