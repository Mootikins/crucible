import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from '../helpers/mock-api';
import { createSSEStream } from '../helpers/mock-sse';
import { createStory } from './_helpers/story';

/**
 * Story: wikilink hover previews in chat messages.
 *
 * Full app (real ChatContext + Message renderer + WikilinkHoverPreview).
 * An assistant turn containing `[[Kiln Note]]` streams in; the rendered
 * anchor previews on hover and opens the note in the editor on click.
 *
 * Validated behaviors:
 *  1. `[[Kiln Note]]` renders as a .wikilink anchor with data-note.
 *  2. Hovering it floats the preview card: note title, path, and a rendered
 *     markdown excerpt of the note content (visual baseline).
 *  3. Hovering away dismisses the card.
 *  4. Clicking the anchor opens the note in an editor tab.
 */

const KILN = '/home/user/.crucible/kiln';
const NOTE_CONTENT = '# Kiln Note\n\nStored knowledge that grounds the agent.\n';

type Frame = { type: string; data: object };

const STREAM: Frame[] = [
  { type: 'token', data: { type: 'token', content: 'See [[Kiln Note]] for the details.' } },
  {
    type: 'message_complete',
    data: {
      type: 'message_complete',
      id: 'msg-1',
      content: 'See [[Kiln Note]] for the details.',
      prompt_tokens: 10,
      completion_tokens: 10,
      total_tokens: 20,
    },
  },
];

async function setupNoteRoutes(page: Page) {
  await page.route('**/api/notes/**', (route) => {
    const name = decodeURIComponent(
      new URL(route.request().url()).pathname.replace('/api/notes/', ''),
    );
    if (name.toLowerCase() !== 'kiln note') {
      return route.fulfill({ status: 404, body: 'not found' });
    }
    return route.fulfill({
      json: {
        name: 'Kiln Note',
        path: `${KILN}/Kiln Note.md`,
        title: 'Kiln Note',
        tags: [],
        updated_at: '2026-01-01T00:00:00Z',
      },
    });
  });
  await page.route('**/api/kiln/file**', (route) =>
    route.fulfill({ json: { content: NOTE_CONTENT } }),
  );
}

async function streamTurn(page: Page) {
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
        body: createSSEStream(STREAM),
      });
    }
    await new Promise(() => {});
  });
}

test.describe('Chat wikilink hover previews', () => {
  test('anchor renders, previews on hover, dismisses, and opens on click', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupBasicMocks(page, { sseEvents: [] });
    await setupNoteRoutes(page);
    await streamTurn(page);

    await page.goto('/');
    await page.getByTestId('session-item-test-session-001').click();
    await expect(page.getByTestId('chat-input')).toBeEnabled({ timeout: 5000 });

    await page.getByTestId('chat-input').fill('Where is that written down?');
    await page.getByTestId('send-button').click();

    // 1. The wikilink anchor rendered inside the assistant message.
    const anchor = page.locator('a.wikilink[data-note="Kiln Note"]');
    await expect(anchor).toBeVisible({ timeout: 10000 });
    await expect(anchor).toHaveText('Kiln Note');
    await story.step(page, 'wikilink in assistant message');

    // 2. Hover → preview card with title, path, and rendered excerpt.
    await anchor.hover();
    const preview = page.getByTestId('wikilink-preview');
    await expect(preview).toBeVisible({ timeout: 5000 });
    await expect(preview.getByTestId('wikilink-preview-title')).toContainText('Kiln Note');
    await expect(preview.getByTestId('wikilink-preview-title')).toContainText('Kiln Note.md');
    // The excerpt is rendered markdown: the H1 became a heading, body text intact.
    await expect(preview.getByTestId('wikilink-preview-body').locator('h1')).toHaveText(
      'Kiln Note',
    );
    await expect(preview.getByTestId('wikilink-preview-body')).toContainText(
      'Stored knowledge that grounds the agent.',
    );
    await story.step(page, 'hover preview open');
    await expect(preview).toHaveScreenshot('chat-wikilink-hover-preview.png', {
      maxDiffPixelRatio: 0.02,
    });

    // 3. Hovering away dismisses the card.
    await page.getByTestId('chat-input').hover();
    await expect(preview).toHaveCount(0);
    await story.step(page, 'preview dismissed');

    // 4. Clicking the anchor opens the note in an editor (file) tab.
    await anchor.click();
    const fileTab = page.locator('[data-tab-id^="tab-file-"]');
    await expect(fileTab).toBeVisible({ timeout: 5000 });
    await expect(fileTab).toContainText('Kiln Note');
    await story.step(page, 'note opened in editor');
  });
});
