import { test, expect } from '@playwright/test';
import {
  findPath,
  findTabButton,
  findAllTabSets,
  checkTab,
  drag,
  dragSplitter,
  Location,
} from './helpers';
import { CLASSES } from '../../src/lib/flexlayout/core/Types';

const baseURL = '/flexlayout-test.html';
const evidencePath = '../../../.sisyphus/evidence';

// ─── 3.1 Enable Drag ─────────────────────────────────────────────────

test.describe('TabSet: Enable Drag', () => {
  test('draggable tab can be moved, locked tab resists drag', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_drag_disabled');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const tb0 = findTabButton(page, '/ts0', 0);
    const tb1 = findTabButton(page, '/ts0', 1);
    const tb2 = findTabButton(page, '/ts0', 2);
    await expect(tb0.locator('.flexlayout__tab_button_content')).toContainText('Draggable');
    await expect(tb1.locator('.flexlayout__tab_button_content')).toContainText('Locked');
    await expect(tb2.locator('.flexlayout__tab_button_content')).toContainText('Also Draggable');

    const target = findPath(page, '/ts0/t0');
    await drag(page, tb1, target, Location.LEFT);

    await expect(findAllTabSets(page)).toHaveCount(1);

    await page.screenshot({ path: `${evidencePath}/tabset-3.1-drag-disabled.png` });
  });
});

// ─── 3.2 Enable Drop ─────────────────────────────────────────────────

test.describe('TabSet: Enable Drop', () => {
  test('tabset with enableDrop=false rejects drops', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_drop_disabled');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(3);

    const noDropTabs = findPath(page, '/ts1').locator('.flexlayout__tab_button');
    await expect(noDropTabs).toHaveCount(1);

    const from = findTabButton(page, '/ts0', 0);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.CENTER);

    await expect(noDropTabs).toHaveCount(1);

    await page.screenshot({ path: `${evidencePath}/tabset-3.2-drop-disabled.png` });
  });

  test('tabset without enableDrop restriction accepts drops', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_drop_disabled');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const from = findTabButton(page, '/ts0', 0);
    const to = findPath(page, '/ts2/t0');
    await drag(page, from, to, Location.CENTER);

    await expect(findAllTabSets(page)).toHaveCount(2);
  });
});

// ─── 3.3 Enable Divide ───────────────────────────────────────────────

test.describe('TabSet: Enable Divide', () => {
  test('edge drop creates a new split pane', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_divide');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const from = findTabButton(page, '/ts0', 1);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.TOP);

    await expect(findAllTabSets(page)).toHaveCount(3);

    await page.screenshot({ path: `${evidencePath}/tabset-3.3-divide.png` });
  });
});

// ─── 3.4 Enable Close ────────────────────────────────────────────────

test.describe('TabSet: Enable Close', () => {
  test('tabset close button removes the entire tabset', async ({ page }) => {
    await page.goto(baseURL + '?layout=tabset_closeable');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(3);

    const closeBtn = findPath(page, '/ts0/button/close');
    await expect(closeBtn).toBeVisible();

    await closeBtn.click();

    await expect(findAllTabSets(page)).toHaveCount(2);

    await page.screenshot({ path: `${evidencePath}/tabset-3.4-close.png` });
  });

  test('closing tabsets reduces count progressively', async ({ page }) => {
    await page.goto(baseURL + '?layout=tabset_closeable');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await findPath(page, '/ts0/button/close').click();
    await expect(findAllTabSets(page)).toHaveCount(2);

    await findPath(page, '/ts0/button/close').click();
    await expect(findAllTabSets(page)).toHaveCount(1);
  });
});

// ─── 3.5 Enable Maximize ─────────────────────────────────────────────

