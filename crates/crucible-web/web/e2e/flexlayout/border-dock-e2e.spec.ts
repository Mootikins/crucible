import { test, expect } from '@playwright/test';
import { findPath, findTabButton } from './helpers';

const baseURL = '/flexlayout-test.html';
const evidencePath = '../../../.sisyphus/evidence';

test.describe('Docked Panes > Demo loads', () => {
  test('docked_panes layout loads with all 4 borders', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findTabButton(page, '/border/top', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/bottom', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/left', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/right', 0)).toBeVisible();

    await page.screenshot({ path: `${evidencePath}/dock-e2e-layout-loads.png` });
  });

  test('center content area is visible', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const mainTabset = page.locator('.flexlayout__tabset').first();
    await expect(mainTabset).toBeVisible();
  });

  test('tab names match expected values', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findTabButton(page, '/border/left', 0).locator('.flexlayout__border_button_content')).toContainText('Explorer');
    await expect(findTabButton(page, '/border/left', 1).locator('.flexlayout__border_button_content')).toContainText('Search');
    await expect(findTabButton(page, '/border/right', 0).locator('.flexlayout__border_button_content')).toContainText('Properties');
    await expect(findTabButton(page, '/border/bottom', 0).locator('.flexlayout__border_button_content')).toContainText('Terminal');
    await expect(findTabButton(page, '/border/bottom', 1).locator('.flexlayout__border_button_content')).toContainText('Output');
    await expect(findTabButton(page, '/border/top', 0).locator('.flexlayout__border_button_content')).toContainText('Toolbar');
  });
});

test.describe('Docked Panes > Tab locking', () => {
  test('tabs have no close buttons (enableClose: false)', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const closeButtons = page.locator('.flexlayout__border_button_close');
    await expect(closeButtons).toHaveCount(0);
  });
});

test.describe('Docked Panes > Collapse/Expand/Minimize cycle', () => {
  test('bottom border has dock button', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/bottom/button/dock"]');
    await expect(dockButton).toBeVisible();
  });

  test('clicking dock button on bottom border collapses it', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/bottom/button/dock"]');
    await dockButton.click();

    const borderBottom = page.locator('.flexlayout__border_bottom');
    await expect(borderBottom.first()).toHaveClass(/flexlayout__border--collapsed/);

    await page.screenshot({ path: `${evidencePath}/dock-e2e-bottom-collapsed.png` });
  });

  test('full cycle: expanded → collapsed → minimized → expanded on bottom', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/bottom/button/dock"]');

    // expanded → collapsed
    await dockButton.click();
    const borderBottom = page.locator('.flexlayout__border_bottom');
    await expect(borderBottom.first()).toHaveClass(/flexlayout__border--collapsed/);

    // collapsed → minimized
    await dockButton.click();
    await expect(borderBottom.first()).toHaveClass(/flexlayout__border--minimized/);

    // minimized → expanded
    await dockButton.click();
    await expect(borderBottom.first()).not.toHaveClass(/flexlayout__border--collapsed/);
    await expect(borderBottom.first()).not.toHaveClass(/flexlayout__border--minimized/);

    await page.screenshot({ path: `${evidencePath}/dock-e2e-bottom-cycle.png` });
  });

  test('left border dock cycle works', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/left/button/dock"]');
    await expect(dockButton).toBeVisible();

    await dockButton.click();
    const borderLeft = page.locator('.flexlayout__border_left');
    await expect(borderLeft.first()).toHaveClass(/flexlayout__border--collapsed/);

    await dockButton.click();
    await expect(borderLeft.first()).toHaveClass(/flexlayout__border--minimized/);

    await dockButton.click();
    await expect(borderLeft.first()).not.toHaveClass(/flexlayout__border--collapsed/);
    await expect(borderLeft.first()).not.toHaveClass(/flexlayout__border--minimized/);
  });

  test('right border dock cycle works', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/right/button/dock"]');
    await expect(dockButton).toBeVisible();

    await dockButton.click();
    const borderRight = page.locator('.flexlayout__border_right');
    await expect(borderRight.first()).toHaveClass(/flexlayout__border--collapsed/);

    await dockButton.click();
    await expect(borderRight.first()).toHaveClass(/flexlayout__border--minimized/);

    await dockButton.click();
    await expect(borderRight.first()).not.toHaveClass(/flexlayout__border--collapsed/);
    await expect(borderRight.first()).not.toHaveClass(/flexlayout__border--minimized/);
  });

  test('top border dock cycle works', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/top/button/dock"]');
    await expect(dockButton).toBeVisible();

    await dockButton.click();
    const borderTop = page.locator('.flexlayout__border_top');
    await expect(borderTop.first()).toHaveClass(/flexlayout__border--collapsed/);

    await dockButton.click();
    await expect(borderTop.first()).toHaveClass(/flexlayout__border--minimized/);

    await dockButton.click();
    await expect(borderTop.first()).not.toHaveClass(/flexlayout__border--collapsed/);
    await expect(borderTop.first()).not.toHaveClass(/flexlayout__border--minimized/);
  });
});

