import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady } from './helpers/nav';
import { busEmit } from './helpers/bus';

test('note navigation preserves dirty buffers, undo, hashes and explicit new tabs', async ({ page }) => {
  await setupBasicMocks(page);
  await page.addInitScript(() => localStorage.setItem('crucible:settings', JSON.stringify({ version: 2, editor: { vimMode: false, autosaveSeconds: 0 } })));
  await page.route('**/api/rpc/kiln.list', route => route.fulfill({ json: [{ path: '/notes', name: 'notes' }] }));
  const bodies: Record<string, string> = { '/notes/A.md': '# A\n\n[[B]]\n', '/notes/B.md': '# B\n\n[[A]]\n' };
  await page.route('**/api/kiln/file**', async route => {
    if (route.request().method() === 'GET') {
      const path = new URL(route.request().url()).searchParams.get('path')!;
      return route.fulfill({ json: { content: bodies[path], content_hash: `base-${path}` } });
    }
    return route.fulfill({ json: { content_hash: 'saved' } });
  });
  await page.route('**/api/notes/resolve**', route => {
    const name = new URL(route.request().url()).searchParams.get('name')!;
    return route.fulfill({ json: { path: `${name}.md`, absolutePath: `/notes/${name}.md`, title: name } });
  });
  await page.goto('/');
  await appReady(page);
  await busEmit(page, 'openFile', { path: '/notes/A.md', name: 'A.md' });
  const editor = page.locator('.note-editor .cm-content');
  await expect(editor).toContainText('A');
  await editor.click();
  await page.keyboard.press('ControlOrMeta+End');
  await page.keyboard.type('unsaved A');
  await page.getByRole('button', { name: 'Reading view', exact: true }).click();
  await page.locator('.note-editor [data-note="B"]').click();
  await expect(page.locator('[data-content-type="file"].wm-tab')).toHaveCount(1);
  await expect(editor).toContainText('B');
  await page.getByRole('button', { name: 'Back (Alt+Left)', exact: true }).click();
  await expect(editor).toContainText('unsaved A');
  await editor.click();
  await page.keyboard.press('ControlOrMeta+z');
  await expect(editor).not.toContainText('unsaved A');
  await page.keyboard.press('ControlOrMeta+y');
  await expect(editor).toContainText('unsaved A');
  const save = page.waitForRequest(request => request.url().includes('/api/kiln/file') && request.method() === 'PUT');
  await page.keyboard.press('ControlOrMeta+s');
  expect((await save).postDataJSON()).toMatchObject({ path: '/notes/A.md', base_hash: 'base-/notes/A.md' });
  await page.getByRole('button', { name: 'Forward (Alt+Right)', exact: true }).click();
  await expect(editor).toContainText('B');
  await page.getByRole('button', { name: 'Reading view', exact: true }).click();
  await page.locator('.note-editor [data-note="A"]').click({ modifiers: ['ControlOrMeta'] });
  await expect(page.locator('[data-content-type="file"].wm-tab')).toHaveCount(2);
  const ids = await page.locator('[data-content-type="file"].wm-tab').evaluateAll(tabs => tabs.map(tab => tab.getAttribute('data-tab-id')));
  expect(new Set(ids).size).toBe(2);
});
