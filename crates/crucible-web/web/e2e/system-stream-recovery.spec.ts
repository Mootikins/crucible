import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';

test('native system EventSource retry reconciles a missed change without another event', async ({ page }) => {
  await setupBasicMocks(page);
  let connections = 0;
  await page.route('**/api/events/system', route => {
    connections++;
    return route.fulfill({
      status: 200,
      contentType: 'text/event-stream',
      // EOF deliberately drops the connection. Chromium, not a fake source,
      // retries it. No publication/proposal event ever arrives.
      body: 'retry: 50\nevent: stream_version\ndata: {"version":1}\n\n',
    });
  });
  await page.goto('/editor-harness.html');
  const result = await page.evaluate(async () => {
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
    const deadline = setTimeout(() => fail(new Error('no native retry reconciliation')), 5000);
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
  expect(result).toBeGreaterThanOrEqual(2);
  expect(connections).toBeGreaterThanOrEqual(2);
});
