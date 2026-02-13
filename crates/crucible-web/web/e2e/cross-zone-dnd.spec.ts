import { test, expect, type Page } from '@playwright/test';

type PanelPosition = 'left' | 'right' | 'bottom';

async function waitForApp(page: Page) {
  await page.goto('/');
  await page.waitForTimeout(500);
}

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/**
 * Pointer-based drag: mousedown → mousemove(steps) → mouseup.
 * Required because @thisbeyond/solid-dnd uses pointer events, not HTML5 DnD.
 */
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

test.describe('Cross-zone tab drag and drop', () => {
  test.beforeEach(async ({ page }) => {
    await waitForApp(page);
  });

  test('drag edge tab from expanded left panel to center pane', async ({ page }) => {
    const from = await getCenter(page, '[data-testid="edge-tab-left-search-tab"]');
    const to = await getCenterOf(page, page.locator('[data-tab-id="tab-1"]'));

    await pointerDrag(page, from, to);
    await page.waitForTimeout(300);

    await expect(page.locator('[data-testid="edge-tab-left-search-tab"]')).not.toBeVisible({ timeout: 2000 });
    await expect(page.locator('[data-tab-id="search-tab"]')).toBeVisible({ timeout: 2000 });
  });

  test('drag center tab to left edge panel', async ({ page }) => {
    const centerTab = page.locator('[data-tab-id="tab-3"]');
    const from = await getCenterOf(page, centerTab);
    const to = await getCenter(page, '[data-testid="edge-tabbar-left"]');

    await pointerDrag(page, from, to);
    await page.waitForTimeout(300);

    const edgeTab = page.locator('[data-testid="edge-tab-left-tab-3"]');
    await expect(edgeTab).toBeVisible({ timeout: 2000 });
    const tabInCenter = page.locator('[data-tab-id="tab-3"]:not([data-testid^="edge-tab-"])');
    await expect(tabInCenter).not.toBeVisible({ timeout: 2000 });
  });

  test('drag edge tab from left panel to bottom panel', async ({ page }) => {
    const from = await getCenter(page, '[data-testid="edge-tab-left-git-tab"]');
    const to = await getCenter(page, '[data-testid="edge-tabbar-bottom"]');

    await pointerDrag(page, from, to);
    await page.waitForTimeout(300);

    await expect(page.locator('[data-testid="edge-tab-left-git-tab"]')).not.toBeVisible({ timeout: 2000 });
    await expect(page.locator('[data-testid="edge-tab-bottom-git-tab"]')).toBeVisible({ timeout: 2000 });
  });

  test('dragging last tab out of edge panel auto-collapses it', async ({ page }) => {
    // Given: left panel has 3 tabs; drain to 1 by moving explorer + search to center
    const centerTarget = await getCenterOf(page, page.locator('[data-tab-id="tab-1"]'));

    await pointerDrag(page, await getCenter(page, '[data-testid="edge-tab-left-explorer-tab"]'), centerTarget);
    await page.waitForTimeout(300);
    await pointerDrag(page, await getCenter(page, '[data-testid="edge-tab-left-search-tab"]'), centerTarget);
    await page.waitForTimeout(300);

    const leftTabBar = page.locator('[data-testid="edge-tabbar-left"]');
    await expect(leftTabBar).toBeVisible({ timeout: 2000 });

    // When: drag the last remaining tab (git-tab) to center
    await pointerDrag(page, await getCenter(page, '[data-testid="edge-tab-left-git-tab"]'), centerTarget);
    await page.waitForTimeout(300);

    // Then: left panel auto-collapses
    await expect(leftTabBar).not.toBeVisible({ timeout: 2000 });
  });

  test('drag center tab onto collapsed right panel expands it', async ({ page }) => {
    // Given: right panel starts collapsed
    const from = await getCenterOf(page, page.locator('[data-tab-id="tab-4"]'));
    const to = await getCenter(page, '[data-testid="edge-collapsed-drop-right"]');

    // When: drag center tab onto collapsed strip
    await pointerDrag(page, from, to);
    await page.waitForTimeout(300);

    // Then: right panel expands with the dropped tab
    await expect(page.locator('[data-testid="edge-tabbar-right"]')).toBeVisible({ timeout: 2000 });
    await expect(page.locator('[data-testid="edge-tab-right-tab-4"]')).toBeVisible({ timeout: 2000 });
  });

  test('edge tab drag creates drag overlay with tab title', async ({ page }) => {
    const from = await getCenter(page, '[data-testid="edge-tab-left-explorer-tab"]');

    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(from.x + 30, from.y + 30, { steps: 5 });
    await page.waitForTimeout(200);

    const overlay = page.locator('text="Explorer"').last();
    expect(await overlay.isVisible()).toBe(true);

    await page.mouse.up();
    await page.waitForTimeout(200);

    await expect(page.locator('[data-testid="edge-tab-left-explorer-tab"]')).toBeVisible({ timeout: 2000 });
  });

  test('drag edge tab from bottom panel to center pane tab group', async ({ page }) => {
    const from = await getCenter(page, '[data-testid="edge-tab-bottom-problems-tab"]');
    const to = await getCenterOf(page, page.locator('[data-tab-id="tab-5"]'));

    await pointerDrag(page, from, to);
    await page.waitForTimeout(300);

    await expect(page.locator('[data-testid="edge-tab-bottom-problems-tab"]')).not.toBeVisible({ timeout: 2000 });
    await expect(page.locator('[data-tab-id="problems-tab"]')).toBeVisible({ timeout: 2000 });
  });
});
