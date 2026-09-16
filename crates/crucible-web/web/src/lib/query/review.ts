import type { Accessor } from 'solid-js';
import {
  useMutation,
  useQuery,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import {
  addReviewComment,
  listReviewHunks,
  rebaseReview,
  resolveReviewComment,
  setHunkState,
  setHunkStates,
  undoReject,
  type BulkOutcome,
  type DegradedRoot,
  type NewComment,
  type ReviewHunksResponse,
} from '@/lib/review-api';
import type { ReviewComment, ReviewScope, ReviewState } from '@/lib/review-types';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The composed diff of a session, held under that session.
 *
 * Three surfaces read one listing: the changes panel in the right region, the
 * gutter of whatever file is open in the centre, and every tool card in the
 * transcript. The key is what lets one answer serve all three, and what lets
 * the daemon's own event reach them — the session stream says `review_changed`
 * and the route of that stream invalidates this entry.
 *
 * Every write invalidates rather than patches. The daemon decides what a
 * decision did: a reject reverts lines on disk and can renumber every hunk
 * after it, and a bulk decision reports which ids it refused, so "what is
 * left" is the daemon's answer and not something the browser can compute from
 * the reply. The optimistic marks that make a click feel immediate live in
 * `lib/review-store.ts`, over the slot, and this answer corrects them.
 *
 * The scope is ASKED, not keyed. The slot is the listing: one array per
 * session, read by all three surfaces, so a second array per scope would be a
 * second listing of the same session. Switching scope re-asks under the same
 * key with the other word.
 */

/** The options of one session's listing, under the scope its reader is on. */
function hunksOptions(sessionId: string, scope: ReviewScope) {
  return {
    queryKey: keys.review(sessionId),
    queryFn: () => listReviewHunks(sessionId, scope),
  };
}

/** One session's composed diff, under one scope. */
export function useReviewHunks(
  sessionId: Accessor<string | null>,
  scope: Accessor<ReviewScope>,
): UseQueryResult<ReviewHunksResponse, Error> {
  return useQuery(() => {
    const id = sessionId();
    return { ...hunksOptions(id ?? '', scope()), enabled: id !== null };
  }, getQueryClient);
}

/**
 * Makes one session's listing wrong. Every write, the scope switch and the
 * stream's route call it.
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
  // `finally`, not `then`: a refused decision changes the listing too. A bulk
  // reject that fails halfway has reverted the files it got to, and a listing
  // left as it was would show them unreviewed.
  return run().finally(() => invalidateReview(sessionId));
}

/** Accept, reject (which reverts and tells the agent), or return to the queue. */
export function setHunkStateOnce(
  sessionId: string,
  hunkId: string,
  state: ReviewState,
): Promise<{ hunk_id: string; state: ReviewState }> {
  return writing(sessionId, () => setHunkState(sessionId, hunkId, state));
}

/** One decision over several hunks, as ONE daemon call, in the order given. */
export function setHunkStatesOnce(
  sessionId: string,
  hunkIds: string[],
  state: ReviewState,
): Promise<BulkOutcome & { state: ReviewState }> {
  return writing(sessionId, () => setHunkStates(sessionId, hunkIds, state));
}

/** Take back the most recent reject, single or bulk, as one action. */
export function undoRejectOnce(sessionId: string): Promise<BulkOutcome> {
  return writing(sessionId, () => undoReject(sessionId));
}

/** Accept the worktree as the new base, releasing a block reviewing cannot. */
export function rebaseReviewOnce(sessionId: string): Promise<{ roots: DegradedRoot[] }> {
  return writing(sessionId, () => rebaseReview(sessionId));
}

/** Leave a comment on a range of one file. */
export function addReviewCommentOnce(
  sessionId: string,
  comment: NewComment,
): Promise<{ comment: ReviewComment }> {
  return writing(sessionId, () => addReviewComment(sessionId, comment));
}

/** Mark one comment resolved. */
export function resolveReviewCommentOnce(
  sessionId: string,
  commentId: string,
): Promise<{ comment_id: string }> {
  return writing(sessionId, () => resolveReviewComment(sessionId, commentId));
}

/**
 * The same six writes, for a caller that wants their pending and error state.
 *
 * `lib/review-store.ts` reaches the plain functions above instead, because its
 * actions are called from three surfaces that do not share an owner and one of
 * them acts as it goes away.
 */
export function useSetHunkState(
  sessionId: Accessor<string>,
): UseMutationResult<{ hunk_id: string; state: ReviewState }, Error, { hunkId: string; state: ReviewState }> {
  return useMutation(
    () => ({
      mutationFn: ({ hunkId, state }: { hunkId: string; state: ReviewState }) =>
        setHunkStateOnce(sessionId(), hunkId, state),
    }),
    getQueryClient,
  );
}

export function useSetHunkStates(
  sessionId: Accessor<string>,
): UseMutationResult<BulkOutcome & { state: ReviewState }, Error, { hunkIds: string[]; state: ReviewState }> {
  return useMutation(
    () => ({
      mutationFn: ({ hunkIds, state }: { hunkIds: string[]; state: ReviewState }) =>
        setHunkStatesOnce(sessionId(), hunkIds, state),
    }),
    getQueryClient,
  );
}

export function useUndoReject(
  sessionId: Accessor<string>,
): UseMutationResult<BulkOutcome, Error, void> {
  return useMutation(
    () => ({ mutationFn: () => undoRejectOnce(sessionId()) }),
    getQueryClient,
  );
}

export function useRebaseReview(
  sessionId: Accessor<string>,
): UseMutationResult<{ roots: DegradedRoot[] }, Error, void> {
  return useMutation(
    () => ({ mutationFn: () => rebaseReviewOnce(sessionId()) }),
    getQueryClient,
  );
}

export function useAddReviewComment(
  sessionId: Accessor<string>,
): UseMutationResult<{ comment: ReviewComment }, Error, NewComment> {
  return useMutation(
    () => ({ mutationFn: (comment: NewComment) => addReviewCommentOnce(sessionId(), comment) }),
    getQueryClient,
  );
}

export function useResolveReviewComment(
  sessionId: Accessor<string>,
): UseMutationResult<{ comment_id: string }, Error, string> {
  return useMutation(
    () => ({ mutationFn: (commentId: string) => resolveReviewCommentOnce(sessionId(), commentId) }),
    getQueryClient,
  );
}
