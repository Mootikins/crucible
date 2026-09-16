/**
 * What a session is doing, for the one question a reader actually has: does
 * this need me?
 *
 * Three values, not four. `idle` still conflates "finished cleanly" with
 * "abandoned"; a fourth (`needs-review`, for a session that ended with
 * uncommitted changes) needs a daemon-side signal that does not exist yet.
 */
import type { Session } from '@/lib/types';
import { attentionStore } from '@/stores/attentionStore';

export type SessionStatus = 'working' | 'waiting' | 'idle';

/**
 * Derive a session's status from what this client actually knows.
 *
 * `waiting` is GLOBAL. The attention store merges the daemon's
 * pending-interaction aggregate, so a session with no open tab still reports
 * it — which is what lets the titlebar badge count work in other projects.
 *
 * `working` is NOT global. Streaming is reported by mounted chat panels only,
 * so a session running under another client reads as `idle` here. That is a
 * gap in the session listing, not a defect in this function: no field on
 * `Session` says "a turn is in flight". Do not paper over it with
 * `state === 'active'` — that flag only means "not paused and not ended", so
 * every resumable session would render as busy and the dot would say nothing.
 *
 * `waiting` beats `working`. A session that streams AND blocks on a human is,
 * to that human, blocked.
 */
export function sessionStatus(session: Pick<Session, 'session_id'>): SessionStatus {
  const entry = attentionStore.get(session.session_id);
  if (entry?.pendingInteraction) return 'waiting';
  if (entry?.isStreaming) return 'working';
  return 'idle';
}

/**
 * Sort order for the switcher's Active group: what blocks you, then what runs.
 *
 * Active sorts by STATUS, never by recency-opened. The moment it sorts by
 * recency it becomes a second tab strip and stops being a state view.
 */
export const STATUS_RANK: Record<SessionStatus, number> = {
  waiting: 0,
  working: 1,
  idle: 2,
};
