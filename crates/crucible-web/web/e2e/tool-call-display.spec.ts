import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { createSSEStream, TranscriptFrames } from './helpers/mock-sse';
import { segment, toolCard, upsert, userTurn } from '../src/test-utils/transcript';
import { openSessionsList } from './helpers/nav';

/**
 * E2E: Tool Call Display
 *
 * Verifies tool call lifecycle rendering:
 * tool_call → tool_result_delta → tool_result_complete → message_complete
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
    // Build SSE events with type embedded in data (matching real backend
    // format), and the transcript frames the daemon sends with them.
    const transcript = new TranscriptFrames();
    const call = toolCard('msg-001', 'tool-001', {
      name: 'read_file',
      args: { path: '/test.txt' } as never,
      status: 'running',
    });
    const sseBody = createSSEStream([
      transcript.ops([upsert(userTurn('msg-001', 'Read the file'))]),
      transcript.ops([upsert(call)]),
      transcript.ops([upsert({ ...call, status: 'complete', result: 'File contents here' } as typeof call)]),
      transcript.upsert(segment('msg-001', 0, 'I read the file for you.')),
      // Real backend shape: event `tool_call` with `title` (src/web/events.rs)
      { type: 'tool_call', data: { type: 'tool_call', id: 'tool-001', title: 'read_file', arguments: { path: '/test.txt' } } },
      { type: 'tool_result_delta', data: { type: 'tool_result_delta', id: 'tool-001', delta: 'File contents here' } },
      { type: 'tool_result_complete', data: { type: 'tool_result_complete', id: 'tool-001' } },
      { type: 'token', data: { type: 'token', content: 'I read the file for you.' } },
      {
        type: 'message_complete',
        data: {
          type: 'message_complete',
          id: 'msg-002',
          content: 'I read the file for you.',
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
    await openSessionsList(page);

    // Click the session in the left panel to select it and open in chat tab
    const sessionButton = page.getByTestId('session-item-test-session-001');
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

    // Wait for the POST to complete
    await sendPromise;

    // Release the SSE events
    resolveSSE!();

    // Assert: user message appears
    const userMessage = page.getByTestId('message-user');
    await expect(userMessage.first()).toBeVisible({ timeout: 5000 });
    await expect(userMessage.first()).toContainText('Read the file');

    // Assert: ToolCard appears with tool name "read_file"
    // The transcript frames carry the tool card, which stays after the turn.
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
