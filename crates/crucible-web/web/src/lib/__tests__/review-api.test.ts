import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createMockFetch, apiError } from '@/test-utils';
import { getBus } from '@/lib/bus';
import { resetAuthThrottleForTests } from '../api-client';
import { addReviewComment, listReviewHunks, resolveReviewComment } from '../review-api';

/**
 * These URLs and bodies ARE the contract with the axum layer in
 * `crucible-web/src/routes/session/` — the browser never speaks raw JSON-RPC,
 * so a route that does not match one of these is a review surface that cannot
 * be reached. Asserting them here is the only place that contract is written
 * down on this side.
 */

const originalFetch = global.fetch;

/** Answers one route with a body, and nothing else with a 404. */
const serve = (key: string, body: unknown) => {
  const mockFetch = createMockFetch({ [key]: { body } });
  global.fetch = mockFetch;
  return mockFetch;
};

beforeEach(() => {
  resetAuthThrottleForTests();
});

afterEach(() => {
  global.fetch = originalFetch;
});

describe('review REST surface', () => {
  it('lists hunks for a session', async () => {
    const mockFetch = serve('GET /api/session/s1/review/hunks', {
      session_id: 's1',
      hunks: [],
      comments: [],
    });
    await listReviewHunks('s1');

    const sent = await mockFetch.sent(0);
    expect(sent.path).toBe('/api/session/s1/review/hunks');
    expect(sent.query.get('scope')).toBe('session');
    expect(sent.method).toBe('GET');
  });

  // The scope is the daemon's decision to make; the browser only names it.
  it('the turn scope asks the daemon for the turn', async () => {
    const mockFetch = serve('GET /api/session/s1/review/hunks', {
      session_id: 's1',
      scope: 'turn',
      hunks: [],
      comments: [],
    });
    await listReviewHunks('s1', 'turn');

    expect((await mockFetch.sent(0)).query.get('scope')).toBe('turn');
  });

  it('encodes a session id with characters a path would eat', async () => {
    const mockFetch = serve('GET /api/session/a%2Fb/review/hunks', {
      session_id: 'a/b',
      hunks: [],
      comments: [],
    });
    await listReviewHunks('a/b');

    expect((await mockFetch.sent(0)).path).toBe('/api/session/a%2Fb/review/hunks');
  });

  it('comments on a range, passing only what the caller gave', async () => {
    const mockFetch = serve('POST /api/session/s1/review/comment', { comment: {} });
    await addReviewComment('s1', { path: 'src/a.rs', line_start: 3, body: 'why' });

    const sent = await mockFetch.sent(0);
    expect(sent.path).toBe('/api/session/s1/review/comment');
    // `line_end` is deliberately absent: the daemon defaults it to
    // `line_start + 1`, and sending a guess here would fight that.
    expect(sent.body).toEqual({ path: 'src/a.rs', line_start: 3, body: 'why' });
  });

  it('resolves a comment by id', async () => {
    const mockFetch = serve('POST /api/session/s1/review/comment/c%201/resolve', {
      comment_id: 'c 1',
    });
    await resolveReviewComment('s1', 'c 1');

    const sent = await mockFetch.sent(0);
    expect(sent.path).toBe('/api/session/s1/review/comment/c%201/resolve');
    expect(sent.headers.get('Content-Type')).toBe('application/json');
  });

  it('surfaces the daemon message on failure, not a bare status', async () => {
    // An INVALID_PARAMS case means "re-list and try again", and the body says
    // why.
    global.fetch = createMockFetch({
      'GET /api/session/s1/review/hunks': { status: 400, body: 'no such session' },
    });
    await expect(listReviewHunks('s1')).rejects.toThrow('no such session');
  });

  it('unwraps the error envelope instead of throwing the JSON at the user', async () => {
    // Every crucible-web route serializes a WebError as
    // `{"error": {code, message}}`. Throwing the body raw puts that blob in a
    // toast where the server had already written a sentence.
    global.fetch = createMockFetch({
      'POST /api/session/s1/review/comment/c1/resolve': apiError(422, 'unknown comment c1'),
    });

    const error = await resolveReviewComment('s1', 'c1').then(
      () => null,
      (e: Error) => e,
    );
    expect(error?.message).toContain('unknown comment c1');
    expect(error?.message).not.toContain('{');
  });

  it('an expired cookie re-prompts for the key instead of dying silently', async () => {
    // Review is the one surface a user sits on for minutes without navigating,
    // so it is where a session cookie is most likely to expire mid-use. With
    // no 401 branch the write just failed and nothing ever asked them to sign
    // back in.
    const prompted = vi.fn();
    const stopListening = getBus().on('authRequired', prompted);
    global.fetch = createMockFetch({
      'POST /api/session/s1/review/comment/c1/resolve': { status: 401 },
    });

    await expect(resolveReviewComment('s1', 'c1')).rejects.toThrow();

    expect(prompted).toHaveBeenCalled();
    stopListening();
  });

  it('falls back to the status when the body is empty', async () => {
    global.fetch = createMockFetch({
      'POST /api/session/s1/review/comment/c1/resolve': { status: 500 },
    });
    await expect(resolveReviewComment('s1', 'c1')).rejects.toThrow('HTTP 500');
  });
});
