import { test, expect } from '@playwright/test';
import {
  findPath,
  findTabButton,
  findAllTabSets,
} from './helpers';

const baseURL = '/flexlayout-test.html';

// ─── 13.1 Nested Layouts ──────────────────────────────────────────────

test.describe('Advanced: Nested Layouts (13.1)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=stress_sub_layout');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('sub-layout renders recursive Layout components inside tabs', async ({ page }) => {
    // stress_sub_layout has 2 outer tabsets + 2 nested tabsets (one per "nested" component) = 4 total
    await expect(findAllTabSets(page)).toHaveCount(4);

    // Verify top-level tab buttons using .first() since nested layouts duplicate paths
    await expect(
      findTabButton(page, '/ts0', 0).first().locator('.flexlayout__tab_button_content'),
    ).toContainText('NestedLeft');
    await expect(
      findTabButton(page, '/ts0', 1).locator('.flexlayout__tab_button_content'),
    ).toContainText('InfoLeft');
    await expect(
      findTabButton(page, '/ts1', 0).first().locator('.flexlayout__tab_button_content'),
    ).toContainText('NestedRight');
    await expect(
      findTabButton(page, '/ts1', 1).locator('.flexlayout__tab_button_content'),
    ).toContainText('InfoRight');
  });

  test('nested tab panels contain a recursive flexlayout instance', async ({ page }) => {
    // The "nested" component renders a <Layout> inside a div[data-testid="panel-NestedLeft"]
    const nestedPanel = page.locator('[data-testid="panel-NestedLeft"]');
    await expect(nestedPanel).toBeVisible();

    // The nested layout should contain its own flexlayout__layout container
    const nestedLayout = nestedPanel.locator('.flexlayout__layout');
    await expect(nestedLayout).toBeVisible();

    // The nested layout has a "Nested Tab" with component "testing"
    const nestedTabsets = nestedPanel.locator('.flexlayout__tabset');
    await expect(nestedTabsets).toHaveCount(1);
  });

  test('both nested tabs render independent layout instances', async ({ page }) => {
    const leftPanel = page.locator('[data-testid="panel-NestedLeft"]');
    const rightPanel = page.locator('[data-testid="panel-NestedRight"]');

    await expect(leftPanel).toBeVisible();
    await expect(rightPanel).toBeVisible();

    // Each has its own flexlayout__layout root
    await expect(leftPanel.locator('.flexlayout__layout')).toBeVisible();
    await expect(rightPanel.locator('.flexlayout__layout')).toBeVisible();

    // Each has its own tabset
    await expect(leftPanel.locator('.flexlayout__tabset')).toHaveCount(1);
    await expect(rightPanel.locator('.flexlayout__tabset')).toHaveCount(1);
  });

  test('switching to info tab hides nested layout and shows info content', async ({ page }) => {
    // Click InfoLeft tab to switch from NestedLeft
    await findTabButton(page, '/ts0', 1).dispatchEvent('click');

    // InfoLeft content should be visible
    const infoContent = findPath(page, '/ts0/t1');
    await expect(infoContent).toBeVisible();

    // NestedLeft panel should be hidden (use data-testid to avoid path collision with nested layout)
    const nestedPanel = page.locator('[data-testid="panel-NestedLeft"]');
    await expect(nestedPanel).not.toBeVisible();
  });
});

// ─── 13.2 Render On Demand (Global) ──────────────────────────────────

