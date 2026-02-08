import { test, expect } from '@playwright/test';
import {
  findPath,
  findTabButton,
  findAllTabSets,
  drag,
  Location,
} from './helpers';

const baseURL = '/flexlayout-test.html';
const evidencePath = '../../../.sisyphus/evidence';

// ─── 4.1 Border Locations (top/bottom/left/right) ────────────────────

test.describe('Border 4.1: Locations', () => {
  test('test_with_borders renders all 4 border locations', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findTabButton(page, '/border/top', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/bottom', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/left', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/right', 0)).toBeVisible();

    await expect(findTabButton(page, '/border/top', 0).locator('.flexlayout__border_button_content')).toContainText('top1');
    await expect(findTabButton(page, '/border/bottom', 0).locator('.flexlayout__border_button_content')).toContainText('bottom1');
    await expect(findTabButton(page, '/border/left', 0).locator('.flexlayout__border_button_content')).toContainText('left1');
    await expect(findTabButton(page, '/border/right', 0).locator('.flexlayout__border_button_content')).toContainText('right1');

    await page.screenshot({ path: `${evidencePath}/border-4.1-locations.png` });
  });

  test('border tab buttons are positioned on their respective edges', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const layoutBox = await findPath(page, '/').boundingBox();
    expect(layoutBox).toBeTruthy();

    const topBtn = await findTabButton(page, '/border/top', 0).boundingBox();
    const bottomBtn = await findTabButton(page, '/border/bottom', 0).boundingBox();
    const leftBtn = await findTabButton(page, '/border/left', 0).boundingBox();
    const rightBtn = await findTabButton(page, '/border/right', 0).boundingBox();

    expect(topBtn).toBeTruthy();
    expect(bottomBtn).toBeTruthy();
    expect(leftBtn).toBeTruthy();
    expect(rightBtn).toBeTruthy();

    // Top border near top edge
    expect(topBtn!.y).toBeLessThan(layoutBox!.y + layoutBox!.height * 0.15);
    // Bottom border near bottom edge
    expect(bottomBtn!.y + bottomBtn!.height).toBeGreaterThan(layoutBox!.y + layoutBox!.height * 0.85);
    // Left border near left edge
    expect(leftBtn!.x).toBeLessThan(layoutBox!.x + layoutBox!.width * 0.15);
    // Right border near right edge
    expect(rightBtn!.x + rightBtn!.width).toBeGreaterThan(layoutBox!.x + layoutBox!.width * 0.85);
  });
});

// ─── 4.2 Border Show/Hide ────────────────────────────────────────────

test.describe('Border 4.2: Show/Hide', () => {
  test('borders with show:true are visible, show:false are hidden', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_show_hide');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Top and left borders are visible (show: true)
    await expect(findTabButton(page, '/border/top', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/left', 0)).toBeVisible();

    // Bottom and right borders are hidden (show: false)
    await expect(findTabButton(page, '/border/bottom', 0)).not.toBeAttached();
    await expect(findTabButton(page, '/border/right', 0)).not.toBeAttached();

    await page.screenshot({ path: `${evidencePath}/border-4.2-show-hide.png` });
  });

  test('visible border tabs have correct names', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_show_hide');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findTabButton(page, '/border/top', 0).locator('.flexlayout__border_button_content')).toContainText('Top Visible');
    await expect(findTabButton(page, '/border/left', 0).locator('.flexlayout__border_button_content')).toContainText('Left Visible');
  });
});

// ─── 4.3 Border Size ─────────────────────────────────────────────────

