import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from '../helpers/mock-api';
import { createSSEStream } from '../helpers/mock-sse';
import { createStory } from './_helpers/story';
import { waitForFonts } from './_helpers/fonts';

/**
 * Story: wikilink hover popovers in chat messages (Hover Editor pattern).
 *
 * Full app (real ChatContext + Message renderer + WikilinkHoverPreview).
 * An assistant turn containing `[[Kiln Note]]` streams in; hovering the
 * rendered anchor spawns a TRANSIENT FloatingWindow — the same window as
 * pop-outs — holding a real editor of the note.
 *
 * Validated behaviors:
 *  1. `[[Kiln Note]]` renders as a .wikilink anchor with data-note.
 *  2. Hovering it spawns a floating editor window titled with the note,
 *     rendering its content in live preview (visual baseline).
 *  3. Hovering away closes the popover window.
 *  4. Pinning (titlebar pin) keeps it open through hover-away.
 *  5. Clicking the anchor opens the note in an editor tab.
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
    await waitForFonts(page);
    await page.getByTestId('session-item-test-session-001').click();
    await expect(page.getByTestId('chat-input')).toBeEnabled({ timeout: 5000 });

    await page.getByTestId('chat-input').fill('Where is that written down?');
    await page.getByTestId('send-button').click();

    // 1. The wikilink anchor rendered inside the assistant message.
    const anchor = page.locator('a.wikilink[data-note="Kiln Note"]');
    await expect(anchor).toBeVisible({ timeout: 10000 });
    await expect(anchor).toHaveText('Kiln Note');
    await story.step(page, 'wikilink in assistant message');

    // 2. Hover → a transient floating window titled with the note, opening
    // in the configured hover mode (default: rendered reading view).
    await anchor.hover();
    const popover = page.locator('[data-window-id]');
    await expect(popover).toBeVisible({ timeout: 5000 });
    await expect(popover).toContainText('Kiln Note');
    const reading = popover.getByTestId('markdown-preview');
    await expect(reading.locator('h1')).toHaveText('Kiln Note', { timeout: 5000 });
    await expect(reading).toContainText('Stored knowledge that grounds the agent.');
    await expect(popover.getByTestId('float-pin')).toBeVisible();
    await story.step(page, 'hover popover open');
    await expect(popover).toHaveScreenshot('chat-wikilink-hover-preview.png');

    // 3. Hovering away closes the popover.
    await page.getByTestId('chat-input').hover();
    await expect(popover).toHaveCount(0, { timeout: 5000 });
    await story.step(page, 'popover dismissed');

    // 4. Pinning keeps it open through hover-away: the pin control leaves
    // the titlebar (the window is no longer transient — the unit suite pins
    // the survives-hover-away behavior) and the window stays visible.
    await anchor.hover();
    await expect(popover).toBeVisible({ timeout: 5000 });
    await popover.getByTestId('float-pin').click();
    await expect(popover.getByTestId('float-pin')).toHaveCount(0);
    await page.getByTestId('chat-input').hover();
    await expect(popover).toBeVisible();
    await story.step(page, 'popover pinned');
    // Clean up the pinned window so the click-through step starts fresh.
    await popover.locator('[title="Close (closes its tabs)"]').click();
    await expect(popover).toHaveCount(0);

    // 4. Clicking the anchor opens the note in an editor (file) tab.
    await anchor.click();
    const fileTab = page.locator('[data-tab-id^="tab-file-"]');
    await expect(fileTab).toBeVisible({ timeout: 5000 });
    await expect(fileTab).toContainText('Kiln Note');
    await story.step(page, 'note opened in editor');
  });
});
