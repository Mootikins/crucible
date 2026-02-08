import { test, expect } from '@playwright/test';
import {
  findPath,
  findTabButton,
  findAllTabSets,
  checkTab,
  checkBorderTab,
  drag,
  dragSplitter,
  Location,
} from './helpers';
import { CLASSES } from '../../src/lib/flexlayout/core/Types';

const baseURL = '/flexlayout-test.html';
const evidencePath = '../../../.sisyphus/evidence';

// ─── 1. Tab Selection ────────────────────────────────────────────────

test.describe('Core: Tab Selection', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    await expect(findAllTabSets(page)).toHaveCount(3);
  });

  test('clicking inactive tab selects it and shows its panel', async ({ page }) => {
    // ts1 has "Two" selected by default; click ts0's tab button to activate ts0
    const ts0Tab = findTabButton(page, '/ts0', 0);
    const ts1Tab = findTabButton(page, '/ts1', 0);

    // Click ts1 tab to ensure it's the active tabset
    await ts1Tab.click();
    await expect(findPath(page, '/ts1/tabstrip')).toHaveClass(
      new RegExp(CLASSES.FLEXLAYOUT__TABSET_SELECTED),
    );

    // Now click ts0 tab — it should become the active tabset
    await ts0Tab.click();
    await expect(findPath(page, '/ts0/tabstrip')).toHaveClass(
      new RegExp(CLASSES.FLEXLAYOUT__TABSET_SELECTED),
    );
    await expect(findPath(page, '/ts1/tabstrip')).not.toHaveClass(
      new RegExp(CLASSES.FLEXLAYOUT__TABSET_SELECTED),
    );

    // Tab content panel should be visible
    await expect(findPath(page, '/ts0/t0')).toBeVisible();

    await page.screenshot({ path: `${evidencePath}/task-6-tab-selection.png` });
  });

  test('clicking tab button in same tabset switches selected tab', async ({ page }) => {
    // Add a second tab to ts0 so we can switch within the same tabset
    await findPath(page, '/ts0/tabstrip').click();
    await page.locator('[data-id=add-active]').click();

    // Now ts0 should have 2 tabs; the new one (Text1) is selected
    await checkTab(page, '/ts0', 1, true, 'Text1');
    await checkTab(page, '/ts0', 0, false, 'One');

    // Click the first tab
    await findTabButton(page, '/ts0', 0).click();
    await checkTab(page, '/ts0', 0, true, 'One');
    await checkTab(page, '/ts0', 1, false, 'Text1');

    await page.screenshot({ path: `${evidencePath}/task-6-tab-switch.png` });
  });
});

// ─── 2. Maximize Toggle ──────────────────────────────────────────────

test.describe('Core: Maximize Toggle', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    await expect(findAllTabSets(page)).toHaveCount(3);
  });

  test('maximize button hides other tabsets, restore shows all', async ({ page }) => {
    // All 3 tabsets visible
    await expect(findPath(page, '/ts0')).toBeVisible();
    await expect(findPath(page, '/ts1')).toBeVisible();
    await expect(findPath(page, '/ts2')).toBeVisible();

    // Click maximize on ts1
    await findPath(page, '/ts1/button/max').click();

    // Only ts1 should be visible
    await expect(findPath(page, '/ts0')).toBeHidden();
    await expect(findPath(page, '/ts1')).toBeVisible();
    await expect(findPath(page, '/ts2')).toBeHidden();

    await page.screenshot({ path: `${evidencePath}/task-6-maximize-on.png` });

    // Click maximize again to restore
    await findPath(page, '/ts1/button/max').click();

    await expect(findPath(page, '/ts0')).toBeVisible();
    await expect(findPath(page, '/ts1')).toBeVisible();
    await expect(findPath(page, '/ts2')).toBeVisible();

    await page.screenshot({ path: `${evidencePath}/task-6-maximize-off.png` });
  });

  test('double-click tabstrip maximizes tabset', async ({ page }) => {
    await findPath(page, '/ts2/tabstrip').dblclick();

    await expect(findPath(page, '/ts0')).toBeHidden();
    await expect(findPath(page, '/ts1')).toBeHidden();
    await expect(findPath(page, '/ts2')).toBeVisible();

    // Restore via max button
    await findPath(page, '/ts2/button/max').click();

    await expect(findPath(page, '/ts0')).toBeVisible();
    await expect(findPath(page, '/ts1')).toBeVisible();
    await expect(findPath(page, '/ts2')).toBeVisible();
  });
});

// ─── 3. Tab Close / Delete ───────────────────────────────────────────