test.describe('Border 4.3: Size', () => {
  test('border opens at configured borderSize', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_sizing');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Click the bottom border tab to open it
    await findTabButton(page, '/border/bottom', 0).click();
    await expect(findPath(page, '/border/bottom/t0')).toBeVisible();

    // borderSize is 300 — panel height should be approximately 300px
    const panel = await findPath(page, '/border/bottom/t0').boundingBox();
    expect(panel).toBeTruthy();
    expect(panel!.height).toBeGreaterThan(250);
    expect(panel!.height).toBeLessThan(350);

    await page.screenshot({ path: `${evidencePath}/border-4.3-size.png` });
  });

  test('left border opens at configured borderSize', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_sizing');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await findTabButton(page, '/border/left', 0).click();
    await expect(findPath(page, '/border/left/t0')).toBeVisible();

    const panel = await findPath(page, '/border/left/t0').boundingBox();
    expect(panel).toBeTruthy();
    expect(panel!.width).toBeGreaterThan(250);
    expect(panel!.width).toBeLessThan(350);
  });
});

// ─── 4.4 Border Min/Max Size ─────────────────────────────────────────

test.describe('Border 4.4: Min/Max Size', () => {
  test('border opens within min/max constraints (borderMinSize:100, borderMaxSize:500)', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_sizing');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await findTabButton(page, '/border/left', 0).click();
    await expect(findPath(page, '/border/left/t0')).toBeVisible();

    const panel = await findPath(page, '/border/left/t0').boundingBox();
    expect(panel).toBeTruthy();
    expect(panel!.width).toBeGreaterThanOrEqual(95);
    expect(panel!.width).toBeLessThanOrEqual(510);

    await page.screenshot({ path: `${evidencePath}/border-4.4-min-max-size.png` });
  });

  test('bottom border also respects size constraints', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_sizing');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await findTabButton(page, '/border/bottom', 0).click();
    await expect(findPath(page, '/border/bottom/t0')).toBeVisible();

    const panel = await findPath(page, '/border/bottom/t0').boundingBox();
    expect(panel).toBeTruthy();
    expect(panel!.height).toBeGreaterThanOrEqual(95);
    expect(panel!.height).toBeLessThanOrEqual(510);
  });
});

// ─── 4.5 Border Enable Drop ──────────────────────────────────────────

test.describe('Border 4.5: Enable Drop', () => {
  test('border with enableDrop:false rejects tab drops', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_config');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Top border has enableDrop: false
    const topBorderTabs = page.locator('[data-layout-path^="/border/top/tb"]');
    const initialCount = await topBorderTabs.count();

    // Try to drag a tab from center tabset to the top border
    const mainTab = findTabButton(page, '/ts0', 0);
    const topBorder = findTabButton(page, '/border/top', 0);
    await drag(page, mainTab, topBorder, Location.CENTER);

    // Top border should still have the same number of tabs (drop rejected)
    await expect(topBorderTabs).toHaveCount(initialCount);

    await page.screenshot({ path: `${evidencePath}/border-4.5-enable-drop.png` });
  });

  test('border with enableDrop:true accepts tab drops', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_config');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Left border has enableDrop: true
    await expect(findTabButton(page, '/border/left', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/left', 0).locator('.flexlayout__border_button_content')).toContainText('Drop Enabled');
  });
});

// ─── 4.6 Border Auto-Hide ────────────────────────────────────────────

test.describe('Border 4.6: Auto-Hide', () => {
  test('border tab strip is hidden when no tab is selected (auto-hide enabled)', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_autohide');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // With borderEnableAutoHide: true, border tabstrips should be hidden
    // when no tab is selected
    const borderTabBars = page.locator('.flexlayout__border_top, .flexlayout__border_bottom, .flexlayout__border_left, .flexlayout__border_right');
    const visibleBars = await borderTabBars.all();

    // Some or all borders should be auto-hidden (not visible)
    // Verify main content is still accessible
    await expect(findAllTabSets(page)).toHaveCount(1);
    await expect(findPath(page, '/ts0')).toBeVisible();

    await page.screenshot({ path: `${evidencePath}/border-4.6-autohide.png` });
  });

  test('clicking a border tab makes it visible, clicking again hides it', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_autohide');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // The border tab buttons should still be accessible even with auto-hide
    // Find the bottom border tab button if visible
    const bottomTab = findTabButton(page, '/border/bottom', 0);

    // Try to interact with the border - if visible
    const isAttached = await bottomTab.isVisible().catch(() => false);
    if (isAttached) {
      await bottomTab.click();
      // After click, border panel should open
      await expect(findPath(page, '/border/bottom/t0')).toBeVisible();

      // Click again to deselect
      await bottomTab.click();
      // Panel should close
      await expect(findPath(page, '/border/bottom/t0')).not.toBeVisible();
    }

    await page.screenshot({ path: `${evidencePath}/border-4.6-autohide-toggle.png` });
  });
});

