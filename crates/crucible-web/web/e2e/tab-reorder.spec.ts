import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';


async function waitForApp(page: Page) {
  await setupBasicMocks(page);
  await page.route('**/api/layout', async (route) => {
    const method = route.request().method();
    if (method === 'GET') {
      await route.fulfill({ status: 404, contentType: 'application/json', body: '{}' });
      return;
    }
    if (method === 'POST' || method === 'DELETE') {
      await route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
      return;
    }
    await route.continue();
  });
  await page.goto('/');
  // Readiness is asserted by the beforeEach (session-item visible), which only
  // renders once the app has fully mounted — no fixed settle needed.
}

async function pointerDrag(
  page: Page,
  from: { x: number; y: number },
  to: { x: number; y: number },
  steps = 10,
) {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps });
  await page.mouse.up();
}

async function getCenter(page: Page, selector: string) {
  const loc = page.locator(selector);
  await loc.waitFor({ state: 'visible', timeout: 3000 });
  const box = await loc.boundingBox();
  expect(box).toBeTruthy();
  return { x: box!.x + box!.width / 2, y: box!.y + box!.height / 2 };
}

async function getCenterOf(page: Page, locator: ReturnType<Page['locator']>) {
  await locator.waitFor({ state: 'visible', timeout: 3000 });
  const box = await locator.boundingBox();
  expect(box).toBeTruthy();
  return { x: box!.x + box!.width / 2, y: box!.y + box!.height / 2 };
}

async function getCenterPaneDropPoint(page: Page): Promise<{ x: number; y: number }> {
  // Anchor on a CENTER tab — sessions dock in the right edge panel now.
  const centerTab = page.locator('[data-tab-id]:not([data-testid^="edge-tab-"])').first();
  await centerTab.waitFor({ state: 'visible', timeout: 3000 });
  const box = await centerTab.boundingBox();
  expect(box).toBeTruthy();
  return { x: box!.x + box!.width / 2, y: box!.y + 100 };
}

async function getEdgeTabOrder(page: Page, position: string): Promise<string[]> {
  return page.locator(`[data-testid="edge-tabbar-${position}"] [data-tab-id]`).evaluateAll(
    (els) => els.map((el) => el.getAttribute('data-tab-id') ?? ''),
  );
}

