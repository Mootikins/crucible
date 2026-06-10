import type { Page } from '@playwright/test';

/** Serialize events to SSE wire format */
export function createSSEStream(events: Array<{ type: string; data: object }>): string {
  return events.map((e) => `event: ${e.type}\ndata: ${JSON.stringify(e.data)}\n\n`).join('');
}

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
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        Connection: 'keep-alive',
      },
      body,
    });
  });
}
