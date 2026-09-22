/**
 * The `review.*` surface, over the axum bridge.
 *
 * One module per feature slice rather than more of `api.ts`: these two calls
 * are the comment aliases of the session record and share an error contract
 * nothing else needs. The record itself comes from `lib/diff-api.ts`.
 *
 * Every call goes through the generated client, not a private `fetch` wrapper,
 * so they inherit the 401 re-prompt and the `{"error":{message}}` unwrapping.
 * A local "throw on !ok" would put a raw JSON blob in a toast where the server
 * had already written a sentence, and would leave a remote client whose cookie
 * expired mid-review with an opaque failure and no way to sign back in.
 */
import { client, decode } from './api-client';
import type { components } from './api-schema';

type Schemas = components['schemas'];

/**
 * The `Content-Type` of a write that carries no body.
 *
 * One of these routes takes no request body, so the client sends none. The
 * header still has to ride along, because it is load-bearing beyond encoding:
 * it takes the request out of the CORS simple-request set, forcing a preflight
 * the server's allowlist refuses. Dropping it makes every review write
 * something a foreign page can fire blind at a logged-in user.
 */
const preflighted = { headers: { 'Content-Type': 'application/json' } };

/**
 * A comment to add.
 *
 * `line_start` is 1-based and `line_end` is 1-based exclusive, defaulting
 * server-side to `line_start + 1`. `path` is absolute, or relative to the
 * session's single tracked root.
 */
export type NewComment = Schemas['CommentRequest'];

export async function addReviewComment(
  sessionId: string,
  comment: NewComment,
): Promise<Schemas['ReviewCommentResponse']> {
  return decode(
    await client.POST('/api/session/{id}/review/comment', {
      params: { path: { id: sessionId } },
      // The caller's object goes whole, so a `line_end`/`root`/`author` it
      // omitted stays omitted on the wire. `JSON.stringify` drops `undefined`
      // properties, and the daemon's own defaults only apply to a field that
      // is ABSENT — an explicit null would defeat them.
      body: comment,
    }),
    'Failed to comment',
  );
}

export async function resolveReviewComment(
  sessionId: string,
  commentId: string,
): Promise<Schemas['ReviewResolveCommentResponse']> {
  return decode(
    await client.POST('/api/session/{id}/review/comment/{comment_id}/resolve', {
      params: { path: { id: sessionId, comment_id: commentId } },
      // `preflighted` is not redundant and must not be "optimised" away. See
      // its declaration: without the header, `POST …/resolve` — and by the
      // same argument every review write — is something a foreign page can
      // fire blind at a logged-in user.
      ...preflighted,
    }),
    'Failed to resolve comment',
  );
}
