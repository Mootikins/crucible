import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { createSSEStream } from './helpers/mock-sse';
import { MOCK_SESSION } from './helpers/fixtures';

/**
 * E2E: LLM Title Generation
 *
 * Verifies that after the first assistant response, the frontend calls
 * `/api/session/{id}/generate-title` to get an LLM-generated title,
 * then sets it via PUT `/api/session/{id}/title`.
 *
 * Also verifies fallback to truncation when the generate-title endpoint fails.
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

/** Session with no title (triggers auto-generate). */
const UNTITLED_SESSION = {
  ...MOCK_SESSION,
  title: null,
};

test.describe('LLM title generation', () => {
  test('calls generate-title endpoint after first assistant response', async ({ page }) => {
    const responseText = 'Sure, I can help with that!';
    const generatedTitle = 'Help with project setup';
    const sseBody = createSSEStream(buildChatEvents(responseText));

    // Use empty SSE from setupBasicMocks; override with controlled SSE below
    await setupBasicMocks(page, { sseEvents: [] });

    // Override session GET to return untitled session
    await page.route('**/api/session/test-session-001', async (route) => {
      if (route.request().method() === 'GET') {
        await route.fulfill({ json: UNTITLED_SESSION });
      } else {
        await route.fallback();
      }
    });

    // Mock generate-title to return LLM-generated title
    await page.route('**/api/session/*/generate-title', (route) =>
      route.fulfill({ json: { title: generatedTitle } }),
    );

    // Mock title PUT
    await page.route('**/api/session/*/title', async (route) => {
      if (route.request().method() === 'PUT') {
        await route.fulfill({ status: 200, body: '{}' });
      } else {
        await route.fallback();
      }
    });

    // Controlled SSE: hold connection until after send, then deliver events
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

    // Click session to open chat
    const sessionButton = page.getByTestId('session-item-test-session-001');
    await expect(sessionButton).toBeVisible({ timeout: 5000 });
    await sessionButton.click();

    // Wait for chat input
    const chatInput = page.getByTestId('chat-input');
    await expect(chatInput).toBeVisible({ timeout: 5000 });
    await expect(chatInput).not.toBeDisabled({ timeout: 5000 });

    // Send message and set up request watchers BEFORE releasing SSE
    await chatInput.fill('Help me set up my project');

    const sendPromise = page.waitForRequest(
      (req) => req.url().includes('/api/chat/send') && req.method() === 'POST',
    );

    // Watch for generate-title POST (fires after message_complete)
    const generateTitlePromise = page.waitForRequest(
      (req) => req.url().includes('/generate-title') && req.method() === 'POST',
      { timeout: 15000 },
    );

    // Watch for title PUT (fires after generate-title returns)
    const titlePutPromise = page.waitForRequest(
      (req) => req.url().includes('/title') && req.method() === 'PUT',
      { timeout: 15000 },
    );

    await page.getByTestId('send-button').click();
    await sendPromise;

    // Release SSE events — message_complete triggers autoGenerateTitle
    resolveSSE!();

    // Assert: generate-title was called
    const generateReq = await generateTitlePromise;
    expect(generateReq.url()).toContain(`/api/session/${UNTITLED_SESSION.session_id}/generate-title`);

    // Assert: title was set via PUT with the generated title
    const titleReq = await titlePutPromise;
    const titleBody = titleReq.postDataJSON();
    expect(titleBody.title).toBe(generatedTitle);
  });

  test('falls back to truncation when generate-title fails', async ({ page }) => {
    const responseText = 'Here is my response';
    const sseBody = createSSEStream(buildChatEvents(responseText));

    await setupBasicMocks(page, { sseEvents: [] });

    // Override session GET to return untitled session
    await page.route('**/api/session/test-session-001', async (route) => {
      if (route.request().method() === 'GET') {
        await route.fulfill({ json: UNTITLED_SESSION });
      } else {
        await route.fallback();
      }
    });

    // generate-title returns 500 to trigger fallback
    await page.route('**/api/session/*/generate-title', (route) =>
      route.fulfill({ status: 500, body: 'Internal Server Error' }),
    );

    // Mock title PUT
    await page.route('**/api/session/*/title', async (route) => {
      if (route.request().method() === 'PUT') {
        await route.fulfill({ status: 200, body: '{}' });
      } else {
        await route.fallback();
      }
    });

    // Controlled SSE delivery
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

    // Click session to open chat
    const sessionButton = page.getByTestId('session-item-test-session-001');
    await expect(sessionButton).toBeVisible({ timeout: 5000 });
    await sessionButton.click();

    // Wait for chat input
    const chatInput = page.getByTestId('chat-input');
    await expect(chatInput).toBeVisible({ timeout: 5000 });
    await expect(chatInput).not.toBeDisabled({ timeout: 5000 });

    const userMessage = 'Help me set up my project with some configuration';
    await chatInput.fill(userMessage);

    const sendPromise = page.waitForRequest(
      (req) => req.url().includes('/api/chat/send') && req.method() === 'POST',
    );

    // Watch for title PUT (fallback truncation sets title after generate-title fails)
    const titlePutPromise = page.waitForRequest(
      (req) => req.url().includes('/title') && req.method() === 'PUT',
      { timeout: 15000 },
    );

    await page.getByTestId('send-button').click();
    await sendPromise;

    // Release SSE events
    resolveSSE!();

    // Assert: fallback title was set via PUT (truncated, not LLM-generated)
    const titleReq = await titlePutPromise;
    const titleBody = titleReq.postDataJSON();
    expect(titleBody.title).toBeTruthy();
    expect(titleBody.title.length).toBeLessThanOrEqual(53); // 50 chars + '...'
    // Should be a truncation of the user message, not empty
    expect(userMessage).toContain(titleBody.title.replace('...', '').trim());
  });
});
