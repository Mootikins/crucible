import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { openSessionsList } from './helpers/nav';

/**
 * E2E: Error Handling
 *
 * Verifies that API failures and SSE error events surface in the UI.
 */

test.describe('Error handling', () => {
  test('shows error when send message API fails', async ({ page }) => {
    // Override POST /api/chat/send to return HTTP 500
    await setupBasicMocks(page, { chatMessage: 500 });

    await page.goto('/');
    await openSessionsList(page);

    // Click the session in the sidebar to open it in the chat tab
    const sessionItem = page.getByTestId('session-item-test-session-001');
    await expect(sessionItem).toBeVisible({ timeout: 5000 });
    await sessionItem.click();

    // Wait for chat input to be ready
    const chatInput = page.getByTestId('chat-input');
    await expect(chatInput).toBeVisible({ timeout: 5000 });
    await expect(chatInput).not.toBeDisabled({ timeout: 5000 });

    // Type and send a message
    await chatInput.fill('Hello');
    await page.getByTestId('send-button').click();

    // Assert: the failure surfaces in the transcript. `request()` words a
    // failed send as "Failed to send message: <the body's sentence>", or
    // ": HTTP 500" when the body says nothing; ChatContext keeps the user's
    // text and appends an inline "Failed to send: …" system notice.
    await expect(page.getByText(/Failed to send/).first()).toBeVisible({ timeout: 5000 });
  });

  // The SSE stream carries no `error` event: the daemon never sends one, and
  // `ChatEvent::Error` (deleted in step 11 of the Simplification Plan) was
  // declared but never constructed by `from_daemon_event` either — this
  // test exercised a payload nothing on the real wire ever sends. A daemon
  // failure mid-turn reaches the transcript through `turn_finished` with
  // `status: "failed"` instead; see `chatEventReducer.test.ts`'s
  // `turn_finished: a failed turn shows its error` case.
});
