import { vi, type Mock } from 'vitest';

/**
 * The fixed answer of one route: a status, a body the mock serialises as JSON,
 * and headers. It is the form every caller wrote before the query layer, so it
 * stays the default.
 */
export interface MockFetchHandler {
  status?: number;
  body?: unknown;
  headers?: Record<string, string>;
}

/**
 * The answer of one route as a function.
 *
 * The function reads the `Request`, so a test can assert the method, the
 * headers or the body the caller sent, and answer a different value per call.
 * It returns either a `Response` it builds itself, for a status or a content
 * type the object form cannot name, or a plain value the mock serialises as a
 * JSON body with status 200.
 *
 * The URL of that `Request` is absolute (`http://localhost/api/...`), because
 * the `Request` constructor refuses a relative one and every caller in the app
 * fetches a relative path.
 */
export type MockFetchRoute = (request: Request) => unknown;

/** What a test gives one `"METHOD /path"` key. */
export type MockFetchAnswer = MockFetchHandler | MockFetchRoute;

/** What `createMockFetch` answers: a `fetch` a test can also question. */
export type MockFetch = Mock<typeof fetch> & {
  /**
   * Counts the calls that matched one `"METHOD /path"` key, whatever query
   * string followed the path. A key no caller reached counts zero, and a call
   * that matched no route counts under the key it asked for.
   */
  calls(key: string): number;
};

/**
 * The error envelope every `crucible-web` route serialises, as a handler.
 *
 * `request()` in `lib/api.ts` reads `error.message` out of it and puts that
 * sentence in the error it throws, so a test that asserts the daemon's own
 * words needs the envelope and not a bare status.
 */
export function apiError(status: number, message: string): MockFetchHandler {
  return { status, body: { error: { code: status, message } } };
}

/** Builds the `Response` of the object form. */
function fixedResponse(handler: MockFetchHandler): Response {
  return new Response(handler.body === undefined ? '' : JSON.stringify(handler.body), {
    status: handler.status ?? 200,
    headers: { 'Content-Type': 'application/json', ...handler.headers },
  });
}

/** Builds the `Request` one function route reads. */
function requestOf(url: string, init: RequestInit | undefined): Request {
  return new Request(new URL(url, 'http://localhost'), init);
}

/**
 * Create a mock fetch function for testing.
 *
 * @param handlers - Record mapping "METHOD /path" to an answer
 * @returns A `vi.fn()` that matches the key of each call and answers it
 *
 * @example
 * const mockFetch = createMockFetch({
 *   'POST /api/chat/send': { body: { message_id: 'msg-001' } },
 *   'GET /api/kilns': (req) => [{ name: 'main', path: '/kilns/main' }],
 *   'GET /api/fs/list': apiError(422, 'root is not a registered project'),
 * });
 * global.fetch = mockFetch;
 * expect(mockFetch.calls('GET /api/kilns')).toBe(1);
 */
export function createMockFetch(handlers: Record<string, MockFetchAnswer>): MockFetch {
  const counts = new Map<string, number>();

  const mock = vi.fn<typeof fetch>(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === 'string' ? input : input.toString();
    const method = (init?.method || 'GET').toUpperCase();

    // The key names the path alone, so one route answers every query string.
    const path = new URL(url, 'http://localhost').pathname;
    const key = `${method} ${path}`;
    counts.set(key, (counts.get(key) ?? 0) + 1);

    const handler = handlers[key];
    if (!handler) {
      // A route the test did not name answers 404, which is what the daemon
      // answers for a path it does not serve.
      return new Response(JSON.stringify({ error: 'Not found' }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
      });
    }

    if (typeof handler !== 'function') return fixedResponse(handler);

    const answer = await handler(requestOf(url, init));
    if (answer instanceof Response) return answer;
    return fixedResponse({ body: answer });
  });

  return Object.assign(mock, {
    calls: (key: string) => counts.get(key) ?? 0,
  });
}