test.describe('Core: Tab Close', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    await expect(findAllTabSets(page)).toHaveCount(3);
  });

  test('close button removes tab and selects sibling', async ({ page }) => {
    // ts1 has tab "Two"; close it
    const closeBtn = findPath(page, '/ts1/tb0/button/close');
    await closeBtn.click();

    // ts1 is gone (single tab tabset collapses), only 2 tabsets remain
    await expect(findAllTabSets(page)).toHaveCount(2);

    // Remaining tabs are "One" and "Three"
    await checkTab(page, '/ts0', 0, true, 'One');
    await checkTab(page, '/ts1', 0, true, 'Three');

    await page.screenshot({ path: `${evidencePath}/task-6-tab-close.png` });
  });

  test('closing all tabs leaves empty tabset', async ({ page }) => {
    // Close all three tabs one by one
    await findPath(page, '/ts1/tb0/button/close').click();
    await findPath(page, '/ts1/tb0/button/close').click();
    await findPath(page, '/ts0/tb0/button/close').click();

    // Should be 1 tabset left (empty placeholder)
    await expect(findAllTabSets(page)).toHaveCount(1);

    await page.screenshot({ path: `${evidencePath}/task-6-close-all.png` });
  });
});

// ─── 4. Splitter Drag ────────────────────────────────────────────────

test.describe('Core: Splitter Drag', () => {
  test('vertical splitter changes panel widths', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_two_tabs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const splitter = findPath(page, '/s0');

    // Get initial widths
    const initialW0 = (await findPath(page, '/ts0').boundingBox())?.width ?? 0;
    const initialW1 = (await findPath(page, '/ts1').boundingBox())?.width ?? 0;

    // Drag splitter right by 100px
    await dragSplitter(page, splitter, false, 100);

    const newW0 = (await findPath(page, '/ts0').boundingBox())?.width ?? 0;
    const newW1 = (await findPath(page, '/ts1').boundingBox())?.width ?? 0;

    // ts0 should be wider, ts1 should be narrower
    expect(newW0).toBeGreaterThan(initialW0 + 50);
    expect(newW1).toBeLessThan(initialW1 - 50);

    await page.screenshot({ path: `${evidencePath}/task-6-splitter-drag.png` });
  });

  test('horizontal splitter changes panel heights', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_two_tabs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // First create a vertical split by dragging tab to bottom
    const from = findTabButton(page, '/ts0', 0);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.BOTTOM);

    await expect(findAllTabSets(page)).toHaveCount(2);

    // Now drag the horizontal splitter
    const hsplitter = findPath(page, '/r0/s0');
    await dragSplitter(page, hsplitter, true, 80);

    const topH = (await findPath(page, '/r0/ts0').boundingBox())?.height ?? 0;
    const bottomH = (await findPath(page, '/r0/ts1').boundingBox())?.height ?? 0;

    expect(topH - bottomH).toBeGreaterThan(50);

    await page.screenshot({ path: `${evidencePath}/task-6-hsplitter-drag.png` });
  });
});

// ─── 5. Tab Drag-and-Drop ────────────────────────────────────────────

test.describe('Core: Tab Drag-and-Drop', () => {
  test('drag tab to another tabset merges them', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_two_tabs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const tabSets = findAllTabSets(page);
    await expect(tabSets).toHaveCount(2);

    // Drag "One" from ts0 to ts1 center → merges into single tabset
    const from = findTabButton(page, '/ts0', 0);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.CENTER);

    await expect(findAllTabSets(page)).toHaveCount(1);
    await checkTab(page, '/ts0', 0, false, 'Two');
    await checkTab(page, '/ts0', 1, true, 'One');

    await page.screenshot({ path: `${evidencePath}/task-6-tab-dnd-merge.png` });
  });

  test('drag tab to create new split', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_three_tabs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(3);

    // Drag "One" from ts0 to top of ts1 → creates a column split
    const from = findTabButton(page, '/ts0', 0);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.TOP);

    await expect(findAllTabSets(page)).toHaveCount(3);

    await checkTab(page, '/r0/ts0', 0, true, 'One');
    await checkTab(page, '/r0/ts1', 0, true, 'Two');

    await page.screenshot({ path: `${evidencePath}/task-6-tab-dnd-split.png` });
  });
});

// ─── 6. Tab Rename ───────────────────────────────────────────────────

test.describe('Core: Tab Rename', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('double-click tab button opens rename input and Enter commits', async ({ page }) => {
    await findPath(page, '/ts1/tb0').dblclick();

    const textbox = findPath(page, '/ts1/tb0/textbox');
    await expect(textbox).toBeVisible();
    await expect(textbox).toHaveValue('Two');

    // Clear and type new name
    await textbox.fill('');
    await textbox.type('Renamed');
    await textbox.press('Enter');

    await checkTab(page, '/ts1', 0, true, 'Renamed');

    await page.screenshot({ path: `${evidencePath}/task-6-tab-rename.png` });
  });

  test('Escape cancels rename', async ({ page }) => {
    await findPath(page, '/ts1/tb0').dblclick();

    const textbox = findPath(page, '/ts1/tb0/textbox');
    await expect(textbox).toBeVisible();
    await textbox.type('WillBeDiscarded');
    await textbox.press('Escape');

    await checkTab(page, '/ts1', 0, true, 'Two');

    await page.screenshot({ path: `${evidencePath}/task-6-tab-rename-cancel.png` });
  });
});

// ─── 7. Add Tab ──────────────────────────────────────────────────────