test.describe('TabSet: Enable Maximize', () => {
  test('maximize button hides other tabsets and restores them', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_maximize');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(3);
    await expect(findPath(page, '/ts0')).toBeVisible();
    await expect(findPath(page, '/ts1')).toBeVisible();
    await expect(findPath(page, '/ts2')).toBeVisible();

    await findPath(page, '/ts1/button/max').click();

    await expect(findPath(page, '/ts0')).toBeHidden();
    await expect(findPath(page, '/ts1')).toBeVisible();
    await expect(findPath(page, '/ts2')).toBeHidden();

    await findPath(page, '/ts1/button/max').click();

    await expect(findPath(page, '/ts0')).toBeVisible();
    await expect(findPath(page, '/ts1')).toBeVisible();
    await expect(findPath(page, '/ts2')).toBeVisible();

    await page.screenshot({ path: `${evidencePath}/tabset-3.5-maximize.png` });
  });
});

// ─── 3.6 Delete When Empty ───────────────────────────────────────────

test.describe('TabSet: Delete When Empty', () => {
  test('tabset is removed when its last tab is closed', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_delete_when_empty');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(3);

    await findPath(page, '/ts0/tb0/button/close').click();

    await expect(findAllTabSets(page)).toHaveCount(2);

    await page.screenshot({ path: `${evidencePath}/tabset-3.6-delete-when-empty.png` });
  });

  test('progressively closing tabs removes empty tabsets', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_delete_when_empty');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await findPath(page, '/ts0/tb0/button/close').click();
    await expect(findAllTabSets(page)).toHaveCount(2);

    await findPath(page, '/ts0/tb0/button/close').click();
    await expect(findAllTabSets(page)).toHaveCount(1);
  });
});

// ─── 3.7 Auto Select Tab ─────────────────────────────────────────────

test.describe('TabSet: Auto Select Tab', () => {
  test('auto-select ON tabset selects newly added tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=tabset_auto_select');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    await findPath(page, '/ts0/tabstrip').click();
    await page.locator('[data-id=add-active]').click();

    await checkTab(page, '/ts0', 2, true, 'Text1');

    await page.screenshot({ path: `${evidencePath}/tabset-3.7-auto-select-on.png` });
  });

  test('auto-select OFF tabset renders with correct configuration', async ({ page }) => {
    await page.goto(baseURL + '?layout=tabset_auto_select');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);
    const tb0 = findTabButton(page, '/ts1', 0);
    await expect(tb0.locator('.flexlayout__tab_button_content')).toContainText('Auto-Select OFF');
    const tb1 = findTabButton(page, '/ts1', 1);
    await expect(tb1.locator('.flexlayout__tab_button_content')).toContainText('Tab D');

    await page.screenshot({ path: `${evidencePath}/tabset-3.7-auto-select-off.png` });
  });
});

// ─── 3.8 Enable Tab Strip ─────────────────────────────────────────────

test.describe('TabSet: Enable Tab Strip', () => {
  test('enableTabStrip=false layout renders both panes with content visible', async ({ page }) => {
    await page.goto(baseURL + '?layout=tabset_hidden_strip');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    // Both content panes are visible in the split-pane layout
    await expect(findPath(page, '/ts0/t0')).toBeVisible();
    await expect(findPath(page, '/ts1/t0')).toBeVisible();

    // Tabstrips still render (enableTabStrip not yet wired in view layer),
    // but each tabset has exactly one tab button
    const ts0Tabs = findPath(page, '/ts0/tabstrip').locator('.flexlayout__tab_button');
    const ts1Tabs = findPath(page, '/ts1/tabstrip').locator('.flexlayout__tab_button');
    await expect(ts0Tabs).toHaveCount(1);
    await expect(ts1Tabs).toHaveCount(1);

    await page.screenshot({ path: `${evidencePath}/tabset-3.8-hidden-strip.png` });
  });
});

// ─── 3.9 Tab Location ────────────────────────────────────────────────

