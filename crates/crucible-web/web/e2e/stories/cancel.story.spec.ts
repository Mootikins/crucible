import { test, expect } from '@playwright/test';
import { setupBasicMocks } from '../helpers/mock-api';
import { createSSEStream, SSE_HEADERS } from '../helpers/mock-sse';
import { createStory } from './_helpers/story';
import { openSessionsList } from '../helpers/nav';

/**
 * Story: WS-108 — cancel an in-flight turn.
 *
 * Sending sets isStreaming synchronously, so the stop control appears; clicking
 * it POSTs /api/session/:id/cancel, and the turn closes on the daemon's own
 * `ended` frame (the daemon emits `ended("cancelled")` on the session event
 * stream before the cancel POST resolves). The web synthesizes nothing: the
 * transcript keeps exactly what streamed, and the composer returns to send.
 *
 * Determinism note: the app's EventSource treats ANY closed SSE stream as a
 * disconnect and emits a reconnect 'error' that flips isStreaming off. The
 * session event stream here therefore stays open until the cancel lands, then
 * delivers the `ended` frame and closes; reconnects hang so nothing churns
 * after the turn is closed. Real cancellation through the live daemon is
 * exercised by the live tier.
 */

/** The frame `send.rs` emits when a turn is cancelled, on the SSE wire. */
const ENDED_FRAME = {
  type: 'session_event',
  data: { type: 'session_event', event: 'ended', data: { reason: 'cancelled' } },
};

test.describe('WS-108 cancel a turn', () => {
  test('stop control cancels, the daemon frame closes the turn, send is restored', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupBasicMocks(page, { sseEvents: [] });

    let cancelled = false;
    let markCancelled: (() => void) | null = null;
    const cancelAcknowledged = new Promise<void>((r) => (markCancelled = r));
    await page.route('**/api/session/*/cancel', (route) => {
      cancelled = true;
      markCancelled?.();
      return route.fulfill({ json: { cancelled: true } });
    });

    // Hold the event stream open so isStreaming stays true (no reconnect
    // churn); answer it with the daemon's `ended` frame once the cancel POST
    // lands.
    let hit = 0;
    await page.route(/\/api\/chat\/events\/.*/, async (route) => {
      hit += 1;
      if (hit === 1) {
        await cancelAcknowledged;
        return route.fulfill({
          status: 200,
          headers: SSE_HEADERS,
          body: createSSEStream([ENDED_FRAME]),
        });
      }
      await new Promise(() => {}); // never resolves; closed when the context tears down
    });

    await page.goto('/');
    await openSessionsList(page);
    await page.getByTestId('session-item-test-session-001').click();
    const input = page.getByTestId('chat-input');
    await expect(input).toBeEnabled({ timeout: 5000 });

    await input.fill('Do something long-running');
    await page.getByTestId('send-button').click();

    // Stop control appears while streaming.
    const cancelButton = page.getByTestId('cancel-button');
    await expect(cancelButton).toBeVisible({ timeout: 5000 });
    // User message is retained.
    await expect(page.getByTestId('message-user').first()).toContainText('Do something long-running');
    await story.step(page, 'streaming - stop control visible');

    const cancelReq = page.waitForRequest(
      (r) => r.url().includes('/api/session/test-session-001/cancel') && r.method() === 'POST',
    );
    await cancelButton.click();
    await cancelReq;
    expect(cancelled).toBe(true);

    // The turn closed on the daemon's frame, not on a web-side marker: no
    // `[cancelled]` text anywhere, spinner gone, composer back to send.
    await expect(page.getByText('[cancelled]')).toHaveCount(0, { timeout: 5000 });
    await expect(cancelButton).toHaveCount(0);
    await expect(page.getByTestId('send-button')).toBeVisible();
    await story.step(page, 'cancelled - daemon frame closed the turn, composer restored');
  });
});
