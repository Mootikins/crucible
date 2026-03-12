import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { createSSEStream } from './helpers/mock-sse';

/**
 * E2E: Tool Call Display
 *
 * Verifies tool call lifecycle rendering:
 * tool_call_start → tool_result_delta → tool_result_complete → message_complete
 *
 * Uses deferred SSE delivery (same pattern as chat-happy-path):
 *   1. Send message → POST completes
 *   2. Release SSE events → tool card appears, assistant message streams
 *
 * NOTE: SSE event data must include `type` field to match the real Axum backend's
 * `#[serde(tag = "type")]` serialization, which the frontend's handleEvent switch
 * requires for dispatching.
 */

test.describe('Tool call display', () => {
  test('displays tool call card during execution', async ({ page }) => {
    // Build SSE events with type embedded in data (matching real backend format)
    const sseBody = createSSEStream([
      { type: 'tool_call_start', data: { type: 'tool_call_start', id: 'tool-001', name: 'read_file', arguments: { path: '/test.txt' } } },
      { type: 'tool_result_delta', data: { type: 'tool_result_delta', id: 'tool-001', delta: 'File contents here' } },
      { type: 'tool_result_complete', data: { type: 'tool_result_complete', id: 'tool-001' } },
      { type: 'token', data: { type: 'token', content: 'I read the file for you.' } },
      {
        type: 'message_complete',
        data: {
          type: 'message_complete',
          id: 'msg-002',
          content: 'I read the file for you.',
          // Include tool_calls so the Message component renders a persistent ToolCard
          tool_calls: [{ id: 'tool-001', title: 'read_file' }],
        },
      },
    ]);

    // Set up mocks with empty SSE (we control SSE delivery separately)
    await setupBasicMocks(page, { sseEvents: [] });

    // Mock the title endpoint (auto-title fires after first response)
    await page.route('**/api/session/*/title', (route) =>
      route.fulfill({ status: 200, body: '{}' }),
    );

    // Controlled SSE: hold connection pending until after send, then deliver events
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
          headers: {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
            Connection: 'keep-alive',
          },
          body: sseBody,
        });
      } else {
        // Reconnects after delivery: empty stream
        await route.fulfill({
          status: 200,
          headers: {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
            Connection: 'keep-alive',
          },
          body: '',
        });
      }
    });

    await page.goto('/');

    // Click the session in the left panel to select it and open in chat tab
    const sessionButton = page.getByRole('button', { name: /Test Session/ });
    await expect(sessionButton).toBeVisible({ timeout: 5000 });
    await sessionButton.click();

    // Wait for chat input to be visible and enabled (session loaded)
    const chatInput = page.getByTestId('chat-input');
    await expect(chatInput).toBeVisible({ timeout: 5000 });
    await expect(chatInput).not.toBeDisabled({ timeout: 5000 });

    // Type and send a message
    await chatInput.fill('Read the file');

    const sendPromise = page.waitForRequest(
      (req) => req.url().includes('/api/chat/send') && req.method() === 'POST',
    );
    await page.getByTestId('send-button').click();

    // Wait for POST to complete (ensures currentStreamingMessageId is set)
    await sendPromise;

    // Release SSE events — streaming message placeholder exists
    resolveSSE!();

    // Assert: user message appears
    const userMessage = page.getByTestId('message-user');
    await expect(userMessage.first()).toBeVisible({ timeout: 5000 });
    await expect(userMessage.first()).toContainText('Read the file');

    // Assert: ToolCard appears with tool name "read_file"
    // After message_complete, the Message component renders a persistent ToolCard
    // from tool_calls in the event data (name mapped from tool.title)
    await expect(page.locator('text=read_file')).toBeVisible({ timeout: 5000 });

    // Assert: tool result — expand the ToolCard to verify expanded content renders.
    // The ToolCard is collapsed by default; click to expand and check the ID section.
    const toolCardButton = page.locator('button', { hasText: 'read_file' });
    await toolCardButton.first().click();
    // Expanded card shows tool ID and status indicator
    await expect(page.locator('text=tool-001')).toBeVisible({ timeout: 5000 });

    // Assert: final assistant message appears with streamed content
    await expect(page.getByTestId('message-assistant').first()).toContainText(
      'I read the file for you.',
      { timeout: 10000 },
    );
  });
});
