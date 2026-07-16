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
  await page.waitForTimeout(500);
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
  const chatTab = page.locator('[data-tab-id^="tab-chat-"]').first();
  await chatTab.waitFor({ state: 'visible', timeout: 3000 });
  const box = await chatTab.boundingBox();
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
    const thirdTab = page.locator('[data-testid="edge-tab-left-search-tab"]');

    const initialOrder = await getEdgeTabOrder(page, 'left');
    expect(initialOrder.indexOf('sessions-tab')).toBeLessThan(initialOrder.indexOf('search-tab'));
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
    await page.waitForTimeout(300);

    const newOrder = await getEdgeTabOrder(page, 'left');
    expect(newOrder.indexOf('sessions-tab')).toBeGreaterThan(initialSessionsIndex);
  });

  test('reorder edge tab: drag last tab to first position', async ({ page }) => {
    const lastTab = page.locator('[data-testid="edge-tab-left-source-control-tab"]');
    const firstTab = page.locator('[data-testid="edge-tab-left-search-tab"]');

    const initialOrder = await getEdgeTabOrder(page, 'left');
    expect(initialOrder.indexOf('source-control-tab')).toBeGreaterThan(initialOrder.indexOf('search-tab'));

    await lastTab.scrollIntoViewIfNeeded();
    await page.waitForTimeout(100);

    const from = await getCenterOf(page, lastTab);
    const firstBox = await firstTab.boundingBox();
    expect(firstBox).toBeTruthy();
    // The strip overflows and scrollIntoViewIfNeeded(lastTab) can scroll the
    // first tab's box left of the bar — clamp the drop point inside the bar
    // (same reasoning as the "past third tab" test above), otherwise the
    // reorder is cancelled as an out-of-bounds release.
    const barBox = await page.locator('[data-testid="edge-tabbar-left"]').boundingBox();
    expect(barBox).toBeTruthy();
    const to = {
      x: Math.max(firstBox!.x + 2, barBox!.x + 8),
      y: firstBox!.y + firstBox!.height / 2,
    };

    await pointerDrag(page, from, to, 25);
    await page.waitForTimeout(500);

    const newOrder = await getEdgeTabOrder(page, 'left');
    expect(newOrder.indexOf('source-control-tab')).toBeLessThan(newOrder.indexOf('search-tab'));
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
    await page.waitForTimeout(300);

    const newOrder = await getEdgeTabOrder(page, 'left');
    expect(newOrder.indexOf(initialOrder[0]!)).toBeGreaterThan(newOrder.indexOf(initialOrder[1]!));
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
    await page.waitForTimeout(800);

    const indicator = page.locator('[class*="bg-primary"][class*="rounded-full"][class*="h-5"]');
    const indicatorCount = await indicator.count();
    expect(indicatorCount).toBeGreaterThanOrEqual(1);

    await page.mouse.up();
    await page.waitForTimeout(200);

    const postDragCount = await page.locator('[class*="bg-primary"][class*="rounded-full"][class*="h-5"]').count();
    expect(postDragCount).toBe(0);
  });

  test('no insert indicator during cross-zone drag', async ({ page }) => {
    const edgeTab = page.locator('[data-testid="edge-tab-left-explorer-tab"]');
    const centerTab = page.locator('[data-tab-id^="tab-chat-"]').first();

    const from = await getCenterOf(page, edgeTab);
    const to = await getCenterOf(page, centerTab);

    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 10 });
    await page.waitForTimeout(200);

    // The reorder indicator is the 2px×20px bar from TabBar — the header's
    // active mode pill is also bg-primary+rounded-full, so match the size.
    const insertIndicators = page.locator('[class*="w-0.5"][class*="h-5"][class*="bg-primary"]');
    const count = await insertIndicators.count();
    expect(count).toBe(0);

    await page.mouse.up();
  });

  test('cross-zone DnD still works after reorder implementation (regression)', async ({ page }) => {
    const from = await getCenter(page, '[data-testid="edge-tab-left-explorer-tab"]');
    const to = await getCenterPaneDropPoint(page);

    await pointerDrag(page, from, to);
    await page.waitForTimeout(300);

    await expect(page.locator('[data-testid="edge-tab-left-explorer-tab"]')).not.toBeVisible({ timeout: 2000 });
    await expect(page.locator('[data-tab-id="explorer-tab"]:not([data-testid^="edge-tab-"])')).toBeVisible({ timeout: 2000 });
  });
});
