import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from '../helpers/mock-api';
import { createSSEStream, TranscriptFrames } from '../helpers/mock-sse';
import { segment, toolCard, upsert, userTurn } from '../../src/test-utils/transcript';
import { createStory } from './_helpers/story';
import { waitForFonts } from './_helpers/fonts';
import { openSessionsList } from '../helpers/nav';

/**
 * Story: WS-101 / WS-102 / WS-103 — send → stream → thinking + tool cards →
 * complete; and the mid-turn queue (2026-09-18): a message typed while the
 * turn streams renders below the block, marked queued, unsent.
 *
 * Uses the real chat pipeline (ChatContext + transcriptStore + Message/
 * ThinkingBlock/ToolCard). Each stream carries the events and the transcript
 * frames of the daemon's fold. Pinned visual baselines:
 *   - chat-mid-stream.png: turn in flight (working indicator), SSE held open.
 *   - chat-thinking.png:   thinking streaming — the block is OPEN, the text
 *                          visible with its caret (it used to sit collapsed
 *                          behind the header while the model reasoned).
 *   - chat-complete.png:   finalized message with a collapsed thinking block
 *                          and a completed tool card.
 *   - chat-queued.png:     a mid-turn prompt parked below the streaming turn.
 * Dynamic relative-time labels are masked.
 */

type Frame = { type: string; data: object };

function tokenFrames(text: string): Frame[] {
  const chunks: string[] = [];
  for (let i = 0; i < text.length; i += 8) chunks.push(text.slice(i, i + 8));
  return chunks.map((content) => ({
    type: 'text_delta',
    data: { event: 'text_delta', data: { content } },
  }));
}

const PROMPT = 'What is the answer?';
const ANSWER = 'Here is the answer.';
const USAGE = { prompt_tokens: 900, completion_tokens: 334, total_tokens: 1234 };

/**
 * The daemon's fold of COMPLETE_STREAM: the answer and its reasoning in one
 * segment, which the tool call closes, then the tool card, then an empty
 * segment that carries the usage of the turn.
 */
function completeTranscript(): Frame[] {
  const t = new TranscriptFrames();
  const open = segment('msg-1', 0, '', { streaming: true });
  const call = toolCard('msg-1', 't1', { name: 'read_file', args: { path: 'notes/x.md' } as never, status: 'running' });
  return [
    t.ops([upsert(userTurn('msg-1', PROMPT))]),
    t.upsert(open),
    ...t.appends(open.id, ANSWER, 8),
    t.ops([{ op: 'append', id: open.id, field: 'thinking', at: 0, text: 'Considering the options carefully.' }]),
    t.ops([
      upsert(segment('msg-1', 0, ANSWER, { thinking: 'Considering the options carefully.' })),
      upsert(call),
    ]),
    t.ops([upsert({ ...call, status: 'complete', result: 'file contents' } as typeof call)]),
    t.upsert(segment('msg-1', 1, '', { usage: USAGE })),
  ];
}

const COMPLETE_STREAM: Frame[] = [
  ...completeTranscript(),
  ...tokenFrames('Here is the answer.'),
  {
    type: 'thinking',
    data: { event: 'thinking', data: { content: 'Considering the options carefully.' } },
  },
  // Real backend shape: `TurnPayload::ToolCall` carries `call_id`/`tool`/`args`
  // (`crates/crucible-core/src/protocol/session_events/turn.rs`) — not
  // `id`/`title`/`arguments`, and the wire has no delta/complete pair for a
  // tool result: one `tool_result` event carries the whole output.
  {
    type: 'tool_call',
    data: { event: 'tool_call', data: { call_id: 't1', tool: 'read_file', args: { path: 'notes/x.md' } } },
  },
  {
    type: 'tool_result',
    data: {
      event: 'tool_result',
      data: { call_id: 't1', tool: 'read_file', result: 'file contents', terminate: false },
    },
  },
  {
    type: 'message_complete',
    data: {
      event: 'message_complete',
      data: {
        message_id: 'msg-1',
        full_response: 'Here is the answer.',
        prompt_tokens: 900,
        completion_tokens: 334,
        total_tokens: 1234,
      },
    },
  },
];

async function selectSession(page: Page) {
  await page.goto('/');
  await openSessionsList(page);
  await waitForFonts(page);
  await page.getByTestId('session-item-test-session-001').click();
  await expect(page.getByTestId('chat-input')).toBeEnabled({ timeout: 5000 });
}

// Pin the captured element to a fixed box: its natural height is the sum of
// text line heights, which drifts ±1px between font stacks (CI vs local
// freetype) — and a size mismatch fails toHaveScreenshot before any diff
// ratio applies. Both baselines have empty space at the bottom, so the crop
// loses nothing.
//
// The WIDTH is pinned for the same reason, one layout change later. It used
// to be whatever the surrounding shell handed over — 520px, the old right
// rail — so moving the session into the centre tiling silently reshot every
// baseline at 458px. A transcript story is about the transcript; it should
// not fail because a pane beside it was resized.
async function pinCaptureBox(page: Page) {
  await page.getByTestId('message-list').evaluate((el) => {
    el.style.height = '480px';
    el.style.minHeight = '480px';
    el.style.maxHeight = '480px';
    el.style.width = '520px';
    el.style.minWidth = '520px';
    el.style.maxWidth = '520px';
    el.style.flex = 'none';
    el.style.overflow = 'hidden';
  });
}