test.describe('Tab reorder within same bar', () => {
  test.beforeEach(async ({ page }) => {
    await waitForApp(page);
    // Open a session to create a chat tab in the center pane for cross-zone tests
    const sessionItem = page.getByTestId('session-item-test-session-001');
    await expect(sessionItem).toBeVisible({ timeout: 5000 });
    await sessionItem.click();
    // Wait for the chat tab to appear
    await expect(page.locator('[data-tab-id^="tab-chat-"]')).toBeVisible({ timeout: 5000 });
  });

  test('reorder edge tab: drag first tab past third tab', async ({ page }) => {
    const firstTab = page.locator('[data-testid="edge-tab-left-sessions-tab"]');
    const thirdTab = page.locator('[data-testid="edge-tab-left-files-tab"]');

    const initialOrder = await getEdgeTabOrder(page, 'left');
    expect(initialOrder.indexOf('sessions-tab')).toBeLessThan(initialOrder.indexOf('files-tab'));
    const initialSessionsIndex = initialOrder.indexOf('sessions-tab');

    const from = await getCenterOf(page, firstTab);
    const thirdBox = await thirdTab.boundingBox();
    expect(thirdBox).toBeTruthy();
    // Drop targeting is pointer-based: keep the pointer inside the edge bar
    // (the tab strip overflows the 279px panel, so the third tab's right edge
    // can sit over the CENTER pane's tab bar — releasing there is a
    // legitimate cross-bar move, not this test's reorder).
    const barBox = await page.locator('[data-testid="edge-tabbar-left"]').boundingBox();
    expect(barBox).toBeTruthy();
    const to = {
      x: Math.min(thirdBox!.x + thirdBox!.width - 2, barBox!.x + barBox!.width - 8),
      y: thirdBox!.y + thirdBox!.height / 2,
    };

    await pointerDrag(page, from, to, 20);

    // Poll the live tab order until the reorder lands.
    await expect
      .poll(async () => (await getEdgeTabOrder(page, 'left')).indexOf('sessions-tab'), { timeout: 3000 })
      .toBeGreaterThan(initialSessionsIndex);
  });

  test('reorder edge tab: drag last tab to first position', async ({ page }) => {
    const lastTab = page.locator('[data-testid="edge-tab-left-files-tab"]');
    const firstTab = page.locator('[data-testid="edge-tab-left-sessions-tab"]');

    const initialOrder = await getEdgeTabOrder(page, 'left');
    expect(initialOrder.indexOf('files-tab')).toBeGreaterThan(initialOrder.indexOf('sessions-tab'));

    await lastTab.scrollIntoViewIfNeeded();

    const from = await getCenterOf(page, lastTab);
    const firstBox = await firstTab.boundingBox();
    expect(firstBox).toBeTruthy();
    // The strip overflows and scrollIntoViewIfNeeded(lastTab) can scroll the
    // first tab's box left of the strip — clamp the drop point inside the
    // TAB STRIP container (not the bar: the bar's left edge holds the
    // collapse-button cluster, which sits outside the reorder bounds),
    // otherwise the reorder is cancelled as an out-of-bounds release.
    const stripBox = await firstTab.evaluate((el) => {
      const r = el.parentElement!.getBoundingClientRect();
      return { x: r.x, y: r.y, width: r.width, height: r.height };
    });
    const to = {
      x: Math.max(firstBox!.x + 2, stripBox.x + 8),
      y: firstBox!.y + firstBox!.height / 2,
    };

    await pointerDrag(page, from, to, 25);

    // Poll until files-tab has moved ahead of sessions-tab.
    await expect
      .poll(
        async () => {
          const order = await getEdgeTabOrder(page, 'left');
          return order.indexOf('files-tab') < order.indexOf('sessions-tab');
        },
        { timeout: 3000 },
      )
      .toBe(true);
  });

  test('reorder edge tab within left panel', async ({ page }) => {
    const initialOrder = await getEdgeTabOrder(page, 'left');
    expect(initialOrder.length).toBeGreaterThanOrEqual(2);

    const firstEdgeTab = page.locator(`[data-testid="edge-tab-left-${initialOrder[0]}"]`);
    const secondEdgeTab = page.locator(`[data-testid="edge-tab-left-${initialOrder[1]}"]`);

    const from = await getCenterOf(page, firstEdgeTab);
    const secondBox = await secondEdgeTab.boundingBox();
    expect(secondBox).toBeTruthy();
    const to = { x: secondBox!.x + secondBox!.width - 2, y: secondBox!.y + secondBox!.height / 2 };

    await pointerDrag(page, from, to);

    // Poll until the first tab has moved past the second.
    await expect
      .poll(
        async () => {
          const order = await getEdgeTabOrder(page, 'left');
          return order.indexOf(initialOrder[0]!) > order.indexOf(initialOrder[1]!);
        },
        { timeout: 3000 },
      )
      .toBe(true);
  });

  test('insert indicator appears during edge tab reorder drag', async ({ page }) => {
    const initialOrder = await getEdgeTabOrder(page, 'left');
    expect(initialOrder.length).toBeGreaterThanOrEqual(2);
    const firstTab = page.locator(`[data-testid="edge-tab-left-${initialOrder[0]}"]`);
    const thirdTab = page.locator(`[data-testid="edge-tab-left-${initialOrder[1]}"]`);

    const from = await getCenterOf(page, firstTab);
    const thirdBox = await thirdTab.boundingBox();
    expect(thirdBox).toBeTruthy();
    const to = { x: thirdBox!.x + thirdBox!.width / 2, y: thirdBox!.y + thirdBox!.height / 2 };

    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 20 });

    // Mid-drag, the insert indicator renders — poll for it while the pointer
    // is held rather than sleeping a fixed interval.
    const indicator = page.locator('[class*="bg-primary"][class*="rounded-full"][class*="h-5"]');
    await expect(indicator.first()).toBeVisible({ timeout: 3000 });

    await page.mouse.up();

    // Once the drag ends, the indicator is torn down.
    await expect(page.locator('[class*="bg-primary"][class*="rounded-full"][class*="h-5"]')).toHaveCount(0, { timeout: 3000 });
  });

  test('no insert indicator during cross-zone drag', async ({ page }) => {
    const edgeTab = page.locator('[data-testid="edge-tab-left-files-tab"]');
    const centerTab = page.locator('[data-tab-id^="tab-chat-"]').first();

    const from = await getCenterOf(page, edgeTab);
    const to = await getCenterOf(page, centerTab);

    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 10 });

    // Positive signal that the cross-zone drag is active and past the drag
    // threshold: the drag overlay renders the tab title. Assert on that before
    // checking the reorder indicator is absent (otherwise the absence check is
    // vacuous — it would pass before the drag even engages).
    await expect(page.locator('text="Files"').last()).toBeVisible({ timeout: 3000 });

    // The reorder indicator is the 2px×20px bar from TabBar — the header's
    // active mode pill is also bg-primary+rounded-full, so match the size.
    const insertIndicators = page.locator('[class*="w-0.5"][class*="h-5"][class*="bg-primary"]');
    const count = await insertIndicators.count();
    expect(count).toBe(0);

    await page.mouse.up();
  });

  test('cross-zone DnD still works after reorder implementation (regression)', async ({ page }) => {
    // The center pane starts EMPTY (no landing page) — open a Settings tab
    // so the drop-point helper has a center tab to anchor on.
    await page.evaluate(async () => {
      const { openPanelTab } = await import('/src/lib/panel-actions.ts');
      openPanelTab('settings');
    });
    await page
      .locator('[data-tab-id="tab-settings"]:not([data-testid^="edge-tab-"])')
      .waitFor({ state: 'visible', timeout: 3000 });
    const from = await getCenter(page, '[data-testid="edge-tab-left-files-tab"]');
    const to = await getCenterPaneDropPoint(page);

    await pointerDrag(page, from, to);

    await expect(page.locator('[data-testid="edge-tab-left-files-tab"]')).not.toBeVisible({ timeout: 2000 });
    await expect(page.locator('[data-tab-id="files-tab"]:not([data-testid^="edge-tab-"])')).toBeVisible({ timeout: 2000 });
  });
});
