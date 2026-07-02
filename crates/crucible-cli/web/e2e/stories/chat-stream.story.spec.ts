import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from '../helpers/mock-api';
import { createSSEStream } from '../helpers/mock-sse';
import { createStory } from './_helpers/story';

/**
 * Story: WS-101 / WS-102 / WS-103 — send → stream → thinking + tool cards →
 * complete.
 *
 * Uses the real chat pipeline (ChatContext + chatEventReducer + Message/
 * ThinkingBlock/ToolCard). Two pinned visual baselines:
 *   - chat-mid-stream.png: turn in flight (working indicator), SSE held open.
 *   - chat-complete.png:   finalized message with a collapsed thinking block
 *                          and a completed tool card.
 * Dynamic relative-time labels are masked.
 */

type Frame = { type: string; data: object };

function tokenFrames(text: string): Frame[] {
  const chunks: string[] = [];
  for (let i = 0; i < text.length; i += 8) chunks.push(text.slice(i, i + 8));
  return chunks.map((content) => ({ type: 'token', data: { type: 'token', content } }));
}

const COMPLETE_STREAM: Frame[] = [
  ...tokenFrames('Here is the answer.'),
  { type: 'thinking', data: { type: 'thinking', content: 'Considering the options carefully.' } },
  {
    type: 'tool_call_start',
    data: { type: 'tool_call_start', id: 't1', name: 'read_file', arguments: { path: 'notes/x.md' } },
  },
  { type: 'tool_result_delta', data: { type: 'tool_result_delta', id: 't1', delta: 'file contents' } },
  { type: 'tool_result_complete', data: { type: 'tool_result_complete', id: 't1' } },
  {
    type: 'message_complete',
    data: {
      type: 'message_complete',
      id: 'msg-1',
      content: 'Here is the answer.',
      tool_calls: [{ id: 't1', title: 'read_file' }],
      prompt_tokens: 900,
      completion_tokens: 334,
      total_tokens: 1234,
    },
  },
];

async function selectSession(page: Page) {
  await page.goto('/');
  await page.getByTestId('session-item-test-session-001').click();
  await expect(page.getByTestId('chat-input')).toBeEnabled({ timeout: 5000 });
}

function maskDynamic(page: Page) {
  // Relative-time labels (e.g. "just now") are the only dynamic text in the
  // conversation; they render as `.text-xs.text-neutral-500` divs.
  return page.getByTestId('message-list').locator('div.text-xs.text-neutral-500');
}

test.describe('WS-101/102/103 streaming chat', () => {
  test('mid-stream working indicator (visual)', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupBasicMocks(page, { sseEvents: [] });
    // Hold the stream open: the turn stays in flight (no reconnect churn).
    await page.route(/\/api\/chat\/events\/.*/, async () => {
      await new Promise(() => {});
    });

    await selectSession(page);
    await page.getByTestId('chat-input').fill('What is the answer?');
    await page.getByTestId('send-button').click();

    await expect(page.getByTestId('cancel-button')).toBeVisible({ timeout: 5000 });
    await expect(page.getByTestId('message-user').first()).toContainText('What is the answer?');
    await story.step(page, 'turn in flight');

    await expect(page.getByTestId('message-list')).toHaveScreenshot('chat-mid-stream.png', {
      mask: [maskDynamic(page)],
      maxDiffPixelRatio: 0.02,
    });
  });

  test('streams tokens, thinking, tool card, then completes (visual)', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupBasicMocks(page, { sseEvents: [] });

    // The app subscribes to the event stream on session load — before send — so
    // hold the stream until the send POST lands (currentStreamingMessageId set),
    // then deliver the whole turn. Post-completion reconnects hang (no churn).
    let markSent: (() => void) | null = null;
    const sent = new Promise<void>((r) => (markSent = r));
    await page.route('**/api/chat/send', (route) => {
      markSent?.();
      return route.fulfill({ json: { message_id: 'msg-1' } });
    });

    let hit = 0;
    await page.route(/\/api\/chat\/events\/.*/, async (route) => {
      hit += 1;
      if (hit === 1) {
        await sent;
        return route.fulfill({
          status: 200,
          headers: { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' },
          body: createSSEStream(COMPLETE_STREAM),
        });
      }
      await new Promise(() => {});
    });

    await selectSession(page);
    await page.getByTestId('chat-input').fill('What is the answer?');
    await page.getByTestId('send-button').click();

    // Answer text streamed in.
    const assistant = page.getByTestId('message-assistant').first();
    await expect(assistant).toContainText('Here is the answer.', { timeout: 10000 });
    // Thinking block (WS-102): collapsed, shows a token count.
    await expect(page.getByText(/Thought for \d+ tokens/)).toBeVisible();
    // Tool card (WS-103): read_file rendered with a completed status.
    await expect(page.getByText('read_file')).toBeVisible();
    // Completion shows token usage (WS-101). Rendered as `.text-[11px]` — not
    // under the `.text-xs` timestamp mask — so it also appears in the baseline.
    await expect(page.getByText('1,234 tokens')).toBeVisible();
    // Completion returned the composer to send state.
    await expect(page.getByTestId('send-button')).toBeVisible();
    await story.step(page, 'completed with thinking + tool card');

    await expect(page.getByTestId('message-list')).toHaveScreenshot('chat-complete.png', {
      mask: [maskDynamic(page)],
      maxDiffPixelRatio: 0.02,
    });
  });
});
