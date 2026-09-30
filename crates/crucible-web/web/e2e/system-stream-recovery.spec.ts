import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';

/**
 * Opens the system stream in the page and waits until the Inbox list was made
 * stale twice: at the first open, and at a reopen after the connection ended.
 * No publication or proposal event ever arrives.
 */
async function reconcilesTwice(page: import('@playwright/test').Page): Promise<number> {
  await page.goto('/editor-harness.html');
  return page.evaluate(async () => {
    // @ts-expect-error Vite serves the source module in this browser tier.
    const { systemEvents, resetSseForTests } = await import('/src/lib/query/sse.ts');
    // @ts-expect-error Vite source import.
    const { installSystemEventRoute } = await import('/src/lib/query/routes/system.ts');
    // @ts-expect-error Vite source import.
    const { getQueryClient } = await import('/src/lib/query/client.ts');
    // @ts-expect-error Vite source import.
    const { keys } = await import('/src/lib/query/keys.ts');
    installSystemEventRoute();
    const client = getQueryClient();
    let invalidations = 0;
    let finish!: (count: number) => void;
    let fail!: (error: Error) => void;
    const recovered = new Promise<number>((resolve, reject) => { finish = resolve; fail = reject; });
    const deadline = setTimeout(() => fail(new Error('no reconnect reconciliation')), 8000);
    const key = keys.proposals();
    const stopCache = client.getQueryCache().subscribe((event: { type: string; action?: { type: string }; query: { queryKey: unknown } }) => {
      if (event.type === 'updated' && event.action?.type === 'invalidate'
        && JSON.stringify(event.query.queryKey) === JSON.stringify(key)) {
        invalidations++;
        // A snapshot fetched after the first connection is now stale due to
        // a change during disconnect; only the reopen can invalidate it.
        client.setQueryData(key, ['snapshot']);
        if (invalidations >= 2) finish(invalidations);
      }
    });
    client.setQueryData(key, ['snapshot']);
    const stop = systemEvents().subscribe(() => {});
    try {
      return await recovered;
    } finally {
      clearTimeout(deadline);
      stop();
      stopCache();
      resetSseForTests();
    }
  });
}

/** One open of the system stream that ends at once, so the client must reopen it. */
const endedStream = {
  status: 200,
  contentType: 'text/event-stream',
  body: 'retry: 50\nevent: stream_version\ndata: {"version":1}\n\n',
};

test('a dropped system stream reopens and reconciles a missed change without another event', async ({ page }) => {
  await setupBasicMocks(page);
  let connections = 0;
  await page.route('**/api/events*', route => {
    connections++;
    // EOF deliberately drops the connection.
    return route.fulfill(endedStream);
  });
  expect(await reconcilesTwice(page)).toBeGreaterThanOrEqual(2);
  expect(connections).toBeGreaterThanOrEqual(2);
});

test('a system stream that the server refuses with a 502 opens again', async ({ page }) => {
  // A non-2xx answer leaves a browser EventSource CLOSED, and the browser
  // never retries it. The client must open a new source itself.
  await setupBasicMocks(page);
  let connections = 0;
  await page.route('**/api/events*', route => {
    connections++;
    if (connections === 1) return route.fulfill({ status: 502, contentType: 'text/plain', body: 'bad gateway' });
    return route.fulfill(endedStream);
  });
  expect(await reconcilesTwice(page)).toBeGreaterThanOrEqual(2);
  expect(connections).toBeGreaterThanOrEqual(2);
});
