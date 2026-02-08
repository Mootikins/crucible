import { test, expect } from '@playwright/test';
import {
  findPath,
  findTabButton,
  findAllTabSets,
  checkTab,
  checkBorderTab,
} from './helpers';

const baseURL = '/flexlayout-test.html';

// ─── 1.1 Row Containers ──────────────────────────────────────────────

test.describe('Layout Structure: Row Containers', () => {
  test('basic_simple renders a horizontal root row with two tabsets side-by-side', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_simple');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findPath(page, '/')).toHaveClass(/flexlayout__layout/);

    await expect(findAllTabSets(page)).toHaveCount(2);

    // Verify they are laid out horizontally: ts0 left of ts1
    const box0 = await findPath(page, '/ts0').boundingBox();
    const box1 = await findPath(page, '/ts1').boundingBox();
    expect(box0).toBeTruthy();
    expect(box1).toBeTruthy();
    // ts0 is to the left of ts1
    expect(box0!.x + box0!.width).toBeLessThanOrEqual(box1!.x + 2);
    // Both on same vertical level (horizontal layout)
    expect(Math.abs(box0!.y - box1!.y)).toBeLessThan(5);
  });

  test('basic_vertical_root renders a vertical root row with tabsets stacked top-to-bottom', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_vertical_root');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findPath(page, '/')).toHaveClass(/flexlayout__layout/);

    await expect(findAllTabSets(page)).toHaveCount(3);

    const box0 = await findPath(page, '/ts0').boundingBox();
    const box1 = await findPath(page, '/ts1').boundingBox();
    const box2 = await findPath(page, '/ts2').boundingBox();
    expect(box0).toBeTruthy();
    expect(box1).toBeTruthy();
    expect(box2).toBeTruthy();

    // Vertical stacking: ts0 above ts1 above ts2
    expect(box0!.y + box0!.height).toBeLessThanOrEqual(box1!.y + 2);
    expect(box1!.y + box1!.height).toBeLessThanOrEqual(box2!.y + 2);
    // All share the same x position (vertical layout)
    expect(Math.abs(box0!.x - box1!.x)).toBeLessThan(5);
    expect(Math.abs(box1!.x - box2!.x)).toBeLessThan(5);
  });
});

// ─── 1.2 TabSet Containers ───────────────────────────────────────────

test.describe('Layout Structure: TabSet Containers', () => {
  test('tabsets contain tab buttons in tabstrip and corresponding tab content panels', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findPath(page, '/ts0/tabstrip')).toBeVisible();
    await expect(findPath(page, '/ts1/tabstrip')).toBeVisible();

    await checkTab(page, '/ts0', 0, true, 'One');
    await checkTab(page, '/ts1', 0, true, 'Two');

    await expect(findPath(page, '/ts0')).toHaveClass(/flexlayout__tabset/);
    await expect(findPath(page, '/ts1')).toHaveClass(/flexlayout__tabset/);
  });

  test('tabset with multiple tabs shows only the selected tab content', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // ts0 has tab "One" (selected), ts1 has tab "Two" (selected), ts2 has tab "Three" (selected)
    await expect(findAllTabSets(page)).toHaveCount(3);
    await checkTab(page, '/ts0', 0, true, 'One');
    await checkTab(page, '/ts1', 0, true, 'Two');
    await checkTab(page, '/ts2', 0, true, 'Three');
  });
});

// ─── 1.3 Weight-based Sizing ─────────────────────────────────────────

test.describe('Layout Structure: Weight-based Sizing', () => {
  test('equal-weight tabsets have approximately equal widths', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_weights');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const box0 = await findPath(page, '/ts0').boundingBox();
    const box1 = await findPath(page, '/ts1').boundingBox();
    expect(box0).toBeTruthy();
    expect(box1).toBeTruthy();

    // Both tabsets have weight: 50, so widths should be roughly equal
    const widthRatio = box0!.width / box1!.width;
    expect(widthRatio).toBeGreaterThan(0.85);
    expect(widthRatio).toBeLessThan(1.15);
  });

  test('action buttons change weight proportions to 80/20', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_weights');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Click the 80/20 action button
    await page.locator('[data-id="action-weights-8020"]').click();

    const box0 = await findPath(page, '/ts0').boundingBox();
    const box1 = await findPath(page, '/ts1').boundingBox();
    expect(box0).toBeTruthy();
    expect(box1).toBeTruthy();

    // First tabset should be approximately 4x wider than second
    const ratio = box0!.width / box1!.width;
    expect(ratio).toBeGreaterThan(2.5);
  });

  test('equal weights action restores proportional sizing', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_weights');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Set to 80/20 first
    await page.locator('[data-id="action-weights-8020"]').click();

    // Then restore to equal
    await page.locator('[data-id="action-equal-weights"]').click();

    const box0 = await findPath(page, '/ts0').boundingBox();
    const box1 = await findPath(page, '/ts1').boundingBox();
    expect(box0).toBeTruthy();
    expect(box1).toBeTruthy();

    const widthRatio = box0!.width / box1!.width;
    expect(widthRatio).toBeGreaterThan(0.85);
    expect(widthRatio).toBeLessThan(1.15);
  });
});