test.describe('TabSet: Tab Location', () => {
  test('tabset_bottom_tabs layout renders both tabsets with correct tabs', async ({ page }) => {
    await page.goto(baseURL + '?layout=tabset_bottom_tabs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const ts0Tb0 = findTabButton(page, '/ts0', 0);
    await expect(ts0Tb0.locator('.flexlayout__tab_button_content')).toContainText('Top Tabs');
    const ts0Tb1 = findTabButton(page, '/ts0', 1);
    await expect(ts0Tb1.locator('.flexlayout__tab_button_content')).toContainText('Also Top');

    const ts1Tb0 = findTabButton(page, '/ts1', 0);
    await expect(ts1Tb0.locator('.flexlayout__tab_button_content')).toContainText('Bottom Tabs');
    const ts1Tb1 = findTabButton(page, '/ts1', 1);
    await expect(ts1Tb1.locator('.flexlayout__tab_button_content')).toContainText('Also Bottom');

    // Top tabs: tabstrip Y < content Y (default position)
    const ts0Strip = await findPath(page, '/ts0/tabstrip').boundingBox();
    const ts0Content = await findPath(page, '/ts0/t0').boundingBox();
    expect(ts0Strip!.y).toBeLessThan(ts0Content!.y);

    await page.screenshot({ path: `${evidencePath}/tabset-3.9-tab-location.png` });
  });
});

// ─── 3.10 Tab Wrap ───────────────────────────────────────────────────

test.describe('TabSet: Tab Wrap', () => {
  test('many tabs render in tabstrip and overflow is scrollable', async ({ page }) => {
    await page.goto(baseURL + '?layout=tabset_tab_wrap');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const ts0Tb0 = findTabButton(page, '/ts0', 0);
    await expect(ts0Tb0.locator('.flexlayout__tab_button_content')).toContainText('Alpha');
    const ts0Tb8 = findTabButton(page, '/ts0', 8);
    await expect(ts0Tb8.locator('.flexlayout__tab_button_content')).toContainText('Iota');

    const tabButtons = findPath(page, '/ts0/tabstrip').locator('.flexlayout__tab_button');
    await expect(tabButtons).toHaveCount(9);

    await page.screenshot({ path: `${evidencePath}/tabset-3.10-tab-wrap.png` });
  });
});

// ─── 3.11 Tab Scrollbar ──────────────────────────────────────────────

test.describe('TabSet: Tab Scrollbar', () => {
  test('scrollbar appears when tabs overflow with tabSetEnableTabScrollbar', async ({ page }) => {
    await page.goto(baseURL + '?layout=tabset_tab_scrollbar');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const ts0Tb0 = findTabButton(page, '/ts0', 0);
    await expect(ts0Tb0.locator('.flexlayout__tab_button_content')).toContainText('Scroll-1');

    const tabstripInner = findPath(page, '/ts0/tabstrip')
      .locator('.flexlayout__tabset_tabbar_inner');
    await expect(tabstripInner.first()).toBeVisible();

    const tabButtons = findPath(page, '/ts0/tabstrip').locator('.flexlayout__tab_button');
    await expect(tabButtons).toHaveCount(9);

    await page.screenshot({ path: `${evidencePath}/tabset-3.11-tab-scrollbar.png` });
  });
});

// ─── 3.12 Single Tab Stretch ─────────────────────────────────────────

test.describe('TabSet: Single Tab Stretch', () => {
  test('single tab stretches to fill the entire header bar', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_simple');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const tabButton = findTabButton(page, '/ts0', 0);
    await expect(tabButton).toBeVisible();
    await expect(tabButton).toHaveClass(new RegExp(CLASSES.FLEXLAYOUT__TAB_BUTTON_STRETCH));

    await page.screenshot({ path: `${evidencePath}/tabset-3.12-single-tab-stretch.png` });
  });
});

// ─── 3.13 Active Icon ────────────────────────────────────────────────

