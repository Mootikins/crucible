import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  addReviewComment,
  listReviewHunks,
  rebaseReview,
  resolveReviewComment,
  setHunkState,
} from '../review-api';

/**
 * These URLs and bodies ARE the contract with the axum layer in
 * `crucible-web/src/routes/session/` — the browser never speaks raw JSON-RPC,
 * so a route that does not match one of these is a review surface that cannot
 * be reached. Asserting them here is the only place that contract is written
 * down on this side.
 */
const fetchMock = vi.fn();

const ok = (body: unknown) =>
  fetchMock.mockResolvedValue({
    ok: true,
    json: async () => body,
    text: async () => '',
  });

beforeEach(() => {
  vi.stubGlobal('fetch', fetchMock);
});

const call = (n = 0) => fetchMock.mock.calls[n] as [string, RequestInit];
const bodyOf = (n = 0) => JSON.parse(call(n)[1].body as string);

describe('review REST surface', () => {
  it('lists hunks for a session', async () => {
    ok({ session_id: 's1', hunks: [], comments: [] });
    await listReviewHunks('s1');
    expect(call()[0]).toBe('/api/session/s1/review/hunks');
    expect(call()[1].method).toBe('GET');
  });

  it('encodes a session id with characters a path would eat', async () => {
    ok({ session_id: 'a/b', hunks: [], comments: [] });
    await listReviewHunks('a/b');
    expect(call()[0]).toBe('/api/session/a%2Fb/review/hunks');
  });

  it('sets state by hunk id', async () => {
    ok({ hunk_id: 'h1', state: 'accepted' });
    await setHunkState('s1', 'h1', 'accepted');
    expect(call()[0]).toBe('/api/session/s1/review/state');
    expect(bodyOf()).toEqual({ hunk_id: 'h1', state: 'accepted' });
  });

  it('comments on a range, passing only what the caller gave', async () => {
    ok({ comment: {} });
    await addReviewComment('s1', {
      path: 'src/a.rs',
      line_start: 3,
      body: 'why',
    });
    expect(call()[0]).toBe('/api/session/s1/review/comment');
    // `line_end` is deliberately absent: the daemon defaults it to
    // `line_start + 1`, and sending a guess here would fight that.
    expect(bodyOf()).toEqual({ path: 'src/a.rs', line_start: 3, body: 'why' });
  });

  // The only release for a degraded root — reviewing cannot clear one, because
  // it contributes no hunks. The `{}` body is what carries
  // `Content-Type: application/json` and forces the preflight the CORS
  // allowlist refuses; dropping it makes the rebase fireable cross-origin.
  it('rebases the session base, with the body that forces a preflight', async () => {
    ok({ roots: [] });
    await rebaseReview('s1');
    expect(call()[0]).toBe('/api/session/s1/review/rebase');
    expect(call()[1].method).toBe('POST');
    expect(bodyOf()).toEqual({});
    expect((call()[1].headers as Record<string, string>)['Content-Type']).toBe('application/json');
  });

  it('resolves a comment by id', async () => {
    ok({ comment_id: 'c 1' });
    await resolveReviewComment('s1', 'c 1');
    expect(call()[0]).toBe('/api/session/s1/review/comment/c%201/resolve');
  });

  it('surfaces the daemon message on failure, not a bare status', async () => {
    // The INVALID_PARAMS cases — unknown hunk, stale hunk, external hunk — all
    // mean "re-list and try again", and the body says which.
    fetchMock.mockResolvedValue({
      ok: false,
      status: 400,
      text: async () => 'hunk no longer exists',
    });
    await expect(listReviewHunks('s1')).rejects.toThrow('hunk no longer exists');
  });

  it('unwraps the error envelope instead of throwing the JSON at the user', async () => {
    // Every crucible-web route serializes a WebError as
    // `{"error": {code, message}}`. Throwing the body raw puts that blob in a
    // toast where the server had already written a sentence.
    fetchMock.mockResolvedValue({
      ok: false,
      status: 422,
      text: async () =>
        JSON.stringify({
          error: { code: 422, message: 'Hunk no longer exists' },
        }),
    });
    const error = await setHunkState('s1', 'h1', 'rejected').then(
      () => null,
      (e: Error) => e,
    );
    expect(error?.message).toContain('Hunk no longer exists');
    expect(error?.message).not.toContain('{');
  });

  it('an expired cookie re-prompts for the key instead of dying silently', async () => {
    // Review is the one surface a user sits on for minutes without navigating,
    // so it is where a session cookie is most likely to expire mid-use. With
    // no 401 branch the write just failed and nothing ever asked them to sign
    // back in.
    const prompted = vi.fn();
    window.addEventListener('crucible:auth-required', prompted);
    fetchMock.mockResolvedValue({
      ok: false,
      status: 401,
      text: async () => '',
    });

    await expect(setHunkState('s1', 'h1', 'accepted')).rejects.toThrow();

    expect(prompted).toHaveBeenCalled();
    window.removeEventListener('crucible:auth-required', prompted);
  });

  it('falls back to the status when the body is empty', async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 500,
      text: async () => '',
    });
    await expect(setHunkState('s1', 'h1', 'rejected')).rejects.toThrow('HTTP 500');
  });
});
