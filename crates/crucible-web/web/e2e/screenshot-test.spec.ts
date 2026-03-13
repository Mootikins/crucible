import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { createSSEStream } from './helpers/mock-sse';
import { MOCK_SESSION } from './helpers/fixtures';

function buildChatEvents(content: string, messageId = 'msg-001') {
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

test('send/stream flow screenshot', async ({ page }) => {
  const responseText = 'Hello! How can I help you today?';
  const sseBody = createSSEStream(buildChatEvents(responseText));

  await setupBasicMocks(page, { sseEvents: [] });
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
  const sessionButton = page.getByRole('button', { name: /Test Session/ });
  await expect(sessionButton).toBeVisible({ timeout: 5000 });
  await sessionButton.click();

  const chatInput = page.getByTestId('chat-input');
  await expect(chatInput).toBeVisible({ timeout: 5000 });
  await expect(chatInput).not.toBeDisabled({ timeout: 5000 });

  await chatInput.fill('Hello there');
  const sendPromise = page.waitForRequest(
    (req) => req.url().includes('/api/chat/send') && req.method() === 'POST',
  );

  await page.getByTestId('send-button').click();
  await sendPromise;
  resolveSSE!();

  // Wait for response to appear
  const assistantMessage = page.getByTestId('message-assistant');
  await expect(assistantMessage.first()).toContainText(responseText, {
    timeout: 10000,
  });

  // Take screenshot
  await page.screenshot({ path: '/home/moot/crucible/.sisyphus/evidence/task-5-send-stream.png' });
});

test('session resume flow screenshot', async ({ page }) => {
  await setupBasicMocks(page, {});
  await page.goto('/');

  // Wait for session list
  await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });

  // Click session to select it
  const sessionButton = page.getByRole('button', { name: /Test Session/ });
  await expect(sessionButton).toBeVisible({ timeout: 5000 });
  await sessionButton.click();

  // Wait for chat input to be ready (session loaded)
  const chatInput = page.getByTestId('chat-input');
  await expect(chatInput).toBeVisible({ timeout: 5000 });
  await expect(chatInput).not.toBeDisabled({ timeout: 5000 });

  // Take screenshot
  await page.screenshot({ path: '/home/moot/crucible/.sisyphus/evidence/task-5-resume.png' });
});
