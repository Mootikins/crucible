import type { Session } from './types';

// ── Session display helpers ──────────────────────────────────────────────
// Shared by Home (resume card) and the Inbox (session lists) so sorting
// and untitled-session naming stay consistent across surfaces.

type SessionTimes = Pick<Session, 'last_activity' | 'started_at'>;

/** Timestamp to sort/display sessions by — last event, falling back to start. */
export function sessionActivityTime(session: SessionTimes): number {
  const t = Date.parse(session.last_activity ?? session.started_at ?? '');
  return Number.isNaN(t) ? 0 : t;
}

/** Newest-activity-first copy of the given sessions. */
export function sortByRecency<T extends SessionTimes>(sessions: readonly T[]): T[] {
  return [...sessions].sort((a, b) => sessionActivityTime(b) - sessionActivityTime(a));
}

/**
 * Human title with a readable fallback for untitled sessions. The previous
 * fallback (`id.slice(0, 8)`) collapsed every `chat-YYYYMMDD…` id into the
 * same "Session chat-202" label.
 */
export function sessionDisplayTitle(
  session: Pick<Session, 'title' | 'started_at'>,
): string {
  if (session.title && session.title.trim() !== '') return session.title;
  const started = new Date(session.started_at);
  if (!Number.isNaN(started.getTime())) {
    const date = started.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
    const time = started.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
    return `Untitled · ${date} ${time}`;
  }
  return 'Untitled session';
}
