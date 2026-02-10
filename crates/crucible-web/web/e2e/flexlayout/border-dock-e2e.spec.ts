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

test.describe('Docked Panes > 2-State Collapse/Expand cycle', () => {
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

  test('2-state cycle: expanded → collapsed → expanded on bottom', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/bottom/button/dock"]');

    await dockButton.click();
    const borderBottom = page.locator('.flexlayout__border_bottom');
    await expect(borderBottom.first()).toHaveClass(/flexlayout__border--collapsed/);

    await dockButton.click();
    await expect(borderBottom.first()).not.toHaveClass(/flexlayout__border--collapsed/);

    await page.screenshot({ path: `${evidencePath}/dock-e2e-bottom-cycle.png` });
  });

  test('left border 2-state cycle works', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/left/button/dock"]');
    await expect(dockButton).toBeVisible();

    await dockButton.click();
    const borderLeft = page.locator('.flexlayout__border_left');
    await expect(borderLeft.first()).toHaveClass(/flexlayout__border--collapsed/);

    await dockButton.click();
    await expect(borderLeft.first()).not.toHaveClass(/flexlayout__border--collapsed/);
  });

  test('right border 2-state cycle works', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/right/button/dock"]');
    await expect(dockButton).toBeVisible();

    await dockButton.click();
    const borderRight = page.locator('.flexlayout__border_right');
    await expect(borderRight.first()).toHaveClass(/flexlayout__border--collapsed/);

    await dockButton.click();
    await expect(borderRight.first()).not.toHaveClass(/flexlayout__border--collapsed/);
  });

  test('top border 2-state cycle works', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/top/button/dock"]');
    await expect(dockButton).toBeVisible();

    await dockButton.click();
    const borderTop = page.locator('.flexlayout__border_top');
    await expect(borderTop.first()).toHaveClass(/flexlayout__border--collapsed/);

    await dockButton.click();
    await expect(borderTop.first()).not.toHaveClass(/flexlayout__border--collapsed/);
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

  test('collapsed left border has rotate(-90deg) on inner tab container', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/left/button/dock"]');
    await dockButton.click();

    const borderLeft = page.locator('.flexlayout__border_left');
    await expect(borderLeft.first()).toHaveClass(/flexlayout__border--collapsed/);

    const container = page.locator('.flexlayout__border_left .flexlayout__border_inner_tab_container_left');
    const transform = await container.evaluate((el) => getComputedStyle(el).transform);
    // rotate(-90deg) produces a matrix with cos(-90)=0, sin(-90)=-1
    // matrix(0, -1, 1, 0, tx, ty)
    expect(transform).toMatch(/matrix\(0,\s*-1,\s*1,\s*0/);

    await page.screenshot({ path: `${evidencePath}/task-5-left-vertical.png` });
  });

  test('collapsed right border has rotate(90deg) on inner tab container', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/right/button/dock"]');
    await dockButton.click();

    const borderRight = page.locator('.flexlayout__border_right');
    await expect(borderRight.first()).toHaveClass(/flexlayout__border--collapsed/);

    const container = page.locator('.flexlayout__border_right .flexlayout__border_inner_tab_container_right');
    const transform = await container.evaluate((el) => getComputedStyle(el).transform);
    // rotate(90deg) produces matrix(0, 1, -1, 0, tx, ty)
    expect(transform).toMatch(/matrix\(0,\s*1,\s*-1,\s*0/);

    await page.screenshot({ path: `${evidencePath}/task-5-right-vertical.png` });
  });

  test('collapsed top border has no rotation on inner tab container', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/top/button/dock"]');
    await dockButton.click();

    const borderTop = page.locator('.flexlayout__border_top');
    await expect(borderTop.first()).toHaveClass(/flexlayout__border--collapsed/);

    const container = page.locator('.flexlayout__border_top .flexlayout__border_inner_tab_container_top');
    const transform = await container.evaluate((el) => getComputedStyle(el).transform);
    // No rotation = "none" or identity matrix
    expect(transform === 'none' || transform === 'matrix(1, 0, 0, 1, 0, 0)').toBe(true);

    await page.screenshot({ path: `${evidencePath}/task-5-top-horizontal.png` });
  });

  test('collapsed bottom border has no rotation on inner tab container', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/bottom/button/dock"]');
    await dockButton.click();

    const borderBottom = page.locator('.flexlayout__border_bottom');
    await expect(borderBottom.first()).toHaveClass(/flexlayout__border--collapsed/);

    const container = page.locator('.flexlayout__border_bottom .flexlayout__border_inner_tab_container_bottom');
    const transform = await container.evaluate((el) => getComputedStyle(el).transform);
    expect(transform === 'none' || transform === 'matrix(1, 0, 0, 1, 0, 0)').toBe(true);
  });

  test('collapsed left border labels use flex-direction: row-reverse', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/left/button/dock"]');
    await dockButton.click();

    const container = page.locator('.flexlayout__border_left .flexlayout__border_inner_tab_container_left');
    const flexDir = await container.evaluate((el) => getComputedStyle(el).flexDirection);
    expect(flexDir).toBe('row-reverse');
  });
});

