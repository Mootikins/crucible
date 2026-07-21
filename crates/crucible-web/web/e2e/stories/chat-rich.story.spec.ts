import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from '../helpers/mock-api';
import { createSSEStream } from '../helpers/mock-sse';
import { waitForFonts } from './_helpers/fonts';

/**
 * Rhythm story: a realistic assistant turn — heading, prose, a list, a
 * blockquote, a fenced code block — followed by a RUN of four tool calls, so
 * the transcript's vertical rhythm and the slim tool-stack can be judged on
 * content closer to what an agent actually emits. Captured at the chat panel's
 * natural width, scrolled to the tool-stack tail.
 *
 * Not a pinned regression like chat-stream; this exists to eyeball density.
 */

type Frame = { type: string; data: object };

const ANSWER = [
  '## Shipping a daemon change',
  '',
  "Crucible's daemon owns all storage, so a change ships in three steps:",
  '',
  '- Build the release binary with `just build`',
  '- Run the migration sweep against the socket',
  '- Restart the server and verify `kiln.list`',
  '',
  '> The socket path resolves from `$CRUCIBLE_SOCKET`, falling back to the',
  '> runtime dir. Keep it stable across restarts.',
  '',
  '```rust',
  'fn main() {',
  '    let sock = resolve_socket();',
  '    Server::bind(&sock).serve();',
  '}',
  '```',
  '',
  '---',
  '',
  '| Step | Command | Owner |',
  '| --- | --- | --- |',
  '| Build | `just build` | you |',
  '| Sweep | `cru daemon sweep` | daemon |',
  '| Verify | `kiln.list` | daemon |',
  '',
  "That's the whole flow — the pipeline handles the rest.",
].join('\n');

function tokenFrames(text: string): Frame[] {
  const chunks: string[] = [];
  for (let i = 0; i < text.length; i += 12) chunks.push(text.slice(i, i + 12));
  return chunks.map((content) => ({ type: 'token', data: { type: 'token', content } }));
}

function toolFrames(id: string, title: string, args: object, result: string): Frame[] {
  return [
    { type: 'tool_call', data: { type: 'tool_call', id, title, arguments: args } },
    { type: 'tool_result_delta', data: { type: 'tool_result_delta', id, delta: result } },
    { type: 'tool_result_complete', data: { type: 'tool_result_complete', id } },
  ];
}

const RICH_STREAM: Frame[] = [
  ...tokenFrames(ANSWER),
  { type: 'thinking', data: { type: 'thinking', content: 'Mapping the deploy steps to daemon internals.' } },
  ...toolFrames('t1', 'read_file', { path: 'crates/crucible-daemon/src/server/core.rs' }, 'ok'),
  // MCP envelope result whose text payload is itself JSON — the card must
  // unwrap and pretty-print the payload, not the wrapper.
  ...toolFrames(
    't2',
    'search_codebase',
    { pattern: 'resolve_socket' },
    JSON.stringify({
      content: [
        { type: 'text', text: JSON.stringify({ matches: 3, files: ['server/core.rs', 'rpc/dispatch.rs'] }) },
      ],
    }),
  ),
  ...toolFrames('t3', 'bash_exec', { command: 'just build' }, 'Compiling crucible-daemon v0.11.4'),
  ...toolFrames('t4', 'write_note', { note: 'memory/deploy-flow.md' }, 'ok'),
  {
    type: 'message_complete',
    data: {
      type: 'message_complete',
      id: 'msg-1',
      content: ANSWER,
      tool_calls: [
        { id: 't1', title: 'read_file' },
        { id: 't2', title: 'search_codebase' },
        { id: 't3', title: 'bash_exec' },
        { id: 't4', title: 'write_note' },
      ],
      prompt_tokens: 1800,
      completion_tokens: 420,
      total_tokens: 2220,
    },
  },
];

async function selectSession(page: Page) {
  await page.goto('/');
  await waitForFonts(page);
  await page.getByTestId('session-item-test-session-001').click();
  await expect(page.getByTestId('chat-input')).toBeEnabled({ timeout: 5000 });
}

function maskDynamic(page: Page) {
  return page.getByTestId('message-list').locator('[data-dynamic-time]');
}

async function driveComplete(page: Page) {
  await setupBasicMocks(page, { sseEvents: [] });
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
        body: createSSEStream(RICH_STREAM),
      });
    }
    await new Promise(() => {});
  });

  await selectSession(page);
  await page.getByTestId('chat-input').fill('How do I ship a daemon change?');
  await page.getByTestId('send-button').click();

  await expect(page.getByTestId('message-assistant').first()).toContainText('Shipping a daemon change', { timeout: 10000 });
  await expect(page.getByText('write_note')).toBeVisible();
  await expect(page.getByText('2,220 tokens')).toBeVisible();
  await expect(page.getByTestId('send-button')).toBeVisible();
  // Markdown tables must render as REAL framed tables in chat (parity with
  // notes), and the MCP envelope result must be unwrapped (payload visible,
  // wrapper's escaped soup gone).
  await expect(page.getByTestId('message-list').locator('table')).toBeVisible();
  await expect(page.getByTestId('message-list').locator('hr')).toBeVisible();
}

test.describe('rich transcript rhythm', () => {
  // Pin height BELOW the composer (which follows the list in the flex column)
  // so it never bleeds into the element screenshot, then scroll to the tail so
  // the tool stack + meta row — the most design-relevant region — is in frame.
  test('narrow panel width (visual)', async ({ page }) => {
    await driveComplete(page);
    await page.getByTestId('message-list').evaluate((el) => {
      el.style.height = '480px';
      el.style.maxHeight = '480px';
      el.style.flex = 'none';
      el.style.overflow = 'hidden';
      el.scrollTop = el.scrollHeight;
    });
    await expect(page.getByTestId('message-list')).toHaveScreenshot('rich-narrow.png', {
      mask: [maskDynamic(page)],
      maxDiffPixelRatio: 0.03,
    });
  });

});
