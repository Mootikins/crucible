import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';

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

    // Assert: error message appears in the ChatInput error display
    // sendChatMessage throws "Failed to send message: HTTP 500"
    // ChatContext catches it and calls setError(err.message)
    await page.waitForFunction(
      () => {
        const el = document.querySelector('[class*="text-error"]');
        return el && el.textContent?.includes('Failed to send message');
      },
      null,
      { timeout: 5000 },
    );
  });

  test('shows error when SSE stream contains error event', async ({ page }) => {
    // SSE error events — include `type` in data so handleEvent's switch matches
    const errorEvents = [
      {
        type: 'error',
        data: {
          type: 'error',
          code: 'agent_error',
          message: 'Agent failed to process request',
        },
      },
    ];

    await setupBasicMocks(page, { sseEvents: errorEvents });

    await page.goto('/');

    // Click the session in the sidebar to open it in the chat tab
    const sessionItem = page.getByTestId('session-item-test-session-001');
    await expect(sessionItem).toBeVisible({ timeout: 5000 });
    await sessionItem.click();

    // Wait for chat input to be ready
    const chatInput = page.getByTestId('chat-input');
    await expect(chatInput).toBeVisible({ timeout: 5000 });
    await expect(chatInput).not.toBeDisabled({ timeout: 5000 });

    // Send a message — SSE error event fires on EventSource connection/reconnection
    await chatInput.fill('Hello');
    await page.getByTestId('send-button').click();

    // Assert: error from SSE error event surfaces in the UI.
    // handleEvent sets error AND updates the streaming assistant message to
    // "Error: <message>". The error banner may be overwritten by
    // "Reconnecting..." from EventSource onerror, but the assistant message
    // content persists as the reliable indicator.
    const assistantMessage = page.getByTestId('message-assistant');
    await expect(assistantMessage.first()).toContainText('Agent failed to process request', {
      timeout: 10000,
    });
  });
});