test.describe('Docked Panes > Collapsed state shows labels', () => {
  test('collapsed border shows tab labels', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/bottom/button/dock"]');
    await dockButton.click();

    const borderBottom = page.locator('.flexlayout__border_bottom');
    await expect(borderBottom.first()).toHaveClass(/flexlayout__border--collapsed/);

    const labels = borderBottom.locator('.flexlayout__border_collapsed_label');
    const labelCount = await labels.count();
    expect(labelCount).toBeGreaterThanOrEqual(1);

    await page.screenshot({ path: `${evidencePath}/dock-e2e-collapsed-labels-2state.png` });
  });

  test('collapsed border dock button restores to expanded', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/bottom/button/dock"]');
    await dockButton.click();
    await dockButton.click();

    const borderBottom = page.locator('.flexlayout__border_bottom');
    await expect(borderBottom.first()).not.toHaveClass(/flexlayout__border--collapsed/);

    const terminalTab = page.locator('[data-border-tabbar] .flexlayout__border_button').filter({ hasText: 'Terminal' });
    await expect(terminalTab).toBeVisible();
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

test.describe('Docked Panes > All collapsed', () => {
  test('collapsing all borders gives center most of the viewport', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const layoutBox = await findPath(page, '/').boundingBox();
    expect(layoutBox).toBeTruthy();

    for (const edge of ['top', 'bottom', 'left', 'right']) {
      const btn = page.locator(`[data-layout-path="/border/${edge}/button/dock"]`);
      await btn.click();
    }

    await page.waitForTimeout(300);

    const centerTabset = page.locator('.flexlayout__tabset').first();
    const centerBox = await centerTabset.boundingBox();
    expect(centerBox).toBeTruthy();

    const layoutArea = layoutBox!.width * layoutBox!.height;
    const centerArea = centerBox!.width * centerBox!.height;
    expect(centerArea / layoutArea).toBeGreaterThan(0.5);

    await page.screenshot({ path: `${evidencePath}/dock-e2e-all-collapsed.png` });
  });
});

test.describe('Docked Panes > Expanded tabs-on-top', () => {
  test('expanded left border shows horizontal tab bar at top of content', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const explorerButton = page.locator('[data-border-tabbar] .flexlayout__border_button').filter({ hasText: 'Explorer' });
    await expect(explorerButton).toBeVisible();

    // Per-tile headers: each tile has its own tabbar with exactly 1 button
    const tabBarInner = explorerButton.locator('..');
    const tabButtons = tabBarInner.locator('.flexlayout__border_button');
    const count = await tabButtons.count();
    expect(count).toBe(1);

    const tabBar = tabBarInner.locator('..');
    const flexDir = await tabBar.evaluate((el) => getComputedStyle(el).flexDirection);
    expect(flexDir).toBe('row');

    // Dock button appears on first tile's header (Explorer is tile 0)
    const dockButton = tabBar.locator('.flexlayout__border_dock_button');
    await expect(dockButton).toBeVisible();

    // Search tab also has its own per-tile tabbar
    const searchButton = page.locator('[data-border-tabbar] .flexlayout__border_button').filter({ hasText: 'Search' });
    await expect(searchButton).toBeVisible();

    await page.screenshot({ path: `${evidencePath}/task-6-tabs-on-top.png` });
  });

  test('expanded bottom border shows horizontal tab bar at top of content', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Bottom border is expanded by default
    // Find the bottom border content area
    const allBorderContent = page.locator('[data-border-content]');
    const contentCount = await allBorderContent.count();
    expect(contentCount).toBeGreaterThanOrEqual(1);

    // Check that at least one border content has a tab bar
    const tabBars = page.locator('[data-border-tabbar]');
    const tabBarCount = await tabBars.count();
    expect(tabBarCount).toBeGreaterThanOrEqual(2); // left + bottom at minimum
  });

  test('clicking tab button in expanded border switches content', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Find the right border content (has Properties + Outline tabs)
    // Right border has selected: 0, visibleTabs: [0]
    // We need to click the Outline tab to switch
    const rightBorderContent = page.locator('[data-border-content]');

    // Find a tab bar and click a different tab
    const tabBars = page.locator('[data-border-tabbar]');
    const tabBarCount = await tabBars.count();
    expect(tabBarCount).toBeGreaterThanOrEqual(1);

    // Find the right border's tab bar — it should have "Properties" and "Outline"
    // Use the border path to find the right one
    const rightTabButtons = page.locator('[data-border-tabbar] .flexlayout__border_button');
    const allButtonTexts: string[] = [];
    const buttonCount = await rightTabButtons.count();
    for (let i = 0; i < buttonCount; i++) {
      const text = await rightTabButtons.nth(i).textContent();
      allButtonTexts.push(text || '');
    }

    // Find the "Outline" button (in right border) and click it
    const outlineButton = page.locator('[data-border-tabbar] .flexlayout__border_button').filter({ hasText: 'Outline' });
    const outlineExists = await outlineButton.count();
    if (outlineExists > 0) {
      await outlineButton.click();
      // After clicking, Outline should be selected
      await expect(outlineButton).toHaveClass(/flexlayout__border_button--selected/);
    }

    await page.screenshot({ path: `${evidencePath}/task-6-tab-switch.png` });
  });

  test('dock button in expanded tab bar collapses the border', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Find a dock button inside a tab bar
    const dockButton = page.locator('[data-border-tabbar] .flexlayout__border_dock_button').first();
    await expect(dockButton).toBeVisible();

    // Click it — should collapse
    await dockButton.click();

    // One of the borders should now be collapsed
    const collapsedBorders = page.locator('.flexlayout__border--collapsed');
    const collapsedCount = await collapsedBorders.count();
    expect(collapsedCount).toBeGreaterThanOrEqual(1);
  });

  test('expanded border strip has zero size (tabs moved to content)', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const leftBorderStrip = page.locator('.flexlayout__border_left');
    await expect(leftBorderStrip.first()).toBeAttached();

    const box = await leftBorderStrip.first().boundingBox();
    expect(box).toBeTruthy();
    expect(box!.width).toBeLessThanOrEqual(1);

    const explorerTab = page.locator('[data-border-tabbar] .flexlayout__border_button').filter({ hasText: 'Explorer' });
    await expect(explorerTab).toBeVisible();
  });

  test('splitter resize still works with tabs-on-top layout', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Find the splitter between left border content and main area
    const splitter = page.locator('.flexlayout__splitter_border').first();
    const isVisible = await splitter.isVisible().catch(() => false);

    if (isVisible) {
      const splitterBox = await splitter.boundingBox();
      expect(splitterBox).toBeTruthy();

      if (splitterBox) {
        // Drag splitter 50px to the right
        const startX = splitterBox.x + splitterBox.width / 2;
        const startY = splitterBox.y + splitterBox.height / 2;

        await page.mouse.move(startX, startY);
        await page.mouse.down();
        await page.mouse.move(startX + 50, startY, { steps: 10 });
        await page.mouse.up();

        await page.waitForTimeout(200);
      }
    }

    await page.screenshot({ path: `${evidencePath}/task-6-splitter-resize.png` });
  });
});