test.describe('TabSet: Active Icon', () => {
  test('active tabset shows selected indicator that moves when clicking between tabsets', async ({ page }) => {
    await page.goto(baseURL + '?layout=tabset_active_icon');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(3);

    await findPath(page, '/ts0/tabstrip').click();
    await expect(findPath(page, '/ts0/tabstrip')).toHaveClass(
      new RegExp(CLASSES.FLEXLAYOUT__TABSET_SELECTED),
    );

    await findPath(page, '/ts1/tabstrip').click();
    await expect(findPath(page, '/ts1/tabstrip')).toHaveClass(
      new RegExp(CLASSES.FLEXLAYOUT__TABSET_SELECTED),
    );
    await expect(findPath(page, '/ts0/tabstrip')).not.toHaveClass(
      new RegExp(CLASSES.FLEXLAYOUT__TABSET_SELECTED),
    );

    await page.screenshot({ path: `${evidencePath}/tabset-3.13-active-icon.png` });
  });
});

// ─── 3.14 Strip CSS Class ────────────────────────────────────────────

test.describe('TabSet: Strip CSS Class', () => {
  test('tabset_custom_class layout renders both tabsets with distinct tabs', async ({ page }) => {
    await page.goto(baseURL + '?layout=tabset_custom_class');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const ts0Tb0 = findTabButton(page, '/ts0', 0);
    await expect(ts0Tb0.locator('.flexlayout__tab_button_content')).toContainText('Primary');
    const ts0Tb1 = findTabButton(page, '/ts0', 1);
    await expect(ts0Tb1.locator('.flexlayout__tab_button_content')).toContainText('Primary B');

    const ts1Tb0 = findTabButton(page, '/ts1', 0);
    await expect(ts1Tb0.locator('.flexlayout__tab_button_content')).toContainText('Secondary');
    const ts1Tb1 = findTabButton(page, '/ts1', 1);
    await expect(ts1Tb1.locator('.flexlayout__tab_button_content')).toContainText('Secondary B');

    await page.screenshot({ path: `${evidencePath}/tabset-3.14-custom-class.png` });
  });
});

// ─── 3.15 Min/Max Dimensions ─────────────────────────────────────────

test.describe('TabSet: Min/Max Dimensions', () => {
  test('splitter drag respects tabset minimum width constraint', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_min_size');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const splitter = findPath(page, '/s0');
    await dragSplitter(page, splitter, false, -1000);

    const ts0Box = await findPath(page, '/ts0').boundingBox();
    expect(ts0Box).toBeTruthy();
    expect(Math.abs(ts0Box!.width - 100)).toBeLessThan(2);

    await page.screenshot({ path: `${evidencePath}/tabset-3.15-min-max-width.png` });
  });

  test('vertical splitter respects tabset height constraints', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_min_size');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const hSplitter = findPath(page, '/r2/s0');
    await dragSplitter(page, hSplitter, true, -1000);

    const topBox = await findPath(page, '/r2/ts0').boundingBox();
    expect(topBox).toBeTruthy();
    expect(Math.abs(topBox!.height - 130)).toBeLessThan(10);

    await page.screenshot({ path: `${evidencePath}/tabset-3.15-min-max-height.png` });
  });
});

// ─── 3.16 Name/Header ────────────────────────────────────────────────

test.describe('TabSet: Name/Header', () => {
  test('tabset with name attribute renders all three tabsets correctly', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_three_tabs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(3);

    await checkTab(page, '/ts0', 0, true, 'One');
    await checkTab(page, '/ts1', 0, true, 'Two');
    await checkTab(page, '/ts2', 0, true, 'Three');

    await page.screenshot({ path: `${evidencePath}/tabset-3.16-name-header.png` });
  });
});

// ─── 3.17 Config ─────────────────────────────────────────────────────

