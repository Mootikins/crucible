import { test, expect } from '@playwright/test';
import { setupBasicMocks } from '../helpers/mock-api';
import { createStory } from './_helpers/story';

/**
 * Story: WS-108 — cancel an in-flight turn.
 *
 * Sending sets isStreaming synchronously, so the stop control appears; clicking
 * it POSTs /api/session/:id/cancel, appends a `[cancelled]` marker to the
 * partial assistant message, and returns the composer to the send state.
 *
 * Determinism note: the app's EventSource treats ANY closed SSE stream as a
 * disconnect and emits a reconnect 'error' that flips isStreaming off. To keep
 * the streaming state stable for the assertion we hold the SSE connection open
 * (never close it) rather than delivering-then-closing token frames. Real
 * partial-token retention through cancel is exercised by the live tier.
 */

test.describe('WS-108 cancel a turn', () => {
  test('stop control cancels, marks the message cancelled, restores send', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupBasicMocks(page, { sseEvents: [] });

    // Hold the event stream open so isStreaming stays true (no reconnect churn).
    await page.route(/\/api\/chat\/events\/.*/, async () => {
      await new Promise(() => {}); // never resolves; closed when the context tears down
    });

    let cancelled = false;
    await page.route('**/api/session/*/cancel', (route) => {
      cancelled = true;
      return route.fulfill({ json: { cancelled: true } });
    });

    await page.goto('/');
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

    // Partial assistant message retained with a cancelled marker; composer back.
    await expect(page.getByText('[cancelled]')).toBeVisible({ timeout: 5000 });
    await expect(cancelButton).toHaveCount(0);
    await expect(page.getByTestId('send-button')).toBeVisible();
    await story.step(page, 'cancelled - marker shown, composer restored');
  });
});