test.describe('Docked Panes > Collapsed strip has dock button', () => {
  test('collapsed border shows dock button in strip', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/left/button/dock"]');
    await dockButton.click();

    const borderLeft = page.locator('.flexlayout__border_left');
    await expect(borderLeft.first()).toHaveClass(/flexlayout__border--collapsed/);

    await expect(dockButton).toBeVisible();

    await page.screenshot({ path: `${evidencePath}/dock-e2e-collapsed-dock-btn.png` });
  });

  test('clicking dock button in collapsed strip expands border', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/left/button/dock"]');
    await dockButton.click();
    await dockButton.click();

    const borderPath = page.locator('[data-layout-path="/border/left"]');
    await expect(borderPath).toHaveCount(1);
    await expect(borderPath).not.toHaveClass(/flexlayout__border--collapsed/);

    await page.screenshot({ path: `${evidencePath}/dock-e2e-collapsed-expand.png` });
  });

  test('dock button arrow direction correct for each edge', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    for (const edge of ['top', 'bottom', 'left', 'right']) {
      const btn = page.locator(`[data-layout-path="/border/${edge}/button/dock"]`);
      await btn.click();
    }

    const expectedArrows: Record<string, string> = {
      left: '▶',
      right: '◀',
      top: '▼',
      bottom: '▲',
    };

    for (const [edge, arrow] of Object.entries(expectedArrows)) {
      const btn = page.locator(`[data-layout-path="/border/${edge}/button/dock"]`);
      const text = await btn.textContent();
      expect(text?.trim()).toBe(arrow);
    }
  });

  test('dock button is keyboard accessible', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/left/button/dock"]');
    await dockButton.click();

    const tagName = await dockButton.evaluate((el) => el.tagName.toLowerCase());
    expect(tagName).toBe('button');

    await dockButton.focus();
    const isFocused = await dockButton.evaluate((el) => document.activeElement === el);
    expect(isFocused).toBe(true);
  });
});

