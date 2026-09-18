import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { createSSEStream, SSE_HEADERS } from './helpers/mock-sse';
import { MOCK_SESSION } from './helpers/fixtures';
import { openSessionsList } from './helpers/nav';

/**
 * E2E: Chat Happy Path
 *
 * Core send → stream → complete flow and cancel-during-stream.
 *
 * The real Axum backend serializes ChatEvent with `#[serde(tag = "type")]`,
 * so SSE data payloads include the `type` discriminator. We build events
 * matching that wire format here.
 */

/** Build SSE events matching the real backend format (type in data payload). */
function buildChatEvents(
  content: string,
  messageId = 'msg-001',
): Array<{ type: string; data: object }> {
  const chunks: string[] = [];
  for (let i = 0; i < content.length; i += 10) {
    chunks.push(content.slice(i, i + 10));
  }
  return [
    ...chunks.map((chunk) => ({
      type: 'token',
      data: { type: 'token', content: chunk },
    })),
    {
      type: 'message_complete',
      data: { type: 'message_complete', id: messageId, content, tool_calls: [] },
    },
  ];
}

test.describe('Chat happy path', () => {
  test('sends a message and displays streamed response', async ({ page }) => {
    const responseText = 'Hello! How can I help you today?';
    const sseBody = createSSEStream(buildChatEvents(responseText));

    // Set up mocks with empty SSE (we control SSE delivery separately)
    await setupBasicMocks(page, { sseEvents: [] });

    // Mock the title endpoints (auto-title fires after first response)
    await page.route('**/api/session/*/auto-title', (route) =>
      route.fulfill({ json: { title: 'Generated Title' } }),
    );
    await page.route('**/api/session/*/title', (route) =>
      route.fulfill({ status: 200, body: '{}' }),
    );

    // Controlled SSE: hold connection pending until after send, then deliver events once
    let resolveSSE: (() => void) | null = null;
    const sseReady = new Promise<void>((resolve) => {
      resolveSSE = resolve;
    });
    let delivered = false;

    await page.route(/\/api\/chat\/events\/.*/, async (route) => {
      if (!delivered) {
        delivered = true;
        await sseReady;
        await route.fulfill({
          status: 200,
          headers: SSE_HEADERS,
          body: sseBody,
        });
      } else {
        // Reconnects after delivery: empty stream (test is done by now)
        await route.fulfill({
          status: 200,
          headers: SSE_HEADERS,
          body: createSSEStream([]),
        });
      }
    });

    await page.goto('/');
    await openSessionsList(page);

    // Click the session in the left panel to select it and open in chat tab
    const sessionButton = page.getByTestId('session-item-test-session-001');
    await expect(sessionButton).toBeVisible({ timeout: 5000 });
    await sessionButton.click();

    // Wait for chat input to be visible and enabled (session loaded)
    const chatInput = page.getByTestId('chat-input');
    await expect(chatInput).toBeVisible({ timeout: 5000 });
    await expect(chatInput).not.toBeDisabled({ timeout: 5000 });

    // Type a message
    await chatInput.fill('Hello there');

    // Intercept the POST /api/chat/send request
    const sendPromise = page.waitForRequest(
      (req) => req.url().includes('/api/chat/send') && req.method() === 'POST',
    );

    // Click send
    await page.getByTestId('send-button').click();

    // Wait for the POST to complete (ensures currentStreamingMessageId is set)
    await sendPromise;

    // Now release SSE events — streaming message placeholder exists
    resolveSSE!();

    // Assert: user message appears
    const userMessage = page.getByTestId('message-user');
    await expect(userMessage.first()).toBeVisible({ timeout: 5000 });
    await expect(userMessage.first()).toContainText('Hello there');

    // Assert: assistant response appears with streamed content
    const assistantMessage = page.getByTestId('message-assistant');
    await expect(assistantMessage.first()).toContainText(responseText, {
      timeout: 10000,
    });

    // Assert: send button is re-enabled after message_complete
    const sendButton = page.getByTestId('send-button');
    await expect(sendButton).toBeVisible({ timeout: 5000 });
  });

  test('cancel button stops streaming', async ({ page }) => {
    // Cancel test: the cancel button appears from sendMessage setting isStreaming(true),
    // not from SSE event processing. Hold the SSE stream until after send, as the
    // first test does — delivering tokens on session open races the fold and leaves
    // the composer in a streaming state with no user turn.
    const longContent = 'A'.repeat(200);
    const sseBody = createSSEStream(buildChatEvents(longContent));

    await setupBasicMocks(page, { sseEvents: [] });

    // Mock cancel and title endpoints BEFORE goto
    await page.route('**/api/session/*/cancel', (route) =>
      route.fulfill({ json: { cancelled: true } }),
    );
    await page.route('**/api/session/*/auto-title', (route) =>
      route.fulfill({ json: { title: 'Generated Title' } }),
    );
    await page.route('**/api/session/*/title', (route) =>
      route.fulfill({ status: 200, body: '{}' }),
    );

    let resolveSSE: (() => void) | null = null;
    const sseReady = new Promise<void>((resolve) => {
      resolveSSE = resolve;
    });
    let delivered = false;

    await page.route(/\/api\/chat\/events\/.*/, async (route) => {
      if (!delivered) {
        delivered = true;
        await sseReady;
        await route.fulfill({ status: 200, headers: SSE_HEADERS, body: sseBody });
      } else {
        await route.fulfill({ status: 200, headers: SSE_HEADERS, body: createSSEStream([]) });
      }
    });

    await page.goto('/');
    await openSessionsList(page);

    // Click the session in the left panel to select it and open in chat tab
    const sessionButton = page.getByTestId('session-item-test-session-001');
    await expect(sessionButton).toBeVisible({ timeout: 5000 });
    await sessionButton.click();

    // Wait for chat input to be ready
    const chatInput = page.getByTestId('chat-input');
    await expect(chatInput).toBeVisible({ timeout: 5000 });
    await expect(chatInput).not.toBeDisabled({ timeout: 5000 });

    // Send a message
    await chatInput.fill('Tell me something long');
    const sendPromise = page.waitForRequest(
      (req) => req.url().includes('/api/chat/send') && req.method() === 'POST',
    );
    await page.getByTestId('send-button').click();
    await sendPromise;

    // Wait for cancel button to appear (streaming started)
    const cancelButton = page.getByTestId('cancel-button');
    await expect(cancelButton).toBeVisible({ timeout: 5000 });

    // Intercept cancel request
    const cancelPromise = page.waitForRequest(
      (req) =>
        req.url().includes(`/api/session/${MOCK_SESSION.session_id}/cancel`) &&
        req.method() === 'POST',
    );

    // Click cancel — still before SSE completes, so isStreaming stays true.
    await cancelButton.click();
    resolveSSE!();

    // Assert: cancel API was called
    const cancelRequest = await cancelPromise;
    expect(cancelRequest).toBeTruthy();

    // Assert: cancel button disappears (streaming stopped), send button re-appears
    await expect(cancelButton).not.toBeVisible({ timeout: 5000 });
    await expect(page.getByTestId('send-button')).toBeVisible({ timeout: 5000 });
  });
});
