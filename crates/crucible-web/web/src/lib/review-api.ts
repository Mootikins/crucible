/**
 * The `review.*` surface, over the axum bridge.
 *
 * One module per feature slice rather than more of `api.ts`: these seven calls
 * are the whole attributed-diff review API and share an error contract nothing
 * else needs.
 *
 * Every call goes through `api.ts`'s `request`, not a private `fetch` wrapper,
 * so they inherit the 401 re-prompt and the `{"error":{message}}` unwrapping.
 * A local "throw on !ok" would put a raw JSON blob in a toast where the server
 * had already written a sentence, and would leave a remote client whose cookie
 * expired mid-review with an opaque failure and no way to sign back in.
 *
 * There is deliberately no `revertHunk`. Rejecting IS reverting — one daemon
 * operation, reached through `setHunkState(id, hunk, 'rejected')`. A second
 * spelling existed through eight layers, did nothing the first did not, and
 * cost the agent a duplicate tool description every turn.
 */
import { request } from './api';
import type { components } from './api-schema';
import type { ReviewScope, ReviewState } from './review-types';

type Schemas = components['schemas'];

/** Path prefix, matching the `session` route group's `modes`/`mode`/`status`. */
const base = (sessionId: string) => `/api/session/${encodeURIComponent(sessionId)}/review`;

/**
 * A JSON body, and the `Content-Type` that comes with it.
 *
 * The header is load-bearing beyond encoding: it takes the request out of the
 * CORS simple-request set, forcing a preflight the server's allowlist refuses.
 */
function jsonBody(body: unknown): { headers: Record<string, string>; body: string } {
  return {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  };
}

/** A root whose attribution the daemon can no longer vouch for. `degraded` is
 * null when the root is intact; the string is shown to the user verbatim. */
export type DegradedRoot = Schemas['ReviewRootRow'];

/** One journal record the daemon could not read back. `line` is 1-based in
 * `review.jsonl`. */
export type IntegritySkip = Schemas['ReviewSkipRow'];

/**
 * The list response.
 *
 * `scope` is the scope the daemon answered under — a store that switched scope
 * while this listing was in flight uses it to drop the stale answer. `gate` is
 * non-null only while a turn is parked on the review gate, and carries the
 * tool and the path and nothing else: a gate that is present IS a blocked
 * turn, so a separate `blocked` flag said the same thing twice.
 */
export type ReviewHunksResponse = Schemas['ReviewHunksResponse'];

/**
 * The composed diff under one scope. Always named on the wire, so the answer
 * and the question agree without a default living on two sides.
 */
export function listReviewHunks(
  sessionId: string,
  scope: ReviewScope = 'session',
): Promise<ReviewHunksResponse> {
  return request('GET', `${base(sessionId)}/hunks?scope=${scope}`, {
    errorMessage: 'Failed to load review',
    includeErrorText: true,
  });
}

/** Accept, reject (which reverts and tells the agent), or return to the queue. */
export function setHunkState(
  sessionId: string,
  hunkId: string,
  state: ReviewState,
): Promise<Schemas['ReviewStateResponse']> {
  return request('POST', `${base(sessionId)}/state`, {
    errorMessage: 'Failed to record review decision',
    includeErrorText: true,
    ...jsonBody({ hunk_id: hunkId, state }),
  });
}

/**
 * What a bulk decision or an undo did.
 *
 * A refused hunk is part of the answer, not an error: `applied` names the ids
 * that landed, in order, and `failed` names the rest with the daemon's reason
 * for each. The caller shows `failed` and keeps the rest, rather than
 * re-listing to learn which was which.
 */
export type BulkOutcome = Schemas['ReviewUndoRejectResponse'];

/**
 * One decision over several hunks, as ONE daemon call.
 *
 * The ids go in the order given. The daemon applies them in that order, and a
 * reject reverts files as it goes, so the caller's order is the diff order.
 */
export function setHunkStates(
  sessionId: string,
  hunkIds: string[],
  state: ReviewState,
): Promise<Schemas['ReviewStatesResponse']> {
  return request('POST', `${base(sessionId)}/states`, {
    errorMessage: 'Failed to record review decision',
    includeErrorText: true,
    ...jsonBody({ hunk_ids: hunkIds, state }),
  });
}

/**
 * Take back the most recent reject, single or bulk, as one action.
 *
 * Names no hunk: the daemon owns the stack of rejects for the session and
 * pops it. An empty stack answers two empty lists. The `{}` body is the same
 * preflight rule as `rebaseReview` and `resolveReviewComment`.
 */
export function undoReject(sessionId: string): Promise<Schemas['ReviewUndoRejectResponse']> {
  return request('POST', `${base(sessionId)}/undo-reject`, {
    errorMessage: 'Failed to undo the reject',
    includeErrorText: true,
    ...jsonBody({}),
  });
}

/** The release for a degraded root; nothing else clears one. */
export function rebaseReview(sessionId: string): Promise<Schemas['ReviewRebaseResponse']> {
  return request('POST', `${base(sessionId)}/rebase`, {
    errorMessage: 'Failed to rebase review',
    includeErrorText: true,
    ...jsonBody({}),
  });
}

/**
 * A comment to add.
 *
 * `line_start` is 1-based and `line_end` is 1-based exclusive, defaulting
 * server-side to `line_start + 1`. `path` is absolute, or relative to the
 * session's single tracked root.
 */
export type NewComment = Schemas['CommentRequest'];

export function addReviewComment(
  sessionId: string,
  comment: NewComment,
): Promise<Schemas['ReviewCommentResponse']> {
  return request('POST', `${base(sessionId)}/comment`, {
    errorMessage: 'Failed to comment',
    includeErrorText: true,
    // Object spread, so a `line_end`/`root`/`author` the caller omitted stays
    // omitted on the wire. `JSON.stringify` drops `undefined` properties, and
    // the daemon's own defaults only apply to a field that is ABSENT — an
    // explicit null would defeat them.
    ...jsonBody(comment),
  });
}

export function resolveReviewComment(
  sessionId: string,
  commentId: string,
): Promise<Schemas['ReviewResolveCommentResponse']> {
  return request('POST', `${base(sessionId)}/comment/${encodeURIComponent(commentId)}/resolve`, {
    errorMessage: 'Failed to resolve comment',
    includeErrorText: true,
    // The `{}` body is not redundant and must not be "optimised" away. It is
    // what puts `Content-Type: application/json` on the request, which takes
    // it out of the CORS simple-request set and forces a preflight the
    // server's allowlist refuses. Without it, `POST …/resolve` — and by the
    // same argument every review write — is something a foreign page can fire
    // blind at a logged-in user.
    ...jsonBody({}),
  });
}