test.describe('Advanced: Render On Demand (13.2)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=advanced_render_on_demand');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('layout loads with render-on-demand enabled globally', async ({ page }) => {
    // 2 tabsets in main layout + bottom border
    await expect(findAllTabSets(page)).toHaveCount(2);

    // First tab "Lazy Tab A" is selected and visible
    await expect(
      findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content'),
    ).toContainText('Lazy Tab A');
    await expect(findPath(page, '/ts0/t0')).toBeVisible();
  });

  test('unselected tabs are not rendered in DOM with render-on-demand', async ({ page }) => {
    // "Lazy Tab A" is selected (t0), "Lazy Tab B" (t1) and "Lazy Tab C" (t2) are not selected
    await expect(findPath(page, '/ts0/t0')).toBeVisible();

    // With tabEnableRenderOnDemand: true, unselected tab content should not be visible
    await expect(findPath(page, '/ts0/t1')).not.toBeVisible();
    await expect(findPath(page, '/ts0/t2')).not.toBeVisible();
  });

  test('clicking an unselected tab renders it on demand', async ({ page }) => {
    // "Lazy Tab B" is unselected
    await expect(findPath(page, '/ts0/t1')).not.toBeVisible();

    // Click to select "Lazy Tab B"
    await findTabButton(page, '/ts0', 1).dispatchEvent('click');

    // Now "Lazy Tab B" should be visible
    await expect(findPath(page, '/ts0/t1')).toBeVisible();

    // And "Lazy Tab A" should no longer be visible
    await expect(findPath(page, '/ts0/t0')).not.toBeVisible();
  });

  test('render-on-demand applies to all 3 tabs in the tabset', async ({ page }) => {
    // Verify all 3 tab buttons exist
    await expect(
      findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content'),
    ).toContainText('Lazy Tab A');
    await expect(
      findTabButton(page, '/ts0', 1).locator('.flexlayout__tab_button_content'),
    ).toContainText('Lazy Tab B');
    await expect(
      findTabButton(page, '/ts0', 2).locator('.flexlayout__tab_button_content'),
    ).toContainText('Lazy Tab C');

    // Reference tabset also has its tab
    await expect(
      findTabButton(page, '/ts1', 0).locator('.flexlayout__tab_button_content'),
    ).toContainText('Reference');
    await expect(findPath(page, '/ts1/t0')).toBeVisible();
  });

  test('border tab is also subject to global render-on-demand', async ({ page }) => {
    // Bottom border has "Lazy Border" tab
    const borderTab = findTabButton(page, '/border/bottom', 0);
    await expect(borderTab).toBeVisible();
    await expect(
      borderTab.locator('.flexlayout__border_button_content'),
    ).toContainText('Lazy Border');

    // Border tab content should not be visible initially (border closed)
    await expect(findPath(page, '/border/bottom/t0')).not.toBeVisible();

    // Click to open border — tab content should render on demand
    await borderTab.click();
    await expect(findPath(page, '/border/bottom/t0')).toBeVisible();
  });
});

// ─── 13.3 Complex Hierarchies ────────────────────────────────────────

