import { vi } from 'vitest';

/**
 * Handler configuration for mock fetch.
 * Keys are in format "METHOD /path" (e.g., "POST /api/chat/send", "GET /api/session/list")
 * Values specify the response: status code, body, and optional headers.
 */
export interface MockFetchHandler {
  status?: number;
  body?: unknown;
  headers?: Record<string, string>;
}

/**
 * Create a mock fetch function for testing.
 *
 * @param handlers - Record mapping "METHOD /path" to response configuration
 * @returns A vi.fn() that matches URL patterns and returns Response objects
 *
 * @example
 * const mockFetch = createMockFetch({
 *   'POST /api/chat/send': { body: { message_id: 'msg-001' } },
 *   'GET /api/session/list': { body: { sessions: [] } },
 *   'POST /api/session': { status: 500 }, // error case
 * });
 * global.fetch = mockFetch;
 */
export function createMockFetch(
  handlers: Record<string, MockFetchHandler>
): ReturnType<typeof vi.fn> {
  return vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === 'string' ? input : input.toString();
    const method = (init?.method || 'GET').toUpperCase();

    // Extract path from URL (remove query string and hash)
    const urlObj = new URL(url, 'http://localhost');
    const path = urlObj.pathname;

    // Try to match "METHOD /path" pattern
    const key = `${method} ${path}`;
    const handler = handlers[key];

    if (!handler) {
      // Return 404 for unmatched URLs
      return new Response(JSON.stringify({ error: 'Not found' }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
      });
    }

    const status = handler.status ?? 200;
    const body = handler.body ? JSON.stringify(handler.body) : '';
    const responseHeaders = {
      'Content-Type': 'application/json',
      ...handler.headers,
    };

    return new Response(body, {
      status,
      headers: responseHeaders,
    });
  });
}
