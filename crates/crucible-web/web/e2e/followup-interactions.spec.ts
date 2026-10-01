import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { busEmit } from './helpers/bus';
import { MOCK_SESSION } from './helpers/fixtures';
import { userTurn, segment } from '../src/test-utils/transcript';
import { openHarness, act, groupIds, tabIds } from './windowing/harness';

test('ribbon reordering shows the actual insertion point', async ({ page }, testInfo) => {
  await openHarness(page);
  const [left] = await groupIds(page, 'left');
  await act(page, 'addTab', left, { id: 'rail-two', title: 'Two', contentType: 'beta' });
  const from = (await page.getByTestId('rail-tab-rail-two').boundingBox())!;
  const to = (await page.getByTestId('rail-tab-tab-left').boundingBox())!;
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(to.x + to.width / 2, to.y + 2, { steps: 12 });
  await expect(page.getByTestId('rail-drop-indicator')).toBeVisible();
  const mark = (await page.getByTestId('rail-drop-indicator').boundingBox())!;
  expect(Math.abs(mark.y + mark.height / 2 - to.y)).toBeLessThanOrEqual(1);
  const ghost = (await page.getByTestId('drag-overlay').boundingBox())!;
  expect(ghost.x).toBeGreaterThan(mark.x + mark.width);
  await page.screenshot({ path: testInfo.outputPath('rail-insertion.png') });
  await page.mouse.up();
  expect(await tabIds(page, 'left')).toEqual(['rail-two', 'tab-left']);
  await expect(page.getByTestId('rail-drop-indicator')).toHaveCount(0);
});

test('touch transcript keeps desktop action metrics and reveals user actions on tap', async ({ browser }) => {
  const context = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });
  const page = await context.newPage();
  await setupBasicMocks(page, { sessionHistory: { session_id: MOCK_SESSION.session_id, history: [], total_events: 2, transcript: { as_of_seq: 2, items: [userTurn('turn', 'My message'), segment('turn', 0, 'The reply')] } } });
  await page.goto('/');
  await busEmit(page, 'openSession', { sessionId: MOCK_SESSION.session_id, title: 'Session' });
  const message = page.getByTestId('message-user');
  const meta = message.getByTestId('turn-meta');
  await expect(meta).toHaveCSS('opacity', '0');
  await message.locator('.user-quote').tap();
  await expect(meta).toHaveCSS('opacity', '1');
  await expect(message.getByRole('button', { name: 'Copy message' })).toHaveCSS('height', '22px');
  await expect(page.getByRole('button', { name: 'Copy response' })).toHaveCSS('height', '22px');
  await context.close();
});

test('center swipes pull only the matching drawer and vertical travel stays scrolling', async ({ browser }) => {
  const context = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });
  const page = await context.newPage();
  await setupBasicMocks(page);
  await page.goto('/');
  await expect(page.getByTestId('mobile-shell')).toBeVisible();
  const cdp = await context.newCDPSession(page);
  const swipe = async (x: number, y: number, endX: number, endY: number) => {
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
    for (let i = 1; i <= 10; i++) await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x: x + (endX-x)*i/10, y: y+(endY-y)*i/10 }] });
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  };
  await swipe(195, 400, 205, 220);
  await expect(page.getByTestId('drawer-left')).toHaveAttribute('inert', '');
  await expect(page.getByTestId('drawer-right')).toHaveAttribute('inert', '');
  await swipe(150, 400, 350, 400);
  await expect(page.getByTestId('drawer-left')).not.toHaveAttribute('inert', '');
  await expect(page.getByTestId('drawer-right')).toHaveAttribute('inert', '');
  await page.goBack();
  await expect(page.getByTestId('drawer-left')).toHaveAttribute('inert', '');
  await swipe(250, 400, 40, 400);
  await expect(page.getByTestId('drawer-right')).not.toHaveAttribute('inert', '');
  await expect(page.getByTestId('drawer-left')).toHaveAttribute('inert', '');
  await context.close();
});

test('right resize hairline is centered between the two content surfaces', async ({ page }, testInfo) => {
  await setupBasicMocks(page);
  await page.goto('/');
  await page.route('**/api/kiln/file**', route => route.fulfill({ json: { content: '# Example', content_hash: 'original' } }));
  await busEmit(page, 'openFile', { path: '/home/user/notes/Example.md', name: 'Example.md' });
  const handle = page.locator('[data-testid="edge-host-right"] .wm-edge-handle');
  await handle.hover();
  const center = page.locator('[data-testid="centre-column"] .wm-pane').first();
  const right = page.locator('[data-testid="edge-host-right"] .wm-pane').first();
  const c = (await center.boundingBox())!;
  const r = (await right.boundingBox())!;
  const h = (await handle.boundingBox())!;
  expect(Math.abs(h.x + h.width / 2 - (c.x + c.width + r.x) / 2)).toBeLessThanOrEqual(1);
  await page.screenshot({ path: testInfo.outputPath('resize-centered.png') });
});