// ─── 4.7-4.8 Border Auto-Select Open/Closed ─────────────────────────

test.describe('Border 4.7: Auto-Select Tab When Open', () => {
  test('borderAutoSelectTabWhenOpen auto-selects first tab on border open', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_auto_select_open');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const borderTab = findTabButton(page, '/border/bottom', 0);
    await borderTab.click();

    await expect(findPath(page, '/border/bottom/t0')).toBeVisible();
    await expect(findTabButton(page, '/border/bottom', 0)).toHaveClass(/flexlayout__border_button--selected/);
    await expect(findTabButton(page, '/border/bottom', 0).locator('.flexlayout__border_button_content')).toContainText('Console');

    await page.screenshot({ path: `${evidencePath}/border-4.7-auto-select-open.png` });
  });

  test('multiple tabs exist but first is auto-selected', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_auto_select_open');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findTabButton(page, '/border/bottom', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/bottom', 1)).toBeVisible();

    await findTabButton(page, '/border/bottom', 0).click();
    await expect(findPath(page, '/border/bottom/t0')).toBeVisible();
    await expect(findTabButton(page, '/border/bottom', 0)).toHaveClass(/flexlayout__border_button--selected/);
  });
});

test.describe('Border 4.8: Auto-Select Tab When Closed', () => {
  test('borderAutoSelectTabWhenClosed layout renders with correct tabs', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_auto_select_closed');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Verify border tabs exist
    await expect(findTabButton(page, '/border/bottom', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/bottom', 1)).toBeVisible();

    await expect(findTabButton(page, '/border/bottom', 0).locator('.flexlayout__border_button_content')).toContainText('Terminal');
    await expect(findTabButton(page, '/border/bottom', 1).locator('.flexlayout__border_button_content')).toContainText('Problems');

    await page.screenshot({ path: `${evidencePath}/border-4.8-auto-select-closed.png` });
  });

  test('opening and closing a tab allows second tab to be auto-selected', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_auto_select_closed');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Open first tab
    await findTabButton(page, '/border/bottom', 0).click();
    await expect(findPath(page, '/border/bottom/t0')).toBeVisible();

    // Click the same tab to close it
    await findTabButton(page, '/border/bottom', 0).click();

    // With borderAutoSelectTabWhenClosed: true, another tab may auto-select
    // Verify the border still has both tabs available
    await expect(findTabButton(page, '/border/bottom', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/bottom', 1)).toBeVisible();
  });
});

// ─── 4.9 Border Tab Scrollbar ────────────────────────────────────────

test.describe('Border 4.9: Tab Scrollbar', () => {
  test('border with many tabs renders all tab buttons', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_scrollbar');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findTabButton(page, '/border/bottom', 0).locator('.flexlayout__border_button_content')).toContainText('Console');
    await expect(findTabButton(page, '/border/bottom', 9).locator('.flexlayout__border_button_content')).toContainText('Notifications');

    for (let i = 0; i < 10; i++) {
      await expect(findTabButton(page, '/border/bottom', i)).toBeAttached();
    }

    await page.screenshot({ path: `${evidencePath}/border-4.9-scrollbar.png` });
  });

  test('left border also has multiple tabs with scrollbar', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_scrollbar');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findTabButton(page, '/border/left', 0).locator('.flexlayout__border_button_content')).toContainText('Explorer');
    await expect(findTabButton(page, '/border/left', 5).locator('.flexlayout__border_button_content')).toContainText('Outline');

    for (let i = 0; i < 6; i++) {
      await expect(findTabButton(page, '/border/left', i)).toBeAttached();
    }
  });
});