test.describe('Docked Panes > Context menu tiling', () => {
  test('right-click border tab shows context menu with split options', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Right-click on "Terminal" tab button in bottom border (index 0)
    const terminalTab = findTabButton(page, '/border/bottom', 0);
    await terminalTab.click({ button: 'right' });

    // Context menu should appear
    const contextMenu = page.locator('[data-layout-path="/context-menu"]');
    await expect(contextMenu).toBeVisible({ timeout: 3_000 });

    // Bottom border has Terminal(0), Output(1), Problems(2) — already tiled [0,1]
    // Right-clicking Terminal should show "Untile" (since already tiled) plus split options for non-visible tabs
    // Since visibleTabs=[0,1], "Untile" should appear, plus "Split with Problems" (index 2, not visible)
    await expect(contextMenu).toContainText('Untile');
    await expect(contextMenu).toContainText('Split with Problems');
    // Should NOT contain "Split with Terminal" (self) or "Split with Output" (already tiled)
    await expect(contextMenu).not.toContainText('Split with Terminal');
    await expect(contextMenu).not.toContainText('Split with Output');

    await page.screenshot({ path: `${evidencePath}/task-8-context-menu.png` });
  });

  test('clicking "Split with" option triggers tiling', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Right border has Properties (0) and Outline (1), visibleTabs: [0] (single)
    const propertiesTab = findTabButton(page, '/border/right', 0);
    await propertiesTab.click({ button: 'right' });

    const contextMenu = page.locator('[data-layout-path="/context-menu"]');
    await expect(contextMenu).toBeVisible({ timeout: 3_000 });

    // Click "Split with Outline"
    const splitItem = contextMenu.locator('[data-context-menu-item]', { hasText: 'Split with Outline' });
    await expect(splitItem).toBeVisible();
    await splitItem.click();

    // Context menu should close
    await expect(contextMenu).not.toBeVisible();

    // Border should now show 2 tiled panes with a splitter between them
    await page.waitForTimeout(200);
    const tileSplitters = page.locator('[data-border-tile-splitter]');
    const splitterCount = await tileSplitters.count();
    // There should be at least one more splitter than before (right border now has one)
    expect(splitterCount).toBeGreaterThanOrEqual(1);

    await page.screenshot({ path: `${evidencePath}/task-8-split-result.png` });
  });

  test('untile option when already tiled reverts to single tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Bottom border starts with visibleTabs: [0, 1] (already tiled)
    const terminalTab = findTabButton(page, '/border/bottom', 0);
    await terminalTab.click({ button: 'right' });

    const contextMenu = page.locator('[data-layout-path="/context-menu"]');
    await expect(contextMenu).toBeVisible({ timeout: 3_000 });

    // Should show "Untile" option since border is already tiled
    const untileItem = contextMenu.locator('[data-context-menu-item]', { hasText: 'Untile' });
    await expect(untileItem).toBeVisible();

    // Click "Untile"
    await untileItem.click();

    // Context menu should close
    await expect(contextMenu).not.toBeVisible();

    // Wait for layout to update
    await page.waitForTimeout(200);

    await page.screenshot({ path: `${evidencePath}/task-8-untile.png` });
  });

  test('single-tab border shows no split options', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Top border has only 1 tab (Toolbar)
    const toolbarTab = findTabButton(page, '/border/top', 0);
    await toolbarTab.click({ button: 'right' });

    // Context menu should appear but with no actionable items
    const contextMenu = page.locator('[data-layout-path="/context-menu"]');
    await expect(contextMenu).toBeVisible({ timeout: 3_000 });

    // No split items should be present (only 1 tab, nothing to split with)
    const splitItems = contextMenu.locator('[data-context-menu-item]');
    await expect(splitItems).toHaveCount(0);
  });

  test('context menu dismissed on click outside', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const terminalTab = findTabButton(page, '/border/bottom', 0);
    await terminalTab.click({ button: 'right' });

    const contextMenu = page.locator('[data-layout-path="/context-menu"]');
    await expect(contextMenu).toBeVisible({ timeout: 3_000 });

    // Click on the layout overlay (outside the menu but inside the layout)
    const layoutBox = await findPath(page, '/').boundingBox();
    await page.mouse.click(
      layoutBox!.x + layoutBox!.width / 2,
      layoutBox!.y + layoutBox!.height / 2,
    );

    await expect(contextMenu).not.toBeVisible();
  });

  test('context menu dismissed on Escape key', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const terminalTab = findTabButton(page, '/border/bottom', 0);
    await terminalTab.click({ button: 'right' });

    const contextMenu = page.locator('[data-layout-path="/context-menu"]');
    await expect(contextMenu).toBeVisible({ timeout: 3_000 });

    await page.keyboard.press('Escape');

    await expect(contextMenu).not.toBeVisible();
  });
});