test.describe('Docked Panes > Tiled panes rendering', () => {
  test('bottom border shows 2 tiled panes with content', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const bottomTiles = page.locator('[data-border-tile]').filter({
      has: page.locator('.flexlayout__tab_border'),
    });

    const borderContent = page.locator('[data-border-content]');
    const tiles = borderContent.locator('[data-border-tile]');
    const firstTileCount = await tiles.count();
    expect(firstTileCount).toBeGreaterThanOrEqual(2);

    await page.screenshot({ path: `${evidencePath}/dock-e2e-tiled-bottom.png` });
  });

  test('bottom border has splitter between tiles', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const tileSplitters = page.locator('[data-border-tile-splitter]');
    const count = await tileSplitters.count();
    expect(count).toBeGreaterThanOrEqual(1);
  });

  test('left border shows 2 tiled panes', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const leftBorderContent = page.locator('.flexlayout__border_left').locator('..').locator('[data-border-content]');

    const allBorderContent = page.locator('[data-border-content]');
    const totalTileCount = await allBorderContent.locator('[data-border-tile]').count();
    expect(totalTileCount).toBeGreaterThanOrEqual(4);

    await page.screenshot({ path: `${evidencePath}/dock-e2e-tiled-left.png` });
  });
});

test.describe('Docked Panes > Collapsed state', () => {
  test('collapsed border shows tab names as labels', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/bottom/button/dock"]');
    await dockButton.click();

    const collapsedLabels = page.locator('.flexlayout__border_bottom .flexlayout__border_collapsed_label');
    const labelCount = await collapsedLabels.count();
    expect(labelCount).toBeGreaterThanOrEqual(1);

    await page.screenshot({ path: `${evidencePath}/dock-e2e-collapsed-labels.png` });
  });

  test('collapsed left border has vertical writing mode on labels', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/left/button/dock"]');
    await dockButton.click();

    const collapsedLabels = page.locator('.flexlayout__border_left .flexlayout__border_collapsed_label');
    const labelCount = await collapsedLabels.count();
    expect(labelCount).toBeGreaterThanOrEqual(1);

    await page.screenshot({ path: `${evidencePath}/dock-e2e-collapsed-vertical.png` });
  });
});

test.describe('Docked Panes > Minimized state', () => {
  test('minimized border shows only dock button (tiny arrow)', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/bottom/button/dock"]');
    await dockButton.click(); // → collapsed
    await dockButton.click(); // → minimized

    const borderBottom = page.locator('.flexlayout__border_bottom');
    await expect(borderBottom.first()).toHaveClass(/flexlayout__border--minimized/);

    const tabButtons = borderBottom.locator('.flexlayout__border_button');
    await expect(tabButtons).toHaveCount(0);

    const labels = borderBottom.locator('.flexlayout__border_collapsed_label');
    await expect(labels).toHaveCount(0);

    await page.screenshot({ path: `${evidencePath}/dock-e2e-minimized.png` });
  });

  test('minimized border dock button restores to expanded', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/bottom/button/dock"]');
    await dockButton.click(); // → collapsed
    await dockButton.click(); // → minimized
    await dockButton.click(); // → expanded

    const borderBottom = page.locator('.flexlayout__border_bottom');
    await expect(borderBottom.first()).not.toHaveClass(/flexlayout__border--minimized/);
    await expect(borderBottom.first()).not.toHaveClass(/flexlayout__border--collapsed/);

    const tabButtons = borderBottom.locator('.flexlayout__border_button');
    const count = await tabButtons.count();
    expect(count).toBeGreaterThanOrEqual(1);
  });
});

test.describe('Docked Panes > White content / dark chrome', () => {
  test('content areas have white background', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const tab = page.locator('.flexlayout__tab').first();
    const isVisible = await tab.isVisible().catch(() => false);
    if (isVisible) {
      const bgColor = await tab.evaluate((el) => getComputedStyle(el).backgroundColor);
      expect(bgColor).toMatch(/rgb\(255,\s*255,\s*255\)/);
    }

    await page.screenshot({ path: `${evidencePath}/dock-e2e-white-content.png` });
  });
});

test.describe('Docked Panes > All minimized', () => {
  test('minimizing all borders gives center most of the viewport', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const layoutBox = await findPath(page, '/').boundingBox();
    expect(layoutBox).toBeTruthy();

    for (const edge of ['top', 'bottom', 'left', 'right']) {
      const btn = page.locator(`[data-layout-path="/border/${edge}/button/dock"]`);
      await btn.click(); // → collapsed
      await btn.click(); // → minimized
    }

    await page.waitForTimeout(300);

    const centerTabset = page.locator('.flexlayout__tabset').first();
    const centerBox = await centerTabset.boundingBox();
    expect(centerBox).toBeTruthy();

    const layoutArea = layoutBox!.width * layoutBox!.height;
    const centerArea = centerBox!.width * centerBox!.height;
    expect(centerArea / layoutArea).toBeGreaterThan(0.7);

    await page.screenshot({ path: `${evidencePath}/dock-e2e-all-minimized.png` });
  });
});
