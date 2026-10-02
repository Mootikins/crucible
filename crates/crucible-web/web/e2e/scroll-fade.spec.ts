import { test, expect, type Locator } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady, openSession } from './helpers/nav';
import { MOCK_SESSION } from './helpers/fixtures';
import { historyOf, segment, userTurn } from '../src/test-utils/transcript';

const prose = Array.from({ length: 80 }, (_, n) => `Paragraph ${n + 1} stays readable while earlier text scrolls under the header.`).join('\n\n');

async function checkEdges(scroller: Locator) {
  await expect(scroller).toBeVisible();
  await scroller.evaluate(el => { el.scrollTop = 0; el.dispatchEvent(new Event('scroll')); });
  await expect(scroller).toHaveAttribute('data-fade-y', 'end');
  await scroller.evaluate(el => { el.scrollTop = (el.scrollHeight - el.clientHeight) / 2; el.dispatchEvent(new Event('scroll')); });
  await expect(scroller).toHaveAttribute('data-fade-y', 'both');
  expect(await scroller.evaluate(el => getComputedStyle(el).maskImage)).toContain('linear-gradient');
  await scroller.evaluate(el => { el.scrollTop = el.scrollHeight; el.dispatchEvent(new Event('scroll')); });
  await expect(scroller).toHaveAttribute('data-fade-y', 'start');
}

test('transcripts fade only the edges that hide messages', async ({ page }) => {
  await setupBasicMocks(page, { sessionHistory: historyOf(MOCK_SESSION.session_id, [
    userTurn('fade-turn', 'Read the transcript'), segment('fade-turn', 0, prose),
  ]) });
  await page.goto('/');
  await openSession(page, MOCK_SESSION.session_id);
  await expect(page.getByTestId('message-assistant')).toContainText('Paragraph 80');
  await checkEdges(page.getByTestId('message-list'));
});

test('reading, source and live notes fade below their breadcrumb', async ({ page }) => {
  await setupBasicMocks(page);
  await page.route('**/api/kiln/file?*', route => route.fulfill({ json: { content: '# Long note\n\n' + prose, content_hash: 'fade-note' } }));
  await page.goto('/');
  await appReady(page);
  await page.evaluate(async () => {
    const { openFileInEditor } = await import('/src/lib/file-actions.ts');
    openFileInEditor('/home/user/notes/Long note.md');
  });
  for (const mode of ['Reading view', 'Source', 'Live preview']) {
    await page.getByRole('button', { name: mode, exact: true }).click();
    await checkEdges(mode === 'Reading view' ? page.getByTestId('markdown-preview') : page.locator('.note-editor .cm-scroller'));
  }
});


test('file breadcrumbs use registered roots without exposing absolute prefixes', async ({ page }) => {
  await setupBasicMocks(page);
  await page.route('**/api/kiln/file?*', route => route.fulfill({ json: { content: '# Guide', content_hash: 'breadcrumb-note' } }));
  await page.goto('/');
  await appReady(page);
  for (const [path, breadcrumb] of [
    ['/home/user/project/docs/Guide.md', 'project/docs/Guide.md'],
    ['/home/user/notes/Guide.md', 'notes/Guide.md'],
    ['/home/user/project-other/Guide.md', 'Guide.md'],
  ]) {
    await page.evaluate(async (path) => {
      const { openFileInEditor } = await import('/src/lib/file-actions.ts');
      openFileInEditor(path);
    }, path);
    await expect(page.getByTestId('note-breadcrumb').last()).toHaveText(breadcrumb);
  }
});