test.describe('Advanced: Complex Hierarchies (13.3)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=stress_complex');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('deep nesting: 4+ levels of rows and tabsets', async ({ page }) => {
    // Level 0: root has ts0 + r1 + r2
    await expect(findPath(page, '/ts0')).toBeVisible();
    await expect(findPath(page, '/r1')).toBeVisible();
    await expect(findPath(page, '/r2')).toBeVisible();

    // Level 1: r1 -> ts0 (Editor) + r1 (nested row)
    await expect(findPath(page, '/r1/ts0')).toBeVisible();
    await expect(findPath(page, '/r1/r1')).toBeVisible();

    // Level 2: r1/r1 -> two tabsets (Deep*)
    await expect(findPath(page, '/r1/r1/ts0')).toBeVisible();
    await expect(findPath(page, '/r1/r1/ts1')).toBeVisible();

    // Level 1: r2 -> two tabsets (Panel, Console)
    await expect(findPath(page, '/r2/ts0')).toBeVisible();
    await expect(findPath(page, '/r2/ts1')).toBeVisible();
  });

  test('complex hierarchy has correct total tabset count including float', async ({ page }) => {
    // 6 main tabsets + 1 float window tabset = 7
    await expect(findAllTabSets(page)).toHaveCount(7);
  });

  test('all 4 border locations have tabs in complex layout', async ({ page }) => {
    await expect(findTabButton(page, '/border/top', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/top', 1)).toBeVisible();
    await expect(findTabButton(page, '/border/bottom', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/bottom', 1)).toBeVisible();
    await expect(findTabButton(page, '/border/left', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/right', 0)).toBeVisible();
  });

  test('deeply nested tabsets have proper tab names', async ({ page }) => {
    // Level 2 deep tabs
    await expect(
      findTabButton(page, '/r1/r1/ts0', 0).locator('.flexlayout__tab_button_content'),
    ).toContainText('DeepA');
    await expect(
      findTabButton(page, '/r1/r1/ts0', 1).locator('.flexlayout__tab_button_content'),
    ).toContainText('DeepB');
    await expect(
      findTabButton(page, '/r1/r1/ts1', 0).locator('.flexlayout__tab_button_content'),
    ).toContainText('DeepD');
    await expect(
      findTabButton(page, '/r1/r1/ts1', 1).locator('.flexlayout__tab_button_content'),
    ).toContainText('DeepE');
  });

  test('float window coexists with complex main layout', async ({ page }) => {
    // Float panel should be visible
    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    // Float has its own tabset with Float1 and Float2 tabs
    const floatTabset = floatPanel.locator('.flexlayout__tabset');
    await expect(floatTabset).toHaveCount(1);
  });
});

// ─── 13.4 Performance: Large Layouts ─────────────────────────────────

test.describe('Advanced: Performance — Large Layouts (13.4)', () => {
  test('stress_complex with 30+ tabs renders within timeout', async ({ page }) => {
    const start = Date.now();
    await page.goto(baseURL + '?layout=stress_complex');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    const elapsed = Date.now() - start;

    // Layout should render in under 5 seconds
    expect(elapsed).toBeLessThan(5000);

    // Verify all 7 tabsets rendered
    await expect(findAllTabSets(page)).toHaveCount(7);
  });

  test('tab count across all tabsets in large layout', async ({ page }) => {
    await page.goto(baseURL + '?layout=stress_complex');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Count all tab buttons across all tabsets (main + float)
    const allTabButtons = page.locator('.flexlayout__tab_button');
    const count = await allTabButtons.count();

    // stress_complex has 4+4+3+3+3+5+2 = 24 main tabs + 2 float tabs = 26 tab buttons
    // Border tabs show as border buttons, not tab buttons
    expect(count).toBeGreaterThanOrEqual(20);
  });

  test('many-tab tabset (Console with 5 tabs) renders all tab buttons', async ({ page }) => {
    await page.goto(baseURL + '?layout=stress_complex');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // r2/ts1 is the Console tabset with 5 tabs
    await expect(
      findTabButton(page, '/r2/ts1', 0).locator('.flexlayout__tab_button_content'),
    ).toContainText('Console1');
    await expect(
      findTabButton(page, '/r2/ts1', 1).locator('.flexlayout__tab_button_content'),
    ).toContainText('Console2');
    await expect(
      findTabButton(page, '/r2/ts1', 2).locator('.flexlayout__tab_button_content'),
    ).toContainText('Console3');
    await expect(
      findTabButton(page, '/r2/ts1', 3).locator('.flexlayout__tab_button_content'),
    ).toContainText('Console4');
    await expect(
      findTabButton(page, '/r2/ts1', 4).locator('.flexlayout__tab_button_content'),
    ).toContainText('Console5');
  });

  test('tab selection in large layout responds interactively', async ({ page }) => {
    await page.goto(baseURL + '?layout=stress_complex');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Click Nav2 tab in ts0 (second tab)
    const nav2 = findTabButton(page, '/ts0', 1);
    await expect(nav2.locator('.flexlayout__tab_button_content')).toContainText('Nav2');

    await nav2.dispatchEvent('click');

    // Nav2 should now be selected
    await expect(nav2).toHaveClass(/flexlayout__tab_button--selected/);

    // Nav1 should now be unselected
    await expect(findTabButton(page, '/ts0', 0)).toHaveClass(
      /flexlayout__tab_button--unselected/,
    );
  });

  test('splitters in complex hierarchy are interactive', async ({ page }) => {
    await page.goto(baseURL + '?layout=stress_complex');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Verify splitters exist between top-level elements
    const splitter0 = findPath(page, '/s0');
    await expect(splitter0).toBeVisible();

    // Splitter should have a reasonable size (not zero)
    const splitterBox = await splitter0.boundingBox();
    expect(splitterBox).toBeTruthy();
    expect(splitterBox!.width).toBeGreaterThan(0);
    expect(splitterBox!.height).toBeGreaterThan(0);
  });
});