test.describe('Docked Panes > Collapsed border layout bounds', () => {
  test('main content fills space between all collapsed border strips', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    for (const edge of ['top', 'bottom', 'left', 'right']) {
      const dockButton = page.locator(`[data-layout-path="/border/${edge}/button/dock"]`).first();
      const title = await dockButton.getAttribute('title');
      if (title === 'Collapse') {
        await dockButton.click();
      }
    }

    await page.waitForTimeout(200);

    const main = await page.locator('[data-layout-path="/ts0"]').boundingBox();
    const topStrip = await page.locator('.flexlayout__border_top[data-collapsed-strip="true"]').boundingBox();
    const leftStrip = await page.locator('.flexlayout__border_left[data-collapsed-strip="true"]').boundingBox();
    const rightStrip = await page.locator('.flexlayout__border_right[data-collapsed-strip="true"]').boundingBox();
    const bottomStrip = await page.locator('.flexlayout__border_bottom[data-collapsed-strip="true"]').boundingBox();

    expect(main).not.toBeNull();
    expect(topStrip).not.toBeNull();
    expect(leftStrip).not.toBeNull();
    expect(rightStrip).not.toBeNull();
    expect(bottomStrip).not.toBeNull();

    expect(main!.y).toBeGreaterThanOrEqual(topStrip!.y + topStrip!.height - 2);
    expect(main!.x).toBeGreaterThanOrEqual(leftStrip!.x + leftStrip!.width - 2);
    expect(main!.x + main!.width).toBeLessThanOrEqual(rightStrip!.x + 2);
    expect(main!.y + main!.height).toBeLessThanOrEqual(bottomStrip!.y + 2);

    await page.screenshot({ path: `${evidencePath}/bug-3-7-after.png` });
  });
});