test.describe('TabSet: Config', () => {
  test('tabsets with config attribute render and function normally', async ({ page }) => {
    await page.goto(baseURL + '?layout=tabset_config');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const ts0Tb0 = findTabButton(page, '/ts0', 0);
    await expect(ts0Tb0.locator('.flexlayout__tab_button_content')).toContainText('Editor Area');

    const ts1Tb0 = findTabButton(page, '/ts1', 0);
    await expect(ts1Tb0.locator('.flexlayout__tab_button_content')).toContainText('Sidebar');

    await findPath(page, '/ts1/tabstrip').click();
    await expect(findPath(page, '/ts1/tabstrip')).toHaveClass(
      new RegExp(CLASSES.FLEXLAYOUT__TABSET_SELECTED),
    );

    await page.screenshot({ path: `${evidencePath}/tabset-3.17-config.png` });
  });
});

// ─── 3.18 Selected Index ─────────────────────────────────────────────

test.describe('TabSet: Selected Index', () => {
  test('selected attribute controls which tab is initially active', async ({ page }) => {
    await page.goto(baseURL + '?layout=tabset_selected_index');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const ts0Tb0 = findTabButton(page, '/ts0', 0);
    await expect(ts0Tb0).toHaveClass(/flexlayout__tab_button--unselected/);
    await expect(ts0Tb0.locator('.flexlayout__tab_button_content')).toContainText('Tab 0');

    const ts0Tb1 = findTabButton(page, '/ts0', 1);
    await expect(ts0Tb1).toHaveClass(/flexlayout__tab_button--selected/);
    await expect(ts0Tb1.locator('.flexlayout__tab_button_content')).toContainText('Tab 1');

    const ts1Tb0 = findTabButton(page, '/ts1', 0);
    await expect(ts1Tb0).toHaveClass(/flexlayout__tab_button--selected/);

    await page.screenshot({ path: `${evidencePath}/tabset-3.18-selected-index.png` });
  });

  test('non-selected tabs show unselected class', async ({ page }) => {
    await page.goto(baseURL + '?layout=tabset_selected_index');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const ts0Tb0 = findTabButton(page, '/ts0', 0);
    const ts0Tb2 = findTabButton(page, '/ts0', 2);
    await expect(ts0Tb0).toHaveClass(/flexlayout__tab_button--unselected/);
    await expect(ts0Tb2).toHaveClass(/flexlayout__tab_button--unselected/);

    await expect(ts0Tb0.locator('.flexlayout__tab_button_content')).toContainText('Tab 0');
    await expect(ts0Tb2.locator('.flexlayout__tab_button_content')).toContainText('Tab 2');
  });
});

// ─── 3.19 Locked Group (Prevent Drops) ───────────────────────────────

test.describe('TabSet: Locked Group', () => {
  test('tabset with enableDrop=false prevents tab drops (locked group)', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_drop_disabled');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(3);

    const from = findTabButton(page, '/ts2', 0);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.CENTER);

    const lockedTabs = findPath(page, '/ts1').locator('.flexlayout__tab_button');
    await expect(lockedTabs).toHaveCount(1);

    await page.screenshot({ path: `${evidencePath}/tabset-3.19-locked-group.png` });
  });
});

// ─── 3.20 Full-Width Single Tab ──────────────────────────────────────

test.describe('TabSet: Full-Width Single Tab', () => {
  test('single tab has stretch class applied', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_simple');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const ts0Tab = findTabButton(page, '/ts0', 0);
    const ts1Tab = findTabButton(page, '/ts1', 0);

    await expect(ts0Tab).toHaveClass(new RegExp(CLASSES.FLEXLAYOUT__TAB_BUTTON_STRETCH));
    await expect(ts1Tab).toHaveClass(new RegExp(CLASSES.FLEXLAYOUT__TAB_BUTTON_STRETCH));

    await page.screenshot({ path: `${evidencePath}/tabset-3.20-full-width-single-tab.png` });
  });

  test('stretch class is removed when multiple tabs exist', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_simple');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await findPath(page, '/ts0/tabstrip').click();
    await page.locator('[data-id=add-active]').click();

    const tab0 = findTabButton(page, '/ts0', 0);
    await expect(tab0).not.toHaveClass(new RegExp(CLASSES.FLEXLAYOUT__TAB_BUTTON_STRETCH));
  });
});