// ─── 1.4 Root Orientation Vertical ───────────────────────────────────

test.describe('Layout Structure: Root Orientation Vertical', () => {
  test('vertical root stacks tabsets top-to-bottom instead of left-to-right', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_vertical_root');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Verify tab button names (info component doesn't render tab name in content panel)
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Top');
    await expect(findTabButton(page, '/ts1', 0).locator('.flexlayout__tab_button_content')).toContainText('Middle');
    await expect(findTabButton(page, '/ts2', 0).locator('.flexlayout__tab_button_content')).toContainText('Bottom');

    const rootBox = await findPath(page, '/').boundingBox();
    const box0 = await findPath(page, '/ts0').boundingBox();
    const box1 = await findPath(page, '/ts1').boundingBox();
    expect(rootBox).toBeTruthy();
    expect(box0).toBeTruthy();
    expect(box1).toBeTruthy();

    const widthRatio0 = box0!.width / rootBox!.width;
    expect(widthRatio0).toBeGreaterThan(0.95);

    const widthRatio1 = box1!.width / rootBox!.width;
    expect(widthRatio1).toBeGreaterThan(0.95);
  });

  test('vertical root tabsets have approximately equal heights', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_vertical_root');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const box0 = await findPath(page, '/ts0').boundingBox();
    const box1 = await findPath(page, '/ts1').boundingBox();
    const box2 = await findPath(page, '/ts2').boundingBox();
    expect(box0).toBeTruthy();
    expect(box1).toBeTruthy();
    expect(box2).toBeTruthy();

    // Weights are 33/34/33 — heights should be roughly equal
    const avgHeight = (box0!.height + box1!.height + box2!.height) / 3;
    expect(Math.abs(box0!.height - avgHeight)).toBeLessThan(avgHeight * 0.15);
    expect(Math.abs(box1!.height - avgHeight)).toBeLessThan(avgHeight * 0.15);
    expect(Math.abs(box2!.height - avgHeight)).toBeLessThan(avgHeight * 0.15);
  });
});

// ─── 1.5 Nested Rows ─────────────────────────────────────────────────

