import type { Accessor } from 'solid-js';
import { useQuery, type UseQueryResult } from '@tanstack/solid-query';
import { getDiffComments, getDiffset, resolveDiffComment } from '@/lib/diff-api';
import type { DiffComment, DiffFileEntry, DiffsetSource, UnreadableRoot } from '@/lib/diffset';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The session record of a session, held under that session.
 *
 * The record is the session record diffset: each file that differs between
 * the session base and the disk. Its comments ride along, so one answer
 * serves the panel. The key is what lets the daemon's own event reach it —
 * the session stream says `review_changed` and the route of that stream
 * invalidates this entry.
 *
 * Every write invalidates rather than patches: the listing is the daemon's
 * answer and not something the browser can compute from the reply.
 */

/** The files of a session record and its comments. */
export interface SessionRecordListing {
  files: DiffFileEntry[];
  /** The roots that the record leaves out, because the daemon cannot read them. */
  unreadable_roots: UnreadableRoot[];
  comments: DiffComment[];
}

/** The session record source of a session. */
function sessionRecord(sessionId: string): DiffsetSource {
  return { kind: 'session_record', session: sessionId };
}

/** Reads the files and the comments of one session record. */
async function listSessionRecord(sessionId: string): Promise<SessionRecordListing> {
  const source = sessionRecord(sessionId);
  const [diffset, comments] = await Promise.all([getDiffset(source), getDiffComments(source)]);
  return {
    files: diffset.files,
    unreadable_roots: diffset.unreadable_roots,
    comments: comments.map((listed) => listed.comment),
  };
}

/** One session's record. */
export function useSessionRecord(
  sessionId: Accessor<string | null>,
): UseQueryResult<SessionRecordListing, Error> {
  return useQuery(() => {
    const id = sessionId();
    return {
      queryKey: keys.review(id ?? ''),
      queryFn: () => listSessionRecord(id ?? ''),
      enabled: id !== null,
    };
  }, getQueryClient);
}

/**
 * Makes one session's listing wrong. Every write and the stream's route call
 * it.
 *
 * A listing already in FLIGHT is joined rather than restarted. The daemon
 * acknowledged the write before this ran, so that listing is at least as new
 * as the change it describes.
 */
export function invalidateReview(sessionId: string): Promise<void> {
  return getQueryClient()
    .invalidateQueries({ queryKey: keys.review(sessionId) })
    .then(() => undefined);
}

/** Runs one write, then makes the listing it changed wrong. */
function writing<T>(sessionId: string, run: () => Promise<T>): Promise<T> {
  // `finally`, not `then`: a refused write can still leave the listing
  // stale, and a re-list costs one request.
  return run().finally(() => invalidateReview(sessionId));
}

/** Mark one comment resolved. */
export function resolveReviewCommentOnce(
  sessionId: string,
  commentId: string,
): Promise<{ comment_id: string }> {
  return writing(sessionId, () => resolveDiffComment(sessionRecord(sessionId), commentId));
}
