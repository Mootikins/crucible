import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady, openSession } from './helpers/nav';
import { busEmit } from './helpers/bus';
import { MOCK_SESSION } from './helpers/fixtures';
import { userTurn, segment } from '../src/test-utils/transcript';

test('appearance persists while the real editor and transcript keep their state', async ({ page }, testInfo) => {
  await setupBasicMocks(page, { sessionHistory: {
    session_id: MOCK_SESSION.session_id, history: [], total_events: 2,
    transcript: { as_of_seq: 2, items: [userTurn('turn', 'Keep my history.'), segment('turn', 0, 'Your transcript is preserved.')] },
  } });
  await page.addInitScript(() => {
    if (!localStorage.getItem('crucible:settings')) localStorage.setItem('crucible:settings', JSON.stringify({ version: 2, editor: { vimMode: false, autosaveSeconds: 0 } }));
  });
  await page.route('**/api/kiln/file**', (route) => route.fulfill({ json: route.request().method() === 'GET' ? { content: '# Draft\n\nKeep this paragraph.\n', content_hash: 'initial' } : { ok: true, content_hash: 'saved' } }));
  await page.goto('/');
  await appReady(page);
  await openSession(page, MOCK_SESSION.session_id);
  await expect(page.getByText('Your transcript is preserved.', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Copy response', exact: true })).toHaveCount(1);
  await busEmit(page, 'openFile', { path: '/home/user/notes/draft.md', name: 'draft.md' });
  const editor = page.locator('.note-editor .cm-editor');
  await expect(editor.locator('.cm-lineNumbers')).toHaveCount(0);
  await editor.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+End');
  await page.keyboard.type('Unsaved text');
  const original = await editor.elementHandle();
  await page.getByTestId('ribbon-cmd-settings').click();
  await page.getByTestId('settings-nav-appearance').click();
  await page.getByLabel('True black', { exact: true }).check();
  await page.getByLabel('Gap', { exact: true }).fill('14');
  await page.getByLabel('Corner radius', { exact: true }).fill('18');
  await page.getByLabel('Note text size', { exact: true }).fill('19');
  await page.getByLabel('Accent', { exact: true }).fill('#bb77dd');
  await page.getByLabel('File labels', { exact: true }).uncheck();
  await page.getByTestId('settings-modal-close').click();
  expect(await original!.evaluate((element) => element.isConnected)).toBe(true);
  await expect(editor.locator('.cm-content')).toContainText('Unsaved text');
  await expect(editor.locator('.cm-content')).toHaveCSS('font-size', '19px');
  await expect(page.getByText('Your transcript is preserved.', { exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('migrated-editor-transcript.png') });
  await editor.locator('.cm-content').click();
  await page.keyboard.press('ControlOrMeta+z');
  await expect(editor.locator('.cm-content')).not.toContainText('Unsaved text');
  await page.reload();
  await appReady(page);
  await page.getByTestId('ribbon-cmd-settings').click();
  await page.getByTestId('settings-nav-appearance').click();
  await expect(page.getByLabel('Gap', { exact: true })).toHaveValue('14');
  await expect(page.getByLabel('Note text size', { exact: true })).toHaveValue('19');
  await expect(page.getByLabel('Accent', { exact: true })).toHaveValue('#bb77dd');
  await expect(page.getByLabel('File labels', { exact: true })).not.toBeChecked();
  await page.getByLabel('Theme', { exact: true }).selectOption('light');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
});