function maskDynamic(page: Page) {
  // Relative-time labels (e.g. "just now") are the only dynamic text in the
  // conversation; every one is tagged `data-dynamic-time` at its render site
  // (user prompt timestamp + the turn meta row).
  return page.getByTestId('message-list').locator('[data-dynamic-time]');
}

test.describe('WS-101/102/103 streaming chat', () => {
  test('mid-stream working indicator (visual)', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupBasicMocks(page, { sseEvents: [] });
    // Hold the stream open: the turn stays in flight (no reconnect churn).
    await page.route(/\/api\/events\?.*/, async () => {
      await new Promise(() => {});
    });

    await selectSession(page);
    await page.getByTestId('chat-input').fill('What is the answer?');
    await page.getByTestId('send-button').click();

    await expect(page.getByTestId('cancel-button')).toBeVisible({ timeout: 5000 });
    await expect(page.getByTestId('message-user').first()).toContainText('What is the answer?');
    await story.step(page, 'turn in flight');

    await pinCaptureBox(page);
    await expect(page.getByTestId('message-list')).toHaveScreenshot('chat-mid-stream.png', {
      mask: [maskDynamic(page)],
      // Headroom for cross-environment text antialiasing/advance drift.
      maxDiffPixelRatio: 0.03,
    });
  });

  test('streams tokens, thinking, tool card, then completes (visual)', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupBasicMocks(page, { sseEvents: [] });

    // The app subscribes to the event stream on session load — before send — so
    // hold the stream until the send POST lands,
    // then deliver the whole turn. Post-completion reconnects hang (no churn).
    let markSent: (() => void) | null = null;
    const sent = new Promise<void>((r) => (markSent = r));
    await page.route('**/api/rpc/session.send_message', (route) => {
      markSent?.();
      return route.fulfill({ json: { message_id: 'msg-1' } });
    });

    let hit = 0;
    const SESSION_TOPIC = 'test-session-001';
    const withTopic = (frames: Frame[]) => frames.map((frame) => ({ type: frame.type, data: { ...frame.data, topic: SESSION_TOPIC } }));
    await page.route(/\/api\/events\?.*/, async (route) => {
      const topics = new URL(route.request().url()).searchParams.get('topics')?.split(',') ?? [];
      if (!topics.includes(SESSION_TOPIC)) {
        return route.fulfill({
          status: 200,
          headers: { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' },
          body: createSSEStream([]),
        });
      }
      hit += 1;
      if (hit === 1) {
        await sent;
        return route.fulfill({
          status: 200,
          headers: { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' },
          body: createSSEStream(withTopic(COMPLETE_STREAM)),
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
    // Thinking block (WS-102): collapsed, shows a token estimate.
    await expect(page.getByText(/Thought for ~\d+ tokens/)).toBeVisible();
    // Tool card (WS-103): read_file rendered with a completed status.
    await expect(page.getByText('read_file')).toBeVisible();
    // Completion shows token usage (WS-101). Rendered as `.text-[11px]` — not
    // under the `.text-xs` timestamp mask — so it also appears in the baseline.
    await expect(page.getByText('1,234 tokens')).toBeVisible();
    // Completion returned the composer to send state.
    await expect(page.getByTestId('send-button')).toBeVisible();
    await story.step(page, 'completed with thinking + tool card');

    await pinCaptureBox(page);
    await expect(page.getByTestId('message-list')).toHaveScreenshot('chat-complete.png', {
      mask: [maskDynamic(page)],
      // Headroom for cross-environment text antialiasing/advance drift.
      maxDiffPixelRatio: 0.03,
    });
  });

  test('thinking streams expanded and settles collapsed (visual)', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupBasicMocks(page, { sseEvents: [] });
    // First events delivery: thinking only, no answer yet — the block should
    // be OPEN with the text and its growing-edge caret visible while the
    // model reasons.
    let markSent: (() => void) | null = null;
    const sent = new Promise<void>((r) => (markSent = r));
    await page.route('**/api/rpc/session.send_message', (route) => {
      markSent?.();
      return route.fulfill({ json: { message_id: 'msg-1' } });
    });

    let hit = 0;
    // The daemon's fold: the reasoning streams into an open segment, the
    // answer follows it in the same segment, and the end closes it.
    const REASONING = 'The user asks about the answer. I should weigh the options before replying, starting with the kiln.';
    const t = new TranscriptFrames();
    const open = segment('msg-1', 0, '', { streaming: true });
    const THINKING_ONLY: Frame[] = [
      t.ops([upsert(userTurn('msg-1', PROMPT))]),
      t.upsert(open),
      t.ops([{ op: 'append', id: open.id, field: 'thinking', at: 0, text: REASONING }]),
      { type: 'thinking', data: { event: 'thinking', data: { content: REASONING } } },
    ];
    const REST: Frame[] = [
      ...t.appends(open.id, ANSWER, 8),
      t.upsert(segment('msg-1', 0, ANSWER, { thinking: REASONING, usage: USAGE })),
      ...tokenFrames('Here is the answer.'),
      {
        type: 'message_complete',
        data: {
          event: 'message_complete',
          data: {
            message_id: 'msg-1',
            full_response: 'Here is the answer.',
            prompt_tokens: 900,
            completion_tokens: 334,
            total_tokens: 1234,
          },
        },
      },
    ];
    const SESSION_TOPIC = 'test-session-001';
    const withTopic = (frames: Frame[]) => frames.map((frame) => ({ type: frame.type, data: { ...frame.data, topic: SESSION_TOPIC } }));
    await page.route(/\/api\/events\?.*/, async (route) => {
      const topics = new URL(route.request().url()).searchParams.get('topics')?.split(',') ?? [];
      if (!topics.includes(SESSION_TOPIC)) {
        return route.fulfill({
          status: 200,
          headers: { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' },
          body: createSSEStream([]),
        });
      }
      hit += 1;
      if (hit === 1) {
        await sent;
        return route.fulfill({
          status: 200,
          headers: { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' },
          body: createSSEStream(withTopic(THINKING_ONLY)),
        });
      }
      if (hit === 2) {
        // The thinking stream ended; the client reconnects on its cursor and
        // now gets the rest of the turn.
        return route.fulfill({
          status: 200,
          headers: { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' },
          body: createSSEStream(withTopic(REST)),
        });
      }
      await new Promise(() => {});
    });

    await selectSession(page);
    await page.getByTestId('chat-input').fill('What is the answer?');
    await page.getByTestId('send-button').click();

    // Streaming: the reasoning text is visible WITHOUT a click, with the
    // caret at its growing edge, under a "Thinking" header with the wave.
    const thinking = page.getByText('The user asks about the answer.', { exact: false });
    await expect(thinking).toBeVisible({ timeout: 10000 });
    await expect(page.getByTestId('think-stream-caret')).toBeAttached();
    await story.step(page, 'thinking streams open');

    await pinCaptureBox(page);
    await expect(page.getByTestId('message-list')).toHaveScreenshot('chat-thinking.png', {
      mask: [maskDynamic(page)],
      maxDiffPixelRatio: 0.03,
    });

    // Completion folds the block to the summary line: quiet, no caret. The
    // label is the honest unit — a token ESTIMATE (chars/4) — hence the `~`.
    await expect(page.getByText('Here is the answer.')).toBeVisible({ timeout: 10000 });
    await expect(page.getByText(/Thought for ~\d+ tokens/)).toBeVisible();
    await expect(page.getByTestId('think-stream-caret')).toHaveCount(0);
    // The fold is the grid row closing; chat-complete.png carries the visual
    // proof that the reasoning text is gone from the settled view.
    await expect
      .poll(async () =>
        page
          .getByText('The user asks about the answer.')
          .evaluate((el) => el.closest('div.grid')?.getBoundingClientRect().height ?? -1),
      )
      .toBe(0);
  });

  test('a mid-turn send queues below the streaming block (visual)', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupBasicMocks(page, { sseEvents: [] });
    // Hold the stream open: the first turn never completes.
    await page.route(/\/api\/events\?.*/, async () => {
      await new Promise(() => {});
    });
    // The send POST answers (the turn is admitted) but no events follow.
    let sendCount = 0;
    await page.route('**/api/rpc/session.send_message', (route) => {
      sendCount += 1;
      return route.fulfill({ json: { message_id: 'msg-1' } });
    });

    await selectSession(page);
    await page.getByTestId('chat-input').fill('What is the answer?');
    await page.getByTestId('send-button').click();
    await expect(page.getByTestId('cancel-button')).toBeVisible({ timeout: 5000 });

    // Typing stays live mid-turn; Enter queues the prompt below the block.
    await page.getByTestId('chat-input').fill('And then also check the kiln');
    await page.getByTestId('chat-input').press('Enter');

    const queued = page.locator('[data-testid="message-user"][data-queued="true"]');
    await expect(queued).toContainText('And then also check the kiln');
    await expect(page.getByTestId('message-queued')).toHaveText('queued');
    // The send POST ran once — the queued prompt did not go out.
    expect(sendCount).toBe(1);
    await story.step(page, 'queued below the streaming turn');

    await pinCaptureBox(page);
    await expect(page.getByTestId('message-list')).toHaveScreenshot('chat-queued.png', {
      mask: [maskDynamic(page)],
      maxDiffPixelRatio: 0.03,
    });
  });
});
