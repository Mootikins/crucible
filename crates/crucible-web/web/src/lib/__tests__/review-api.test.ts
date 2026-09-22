import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createMockFetch, apiError } from '@/test-utils';
import { getBus } from '@/lib/bus';
import { resetAuthThrottleForTests } from '../api-client';
import { resolveReviewComment } from '../review-api';

/**
 * The Changes panel resolves a comment of a session record through the
 * diff route `POST /api/diff/comment/resolve`. The body names the session
 * record source, because a comment belongs to a diffset and not to a
 * session route.
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
  it('resolves a comment of the session record through the diff route', async () => {
    const mockFetch = serve('POST /api/diff/comment/resolve', {
      diffset: 'session-a/b',
      comment_id: 'c 1',
      resolved: true,
    });
    await resolveReviewComment('a/b', 'c 1');

    const sent = await mockFetch.sent(0);
    expect(sent.path).toBe('/api/diff/comment/resolve');
    expect(sent.headers.get('Content-Type')).toBe('application/json');
    expect(sent.body).toEqual({
      source: { kind: 'session_record', session: 'a/b' },
      comment_id: 'c 1',
    });
  });

  it('unwraps the error envelope instead of throwing the JSON at the user', async () => {
    // Every crucible-web route serializes a WebError as
    // `{"error": {code, message}}`. Throwing the body raw puts that blob in a
    // toast where the server had already written a sentence.
    global.fetch = createMockFetch({
      'POST /api/diff/comment/resolve': apiError(422, 'unknown comment c1'),
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
      'POST /api/diff/comment/resolve': { status: 401 },
    });

    await expect(resolveReviewComment('s1', 'c1')).rejects.toThrow();

    expect(prompted).toHaveBeenCalled();
    stopListening();
  });

  it('falls back to the status when the body is empty', async () => {
    global.fetch = createMockFetch({
      'POST /api/diff/comment/resolve': { status: 500 },
    });
    await expect(resolveReviewComment('s1', 'c1')).rejects.toThrow('HTTP 500');
  });
});
