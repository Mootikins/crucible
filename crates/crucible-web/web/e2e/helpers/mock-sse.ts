import type { Page } from '@playwright/test';

/** The stream protocol version the web client accepts (see `lib/stream-version.ts`). */
const STREAM_VERSION = 1;

/** Serialize events to SSE wire format */
export function createSSEStream(events: Array<{ type: string; data: object }>): string {
  const handshake = `event: stream_version\ndata: ${JSON.stringify({ version: STREAM_VERSION })}\n\n`;
  const body = events.map((e) => `event: ${e.type}\ndata: ${JSON.stringify(e.data)}\n\n`).join('');
  return handshake + body;
}

/** Headers every mocked SSE response must carry (G6). */
export const SSE_HEADERS = {
  'Content-Type': 'text/event-stream',
  'Cache-Control': 'no-cache',
  Connection: 'keep-alive',
  'X-Crucible-Stream-Version': String(STREAM_VERSION),
} as const;

/** Register a page.route() that responds with SSE stream. Handles reconnection (route hit multiple times). */
export async function mockSSERoute(
  page: Page,
  urlPattern: string | RegExp,
  events: Array<{ type: string; data: object }>,
): Promise<void> {
  const body = createSSEStream(events);
  await page.route(urlPattern, (route) => {
    route.fulfill({
      status: 200,
      headers: SSE_HEADERS,
      body,
    });
  });
}
