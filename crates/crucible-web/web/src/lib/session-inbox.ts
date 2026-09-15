import type { Session } from '@/lib/types';

/** How many sessions the Inbox lists. */
export const INBOX_SIZE = 5;

/** Milliseconds since the epoch of a session's last message, 0 when unreadable. */
export const touchedAt = (s: Session): number => Date.parse(s.last_activity ?? s.started_at) || 0;

/** Newest first. */
export const byRecency = (a: Session, b: Session) => touchedAt(b) - touchedAt(a);

/**
 * The Inbox: the last few sessions the user touched, freshest first.
 *
 * Recency alone decides membership. The rule used to be "not idle AND touched
 * in the last day", which made the section a status filter: it was empty on
 * a quiet morning and it dropped the session the user was in a minute ago as
 * soon as the agent went idle. An inbox answers "where was I", so it lists
 * the newest `INBOX_SIZE` and leaves the rest to the project tree below,
 * which does not repeat them.
 *
 * Archived sessions and plugin passes have sections of their own. One
 * definition, because both shells draw an Inbox and two copies would drift.
 */
export function inboxSessions(sessions: readonly Session[]): Session[] {
  return sessions
    .filter((s) => !s.archived && s.session_type !== 'plugin')
    .sort(byRecency)
    .slice(0, INBOX_SIZE);
}