test.describe('Layout Structure: Nested Rows', () => {
  test('stress_complex has 4+ levels of row nesting', async ({ page }) => {
    await page.goto(baseURL + '?layout=stress_complex');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Level 0: tabset (ts0) + row (r1) + row (r2)
    await expect(findPath(page, '/ts0')).toBeVisible();
    await expect(findPath(page, '/r1')).toBeVisible();
    await expect(findPath(page, '/r2')).toBeVisible();

    // Level 1: r1 → tabset (Editor) + nested row
    await expect(findPath(page, '/r1/ts0')).toBeVisible();
    await expect(findPath(page, '/r1/r1')).toBeVisible();

    // Level 2: r1/r1 → two tabsets (DeepA-C, DeepD-F)
    await expect(findPath(page, '/r1/r1/ts0')).toBeVisible();
    await expect(findPath(page, '/r1/r1/ts1')).toBeVisible();

    await expect(findTabButton(page, '/r1/r1/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('DeepA');
    await expect(findTabButton(page, '/r1/r1/ts1', 0).locator('.flexlayout__tab_button_content')).toContainText('DeepD');
  });

  test('stress_complex has tabsets across nested hierarchy plus float window', async ({ page }) => {
    await page.goto(baseURL + '?layout=stress_complex');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // 6 main tabsets + 1 float window tabset = 7
    await expect(findAllTabSets(page)).toHaveCount(7);
  });
});

// ─── 1.6 Border Panels ───────────────────────────────────────────────

test.describe('Layout Structure: Border Panels', () => {
  test('test_with_borders has all 4 border locations configured', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // All four borders should have tab buttons visible
    await expect(findTabButton(page, '/border/top', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/bottom', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/left', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/right', 0)).toBeVisible();

    // Verify border tab names
    await expect(findTabButton(page, '/border/top', 0).locator('.flexlayout__border_button_content')).toContainText('top1');
    await expect(findTabButton(page, '/border/bottom', 0).locator('.flexlayout__border_button_content')).toContainText('bottom1');
    await expect(findTabButton(page, '/border/left', 0).locator('.flexlayout__border_button_content')).toContainText('left1');
    await expect(findTabButton(page, '/border/right', 0).locator('.flexlayout__border_button_content')).toContainText('right1');
  });

  test('border panels start closed and do not occupy main layout space', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Border tab content panels should not be visible initially
    await expect(findPath(page, '/border/top/t0')).not.toBeVisible();
    await expect(findPath(page, '/border/bottom/t0')).not.toBeVisible();
    await expect(findPath(page, '/border/left/t0')).not.toBeVisible();
    await expect(findPath(page, '/border/right/t0')).not.toBeVisible();

    // The main layout tabsets should be visible and occupy the central area
    await expect(findAllTabSets(page)).toHaveCount(3);
    await expect(findPath(page, '/ts0')).toBeVisible();
    await expect(findPath(page, '/ts1')).toBeVisible();
    await expect(findPath(page, '/ts2')).toBeVisible();
  });

  test('stress_complex borders include tabs on all 4 sides', async ({ page }) => {
    await page.goto(baseURL + '?layout=stress_complex');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // stress_complex has top (2 tabs), bottom (2 tabs), left (1 tab), right (1 tab) borders
    await expect(findTabButton(page, '/border/top', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/top', 1)).toBeVisible();
    await expect(findTabButton(page, '/border/bottom', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/bottom', 1)).toBeVisible();
    await expect(findTabButton(page, '/border/left', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/right', 0)).toBeVisible();
  });
});

// ─── 1.7 Global Config Inheritance ───────────────────────────────────

test.describe('Layout Structure: Global Config Inheritance', () => {
  test('tabCloseType from global config applies to all tabs', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Default global has tabCloseType: 1, meaning close buttons are visible on tabs
    // Verify close buttons exist on tab buttons
    await expect(findPath(page, '/ts0/tb0/button/close')).toBeVisible();
    await expect(findPath(page, '/ts1/tb0/button/close')).toBeVisible();
    await expect(findPath(page, '/ts2/tb0/button/close')).toBeVisible();
  });

  test('rootOrientationVertical global setting changes layout direction', async ({ page }) => {
    // basic_simple: no rootOrientationVertical (horizontal layout)
    await page.goto(baseURL + '?layout=basic_simple');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const hBox0 = await findPath(page, '/ts0').boundingBox();
    const hBox1 = await findPath(page, '/ts1').boundingBox();
    expect(hBox0).toBeTruthy();
    expect(hBox1).toBeTruthy();
    // Horizontal: ts0 left of ts1
    expect(hBox0!.x).toBeLessThan(hBox1!.x);
    expect(Math.abs(hBox0!.y - hBox1!.y)).toBeLessThan(5);

    // basic_vertical_root: rootOrientationVertical: true (vertical layout)
    await page.goto(baseURL + '?layout=basic_vertical_root');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const vBox0 = await findPath(page, '/ts0').boundingBox();
    const vBox1 = await findPath(page, '/ts1').boundingBox();
    expect(vBox0).toBeTruthy();
    expect(vBox1).toBeTruthy();
    // Vertical: ts0 above ts1
    expect(vBox0!.y).toBeLessThan(vBox1!.y);
    expect(Math.abs(vBox0!.x - vBox1!.x)).toBeLessThan(5);
  });

  test('borderAutoSelectTabWhenOpen global config applies to borders', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Global config has borderAutoSelectTabWhenOpen: true
    // Opening a border should auto-select the first tab
    const borderTab = findTabButton(page, '/border/top', 0);
    await borderTab.click();

    // Tab content should be visible (auto-selected)
    await expect(findPath(page, '/border/top/t0')).toBeVisible();
    await checkBorderTab(page, '/border/top', 0, true, 'top1');
  });
});

// ─── 1.8 JSON Model ──────────────────────────────────────────────────

test.describe('Layout Structure: JSON Model', () => {
  const checkTabButton = async (page: import('@playwright/test').Page, path: string, index: number, text: string) => {
    const btn = findTabButton(page, path, index);
    await expect(btn).toBeVisible();
    await expect(btn.locator('.flexlayout__tab_button_content')).toContainText(text);
  };

  test('basic_serialization layout loads from JSON model definition', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_serialization');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(3);

    await checkTabButton(page, '/ts0', 0, 'Layout Info');
    await checkTabButton(page, '/ts1', 0, 'Serialize');
    await checkTabButton(page, '/ts2', 0, 'Restore');
  });

  test('reload button restores layout from original JSON model', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_serialization');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(3);
    await checkTabButton(page, '/ts0', 0, 'Layout Info');

    await findPath(page, '/ts1/tb0/button/close').click();
    await expect(findAllTabSets(page)).toHaveCount(2);

    await page.locator('[data-id=reload]').click();

    await expect(findAllTabSets(page)).toHaveCount(3);
    await checkTabButton(page, '/ts0', 0, 'Layout Info');
    await checkTabButton(page, '/ts1', 0, 'Serialize');
    await checkTabButton(page, '/ts2', 0, 'Restore');
  });

  test('JSON model preserves weight-based proportions', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_serialization');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // basic_serialization uses weights 33/34/33
    const box0 = await findPath(page, '/ts0').boundingBox();
    const box1 = await findPath(page, '/ts1').boundingBox();
    const box2 = await findPath(page, '/ts2').boundingBox();
    expect(box0).toBeTruthy();
    expect(box1).toBeTruthy();
    expect(box2).toBeTruthy();

    // All three should be roughly equal width (33/34/33 weights)
    const avgWidth = (box0!.width + box1!.width + box2!.width) / 3;
    expect(Math.abs(box0!.width - avgWidth)).toBeLessThan(avgWidth * 0.15);
    expect(Math.abs(box1!.width - avgWidth)).toBeLessThan(avgWidth * 0.15);
    expect(Math.abs(box2!.width - avgWidth)).toBeLessThan(avgWidth * 0.15);
  });
});
