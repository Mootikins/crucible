import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { busEmit } from './helpers/bus';
import { MOCK_SESSION } from './helpers/fixtures';
import { userTurn, segment } from '../src/test-utils/transcript';

for (const width of [320, 390]) {
  test(`phone styling keeps the dirty editor usable at ${width}px`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width, height: 844 });
    await setupBasicMocks(page);
    await page.addInitScript(() => localStorage.setItem('crucible:settings', JSON.stringify({ version: 2, editor: { vimModeCompact: false, autosaveSeconds: 0 } })));
    await page.route('**/api/kiln/file**', route => route.fulfill({ json: { content: '# Mobile note\n\nA paragraph for editing.\n', content_hash: 'original' } }));
    await page.goto('/');
    await expect(page.getByTestId('mobile-shell')).toBeVisible();
    await busEmit(page, 'openFile', { path: '/notes/Mobile note.md', name: 'Mobile note.md' });
    await page.getByRole('button', { name: 'Write', exact: true }).click();
    await page.locator('.cm-content').click();
    await page.keyboard.press('ControlOrMeta+End');
    await page.keyboard.type('Unsaved text');
    const save = page.getByRole('button', { name: 'Save', exact: true });
    await expect(save).toBeVisible();
    const header = page.getByTestId('mobile-shell').locator('header').first();
    const title = header.locator('h1');
    expect((await title.boundingBox())!.width).toBeGreaterThanOrEqual(80);
    const headerBox = (await header.boundingBox())!;
    expect((await save.boundingBox())!.y).toBeGreaterThanOrEqual(headerBox.y + headerBox.height);
    for (const button of [save, page.getByRole('button', { name: 'Read', exact: true }), page.getByRole('button', { name: /^Tabs/ })]) {
      const box = (await button.boundingBox())!;
      expect(box.width).toBeGreaterThanOrEqual(44);
      expect(box.height).toBeGreaterThanOrEqual(44);
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(width);
    }
    await page.getByRole('button', { name: 'Read', exact: true }).click();
    await expect(page.getByTestId('markdown-preview')).toContainText('Unsaved text');
    await page.getByRole('button', { name: 'Write', exact: true }).click();
    await expect(page.locator('.cm-content')).toContainText('Unsaved text');
    for (const theme of ['dark', 'light']) {
      await page.evaluate(theme => { document.documentElement.dataset.theme = theme; document.documentElement.style.setProperty('--mk-radius', '18px'); }, theme);
      await expect(page.locator('.compact-surface')).toHaveCSS('border-top-left-radius', '18px');
      await page.screenshot({ path: testInfo.outputPath(`editor-${width}-${theme}.png`) });
    }
    await page.getByRole('button', { name: 'Sessions and files', exact: true }).click();
    const drawer = page.getByTestId('drawer-left');
    await expect(drawer).toHaveCSS('box-shadow', 'none');
    await page.getByRole('tab', { name: 'Files', exact: true }).click();
    await expect(page.getByRole('tab', { name: 'Files', exact: true })).toHaveCSS('border-bottom-width', '0px');
    await page.screenshot({ path: testInfo.outputPath(`drawer-${width}.png`) });
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'More', exact: true }).click();
    await expect(page.getByTestId('bottom-sheet')).toHaveCSS('border-top-left-radius', '18px');
    await expect(page.getByTestId('bottom-sheet')).toHaveCSS('box-shadow', 'none');
    await page.screenshot({ path: testInfo.outputPath(`menu-${width}.png`) });
  });
}


test('phone chat shares transcript metrics and keeps composer controls touch-sized', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await setupBasicMocks(page, { sessionHistory: {
    session_id: MOCK_SESSION.session_id, history: [], total_events: 2,
    transcript: { as_of_seq: 2, items: [userTurn('turn', 'Keep my history.'), segment('turn', 0, 'Your transcript is preserved.')] },
  } });
  await page.goto('/');
  await expect(page.getByTestId('mobile-shell')).toBeVisible();
  await busEmit(page, 'openSession', { sessionId: MOCK_SESSION.session_id, title: 'Test session' });
  await expect(page.getByText('Your transcript is preserved.', { exact: true })).toBeVisible();
  const copy = page.getByRole('button', { name: 'Copy response', exact: true });
  const mode = page.getByTestId('chat-mode');
  await expect(copy).toHaveCSS('height', '22px');
  for (const control of [mode, page.getByRole('button', { name: 'Session actions', exact: true })]) {
    await expect(control).toBeVisible();
    const box = (await control.boundingBox())!;
    expect(box.width).toBeGreaterThanOrEqual(44);
    expect(box.height).toBeGreaterThanOrEqual(44);
  }
  await expect(page.getByTestId('chat-input')).toHaveCSS('font-size', '16px');
  await mode.click();
  await page.keyboard.press('Escape');
  for (const theme of ['dark', 'light']) {
    await page.evaluate(theme => { document.documentElement.dataset.theme = theme; }, theme);
    await page.screenshot({ path: testInfo.outputPath(`chat-390-${theme}.png`) });
  }
  // A short visual viewport approximates the space left by a phone keyboard.
  await page.setViewportSize({ width: 390, height: 420 });
  const send = page.getByTestId('send-button');
  const sendBox = (await send.boundingBox())!;
  expect(sendBox.height).toBeGreaterThanOrEqual(44);
  expect(sendBox.width).toBeGreaterThanOrEqual(44);
  expect(sendBox.y + sendBox.height).toBeLessThanOrEqual(420);
  await page.screenshot({ path: testInfo.outputPath('chat-short-viewport.png') });
});

test('drawer leaf tabs round the exposed panel corner and inside join', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await setupBasicMocks(page);
  await page.goto('/');
  await page.getByRole('button', { name: 'Sessions and files', exact: true }).click();
  await page.evaluate(() => document.documentElement.style.setProperty('--mk-radius', '18px'));
  for (const name of ['Sessions', 'Files']) {
    const tab = page.getByRole('tab', { name, exact: true });
    await tab.click();
    const panel = page.getByRole('tabpanel', { name, exact: true });
    await expect(panel).toHaveCSS(name === 'Sessions' ? 'border-top-right-radius' : 'border-top-left-radius', '18px');
    const join = await tab.evaluate(el => {
      const s = getComputedStyle(el, el === el.parentElement!.firstElementChild ? '::after' : '::before');
      return { content: s.content, background: s.backgroundImage, width: s.width };
    });
    expect(join.content).not.toBe('none');
    expect(join.background).toContain('radial-gradient');
    expect(join.width).toBe('18px');
    for (const theme of ['light', 'dark']) {
      await page.evaluate(t => { document.documentElement.dataset.theme = t; }, theme);
      await page.screenshot({ animations: 'disabled', path: testInfo.outputPath(`drawer-${name}-${theme}.png`) });
    }
  }
});
