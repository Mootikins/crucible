/**
 * The `review.*` surface, over the axum bridge.
 *
 * One module per feature slice rather than more of `api.ts`: these five calls
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
import type { CommentAuthor, ComposedHunk, ReviewComment, ReviewState } from './review-types';

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

/** A root whose attribution the daemon can no longer vouch for. */
export interface DegradedRoot {
  root: string;
  /** Why. `null`/absent is intact; the string is shown to the user verbatim. */
  degraded?: string | null;
}

/** One journal record the daemon could not read back. */
export interface IntegritySkip {
  record: { kind: 'session' } | { kind: 'root'; root: string } | { kind: 'informational' };
  /** 1-based line in `review.jsonl`. */
  line: number;
  reason: string;
}

/**
 * The list response.
 *
 * `degraded`, `integrity` and `gate` are optional because the daemon grows
 * them on its own schedule and the bridge forwards the object untyped — a key
 * this file has not heard of must not break the panel.
 */
export interface ReviewHunksResponse {
  session_id?: string;
  hunks: ComposedHunk[];
  comments: ReviewComment[];
  degraded?: DegradedRoot[];
  integrity?: { skips: IntegritySkip[] };
  /** Present and non-null only while a turn is parked on the review gate. */
  gate?: { blocked: boolean; tool: string; path: string | null } | null;
}

export function listReviewHunks(sessionId: string): Promise<ReviewHunksResponse> {
  return request('GET', `${base(sessionId)}/hunks`, {
    errorMessage: 'Failed to load review',
    includeErrorText: true,
  });
}

/** Accept, reject (which reverts and tells the agent), or return to the queue. */
export function setHunkState(
  sessionId: string,
  hunkId: string,
  state: ReviewState,
): Promise<{ hunk_id: string; state: ReviewState }> {
  return request('POST', `${base(sessionId)}/state`, {
    errorMessage: 'Failed to record review decision',
    includeErrorText: true,
    ...jsonBody({ hunk_id: hunkId, state }),
  });
}

/** The release for a degraded root; nothing else clears one. */
export function rebaseReview(sessionId: string): Promise<{ roots: DegradedRoot[] }> {
  return request('POST', `${base(sessionId)}/rebase`, {
    errorMessage: 'Failed to rebase review',
    includeErrorText: true,
    ...jsonBody({}),
  });
}

export interface NewComment {
  /** Absolute, or relative to the session's single tracked root. */
  path: string;
  /** 1-based. */
  line_start: number;
  /** 1-based, exclusive. Defaults server-side to `line_start + 1`. */
  line_end?: number;
  body: string;
  root?: string;
  author?: CommentAuthor;
}

export function addReviewComment(
  sessionId: string,
  comment: NewComment,
): Promise<{ comment: ReviewComment }> {
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
): Promise<{ comment_id: string }> {
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
