import { test, expect } from '@playwright/test';
import { setupBasicMocks } from '../helpers/mock-api';
import { createStory } from './_helpers/story';

/**
 * Story: WS-109 — export a session as markdown from the browser.
 *
 * The ExportDialog fetches POST /api/session/:id/export (markdown text),
 * previews the first 50 lines, and Download writes a Blob to a file whose name
 * is derived from the session title + date. We assert the downloaded content
 * matches the export endpoint response.
 */

const EXPORT_MD = [
  '# Test Session',
  '',
  '**user:** hello',
  '',
  '**assistant:** hi there',
  '',
  ...Array.from({ length: 60 }, (_, i) => `line ${i}`),
].join('\n');

test.describe('WS-109 export a session', () => {
  test('preview renders and Download matches /export content', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupBasicMocks(page, { sseEvents: [] });
    await page.route('**/api/session/*/export', (route) =>
      route.fulfill({ status: 200, contentType: 'text/plain', body: EXPORT_MD }),
    );

    await page.goto('/');
    await page.getByTestId('session-item-test-session-001').click();
    await expect(page.getByTestId('chat-input')).toBeEnabled({ timeout: 5000 });

    // Open the export dialog (same event the command palette dispatches).
    await page.evaluate(() => window.dispatchEvent(new CustomEvent('crucible:export-session')));

    await expect(page.getByRole('heading', { name: 'Export Session' })).toBeVisible({ timeout: 5000 });
    // Preview shows the first lines and a total-lines count.
    await expect(page.getByText('**assistant:** hi there')).toBeVisible();
    await expect(page.getByText(/total lines/)).toBeVisible();
    await story.step(page, 'export dialog with preview');

    // Download and assert the saved bytes match the endpoint response.
    const downloadPromise = page.waitForEvent('download');
    await page.getByRole('button', { name: 'Download' }).click();
    const download = await downloadPromise;

    expect(download.suggestedFilename()).toMatch(/^session-test-session-\d{4}-\d{2}-\d{2}\.md$/);
    const stream = await download.createReadStream();
    const chunks: Buffer[] = [];
    for await (const chunk of stream) chunks.push(chunk as Buffer);
    expect(Buffer.concat(chunks).toString('utf-8')).toBe(EXPORT_MD);
    await story.step(page, 'downloaded');
  });
});