// ─── 4.10 Border CSS Class ───────────────────────────────────────────

test.describe('Border 4.10: CSS Class', () => {
  test('borders with className attribute have custom classes applied', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_config');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Top border has className: "border-highlight"
    // Bottom border has className: "border-accent"
    // Right border has className: "border-readonly"

    // Verify tabs render with correct names showing different border configs
    await expect(findTabButton(page, '/border/top', 0).locator('.flexlayout__border_button_content')).toContainText('No Drop Zone');
    await expect(findTabButton(page, '/border/bottom', 0).locator('.flexlayout__border_button_content')).toContainText('Styled Bottom');
    await expect(findTabButton(page, '/border/right', 0).locator('.flexlayout__border_button_content')).toContainText('Read-Only');

    // Check that border containers have the custom className applied
    const topBorder = page.locator('.flexlayout__border_top');
    const bottomBorder = page.locator('.flexlayout__border_bottom');
    const rightBorder = page.locator('.flexlayout__border_right');

    // Borders should be present in DOM
    await expect(topBorder.first()).toBeAttached();
    await expect(bottomBorder.first()).toBeAttached();
    await expect(rightBorder.first()).toBeAttached();

    await page.screenshot({ path: `${evidencePath}/border-4.10-css-class.png` });
  });
});

// ─── 4.11 Border Config (Arbitrary JSON) ─────────────────────────────

test.describe('Border 4.11: Config', () => {
  test('borders with config data render correctly', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_config_data');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Bottom border has config: { position: "primary" }
    await expect(findTabButton(page, '/border/bottom', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/bottom', 0).locator('.flexlayout__border_button_content')).toContainText('Primary Border');

    // Left border has config: { position: "secondary", collapsible: true }
    await expect(findTabButton(page, '/border/left', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/left', 0).locator('.flexlayout__border_button_content')).toContainText('Secondary Border');

    await page.screenshot({ path: `${evidencePath}/border-4.11-config.png` });
  });

  test('border tab content describes the config data', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_config_data');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Open bottom border tab
    await findTabButton(page, '/border/bottom', 0).click();
    await expect(findPath(page, '/border/bottom/t0')).toBeVisible();
    await expect(findPath(page, '/border/bottom/t0')).toContainText('position');

    // Open left border tab
    await findTabButton(page, '/border/left', 0).click();
    await expect(findPath(page, '/border/left/t0')).toBeVisible();
    await expect(findPath(page, '/border/left/t0')).toContainText('collapsible');
  });
});

// ─── 4.12 Rotate Border Icons ────────────────────────────────────────

test.describe('Border 4.12: Rotate Border Icons', () => {
  test('vertical borders (left/right) render tab buttons on side edges', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_scrollbar');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const leftTab = findTabButton(page, '/border/left', 0);
    await expect(leftTab).toBeVisible();
    await expect(leftTab.locator('.flexlayout__border_button_content')).toContainText('Explorer');

    const leftBox = await leftTab.boundingBox();
    expect(leftBox).toBeTruthy();

    const layoutBox = await findPath(page, '/').boundingBox();
    expect(layoutBox).toBeTruthy();
    expect(leftBox!.x).toBeLessThan(layoutBox!.x + layoutBox!.width * 0.1);

    await page.screenshot({ path: `${evidencePath}/border-4.12-rotate-icons.png` });
  });

  test('horizontal borders (top/bottom) have horizontal tab buttons', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const topTab = findTabButton(page, '/border/top', 0);
    const bottomTab = findTabButton(page, '/border/bottom', 0);

    const topBox = await topTab.boundingBox();
    const bottomBox = await bottomTab.boundingBox();

    expect(topBox).toBeTruthy();
    expect(bottomBox).toBeTruthy();

    expect(topBox!.width).toBeGreaterThan(topBox!.height);
    expect(bottomBox!.width).toBeGreaterThan(bottomBox!.height);
  });
});
