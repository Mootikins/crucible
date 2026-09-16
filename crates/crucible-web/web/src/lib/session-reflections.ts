import type { Session } from '@/lib/types';

/**
 * The sessions a plugin started for its own work, freshest first.
 *
 * A reflection or consolidation pass runs in a `plugin` session, writes its
 * notes through the note tools, and ends. Its edits land in that session's
 * review queue, and the daemon holds the session out of the archive while
 * anything in that queue is undecided — so the pass is reachable days later,
 * and the user needs a place to find it. Mixed into the project tree it is
 * invisible: a pass has no workspace, so it sorts under "No project" beside
 * every other session that has none.
 *
 * One definition, because both shells draw the section and two copies would
 * drift.
 */
export function reflectionSessions(sessions: readonly Session[]): Session[] {
  return sessions
    .filter((s) => !s.archived && s.type === 'plugin')
    .sort(
      (a, b) =>
        (Date.parse(b.last_activity ?? b.started_at ?? '') || 0) -
        (Date.parse(a.last_activity ?? a.started_at ?? '') || 0),
    );
}