test.describe('Core: Add Tab', () => {
  test('add-active button adds tab to active tabset', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Click ts1 tabstrip to make it active
    await findPath(page, '/ts1/tabstrip').click();

    // Click "Add Active" button
    await page.locator('[data-id=add-active]').click();

    // Should now have 2 tabs in ts1
    await checkTab(page, '/ts1', 0, false, 'Two');
    await checkTab(page, '/ts1', 1, true, 'Text1');

    await page.screenshot({ path: `${evidencePath}/task-6-add-tab.png` });
  });

  test('drag external item into tabset adds tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const from = page.locator('[data-id=add-drag]');
    const to = findPath(page, '/ts1/tabstrip');
    await drag(page, from, to, Location.CENTER);

    await checkTab(page, '/ts1', 0, false, 'Two');
    await checkTab(page, '/ts1', 1, true, 'Text1');

    await page.screenshot({ path: `${evidencePath}/task-6-add-drag.png` });
  });
});

// ─── 8. Border Tab Selection ─────────────────────────────────────────

test.describe('Core: Border Tab Selection', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('clicking border tab opens its panel', async ({ page }) => {
    // Border tabs start unselected (closed panel)
    const borderTab = findTabButton(page, '/border/top', 0);
    await expect(findPath(page, '/border/top/t0')).not.toBeVisible();

    // Click border tab
    await borderTab.click();

    // Panel should now be visible
    await expect(findPath(page, '/border/top/t0')).toBeVisible();
    await checkBorderTab(page, '/border/top', 0, true, 'top1');

    await page.screenshot({ path: `${evidencePath}/task-6-border-tab-open.png` });
  });

  test('clicking open border tab again closes its panel', async ({ page }) => {
    const borderTab = findTabButton(page, '/border/left', 0);

    // Open
    await borderTab.click();
    await expect(findPath(page, '/border/left/t0')).toBeVisible();

    // Close by clicking same tab again
    await borderTab.click();
    await expect(findPath(page, '/border/left/t0')).not.toBeVisible();

    await page.screenshot({ path: `${evidencePath}/task-6-border-tab-toggle.png` });
  });
});

// ─── 9. Tab Overflow ─────────────────────────────────────────────────

test.describe('Core: Tab Overflow', () => {
  test('overflow button appears when tabs exceed width and menu selects hidden tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Add tabs to ts0 to trigger overflow
    await findPath(page, '/ts0/tabstrip').click();
    await page.locator('[data-id=add-active]').click();
    await page.locator('[data-id=add-active]').click();

    // Overflow button should not be visible yet (enough space)
    await expect(findPath(page, '/ts0/button/overflow')).not.toBeVisible();

    // Squeeze ts0 by dragging splitter left
    const splitter = findPath(page, '/s0');
    await dragSplitter(page, splitter, false, -1000);
    await dragSplitter(page, splitter, false, 150);

    // Now overflow button should be visible
    await expect(findPath(page, '/ts0/button/overflow')).toBeVisible();

    await page.screenshot({ path: `${evidencePath}/task-6-overflow-visible.png` });

    // Click overflow to open popup menu
    await findPath(page, '/ts0/button/overflow').click();
    await expect(findPath(page, '/popup-menu')).toBeVisible();

    // Click first hidden tab in popup
    await findPath(page, '/popup-menu/tb0').click();

    // The clicked tab should now be selected and visible
    await checkTab(page, '/ts0', 0, true, 'One');

    await page.screenshot({ path: `${evidencePath}/task-6-overflow-selected.png` });
  });
});

// ─── 10. Layout Persistence (Reload) ─────────────────────────────────

test.describe('Core: Layout Reset via Reload', () => {
  test('reload button resets layout to initial state', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Verify initial state
    await expect(findAllTabSets(page)).toHaveCount(3);
    await checkTab(page, '/ts0', 0, true, 'One');
    await checkTab(page, '/ts1', 0, true, 'Two');
    await checkTab(page, '/ts2', 0, true, 'Three');

    // Mutate: close a tab
    await findPath(page, '/ts1/tb0/button/close').click();
    await expect(findAllTabSets(page)).toHaveCount(2);

    // Reload
    await page.locator('[data-id=reload]').click();

    // Layout should be restored to initial state
    await expect(findAllTabSets(page)).toHaveCount(3);
    await checkTab(page, '/ts0', 0, true, 'One');
    await checkTab(page, '/ts1', 0, true, 'Two');
    await checkTab(page, '/ts2', 0, true, 'Three');

    await page.screenshot({ path: `${evidencePath}/task-6-reload-reset.png` });
  });

  test('reload after maximize restores all tabsets', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Maximize ts1
    await findPath(page, '/ts1/button/max').click();
    await expect(findPath(page, '/ts0')).toBeHidden();

    // Reload
    await page.locator('[data-id=reload]').click();

    // All tabsets visible again
    await expect(findPath(page, '/ts0')).toBeVisible();
    await expect(findPath(page, '/ts1')).toBeVisible();
    await expect(findPath(page, '/ts2')).toBeVisible();

    await page.screenshot({ path: `${evidencePath}/task-6-reload-after-max.png` });
  });
});
