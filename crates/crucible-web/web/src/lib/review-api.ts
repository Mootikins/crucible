/**
 * The comment calls of the Changes panel, on the session record.
 *
 * A comment belongs to a diffset. The panel names the session, so this
 * module turns the session id into the session record source and calls
 * `lib/diff-api.ts`. The web server has no route for each session any
 * longer.
 */
import { resolveDiffComment } from './diff-api';

/** Marks one comment of the session record of `sessionId` resolved. */
export async function resolveReviewComment(
  sessionId: string,
  commentId: string,
): Promise<{ comment_id: string }> {
  return resolveDiffComment({ kind: 'session_record', session: sessionId }, commentId);
}
