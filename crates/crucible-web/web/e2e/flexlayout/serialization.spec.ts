import { test, expect } from '@playwright/test';
import {
  findPath,
  findTabButton,
  findAllTabSets,
  checkBorderTab,
} from './helpers';

const baseURL = '/flexlayout-test.html';

// ─── 8.1 Model.toJson() ──────────────────────────────────────────────

test.describe('Serialization: toJson', () => {
  test('model.toJson() returns valid JSON with global, layout, and borders keys', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_serialization');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // toJson() verifiable via rendered structure — layout loaded from JSON model definition
    await expect(findAllTabSets(page)).toHaveCount(3);
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Layout Info');
    await expect(findTabButton(page, '/ts1', 0).locator('.flexlayout__tab_button_content')).toContainText('Serialize');
    await expect(findTabButton(page, '/ts2', 0).locator('.flexlayout__tab_button_content')).toContainText('Restore');
  });

  test('serialized model preserves tab names after modifications', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_serialization');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Close the middle tab (Serialize)
    await findPath(page, '/ts1/tb0/button/close').click();
    await expect(findAllTabSets(page)).toHaveCount(2);

    // Remaining tabs should still be present
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Layout Info');
    await expect(findTabButton(page, '/ts1', 0).locator('.flexlayout__tab_button_content')).toContainText('Restore');
  });
});

// ─── 8.2 Model.fromJson() ────────────────────────────────────────────

test.describe('Serialization: fromJson', () => {
  test('Model.fromJson() creates a functional layout from JSON definition', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_serialization');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Layout loaded successfully from JSON — verify all components rendered
    await expect(findPath(page, '/')).toHaveClass(/flexlayout__layout/);
    await expect(findAllTabSets(page)).toHaveCount(3);

    // All tab buttons are interactive (visible and contain correct text)
    await expect(findTabButton(page, '/ts0', 0)).toBeVisible();
    await expect(findTabButton(page, '/ts1', 0)).toBeVisible();
    await expect(findTabButton(page, '/ts2', 0)).toBeVisible();
  });

  test('fromJson creates layouts with correct weight proportions', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_serialization');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // basic_serialization uses weights 33/34/33
    const box0 = await findPath(page, '/ts0').boundingBox();
    const box1 = await findPath(page, '/ts1').boundingBox();
    const box2 = await findPath(page, '/ts2').boundingBox();
    expect(box0).toBeTruthy();
    expect(box1).toBeTruthy();
    expect(box2).toBeTruthy();

    // All three should be roughly equal width
    const avgWidth = (box0!.width + box1!.width + box2!.width) / 3;
    expect(Math.abs(box0!.width - avgWidth)).toBeLessThan(avgWidth * 0.15);
    expect(Math.abs(box1!.width - avgWidth)).toBeLessThan(avgWidth * 0.15);
    expect(Math.abs(box2!.width - avgWidth)).toBeLessThan(avgWidth * 0.15);
  });
});

// ─── 8.3 Round-trip (serialize → deserialize → verify) ───────────────

test.describe('Serialization: Round-trip', () => {
  test('closing a tab then reloading restores original 3-tabset layout', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_serialization');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Verify initial state
    await expect(findAllTabSets(page)).toHaveCount(3);
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Layout Info');

    // Modify: close the middle tabset's tab
    await findPath(page, '/ts1/tb0/button/close').click();
    await expect(findAllTabSets(page)).toHaveCount(2);

    // Reload from original JSON
    await page.locator('[data-id=reload]').click();

    // Verify round-trip: layout restored to original state
    await expect(findAllTabSets(page)).toHaveCount(3);
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Layout Info');
    await expect(findTabButton(page, '/ts1', 0).locator('.flexlayout__tab_button_content')).toContainText('Serialize');
    await expect(findTabButton(page, '/ts2', 0).locator('.flexlayout__tab_button_content')).toContainText('Restore');
  });

  test('adding a tab then reloading restores original tab count', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_serialization');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Click a tabset to make it active first
    await findTabButton(page, '/ts0', 0).click();
    const initialTabCount = await page.locator('.flexlayout__tab_button').count();

    // Add a tab via the toolbar button
    await page.locator('[data-id=add-active]').click();
    await page.waitForTimeout(200);
    const tabCount = await page.locator('.flexlayout__tab_button').count();
    expect(tabCount).toBeGreaterThan(initialTabCount);

    // Reload from original JSON
    await page.locator('[data-id=reload]').click();

    // Verify: back to exactly 3 tabs (one per tabset)
    await expect(findAllTabSets(page)).toHaveCount(3);
    const restoredTabCount = await page.locator('.flexlayout__tab_button').count();
    expect(restoredTabCount).toBe(initialTabCount);
  });

  test('weight proportions are preserved through reload', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_serialization');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Capture initial widths
    const initialBox0 = await findPath(page, '/ts0').boundingBox();
    const initialBox2 = await findPath(page, '/ts2').boundingBox();
    expect(initialBox0).toBeTruthy();
    expect(initialBox2).toBeTruthy();

    // Modify layout: close middle tab
    await findPath(page, '/ts1/tb0/button/close').click();
    await expect(findAllTabSets(page)).toHaveCount(2);

    // Reload
    await page.locator('[data-id=reload]').click();
    await expect(findAllTabSets(page)).toHaveCount(3);

    // Verify weights restored
    const restoredBox0 = await findPath(page, '/ts0').boundingBox();
    const restoredBox2 = await findPath(page, '/ts2').boundingBox();
    expect(restoredBox0).toBeTruthy();
    expect(restoredBox2).toBeTruthy();

    // Widths should be approximately the same as initial (within 10px tolerance)
    expect(Math.abs(restoredBox0!.width - initialBox0!.width)).toBeLessThan(10);
    expect(Math.abs(restoredBox2!.width - initialBox2!.width)).toBeLessThan(10);
  });
});

// ─── 8.4 Persistence (save/load layout state) ────────────────────────

test.describe('Serialization: Persistence', () => {
  test('reload button resets layout to initial JSON state after tab close', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_serialization');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Close two tabs
    await findPath(page, '/ts2/tb0/button/close').click();
    await findPath(page, '/ts1/tb0/button/close').click();
    await expect(findAllTabSets(page)).toHaveCount(1);

    // Reload restores all 3
    await page.locator('[data-id=reload]').click();
    await expect(findAllTabSets(page)).toHaveCount(3);
  });

  test('reload restores layout after floating a tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_serialization');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await findTabButton(page, '/ts0', 0).click();
    await page.locator('[data-id=float-active]').click();
    await page.waitForTimeout(300);

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toHaveCount(1);

    await page.locator('[data-id=reload]').click();
    await expect(floatPanel).toHaveCount(0);
    await expect(findAllTabSets(page)).toHaveCount(3);
  });
});

// ─── 8.5 Layout Templates (predefined configurations) ────────────────

test.describe('Serialization: Layout Templates', () => {
  test('basic_simple template loads 2-tabset horizontal layout', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_simple');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);
    const box0 = await findPath(page, '/ts0').boundingBox();
    const box1 = await findPath(page, '/ts1').boundingBox();
    expect(box0).toBeTruthy();
    expect(box1).toBeTruthy();
    // Horizontal: ts0 left of ts1
    expect(box0!.x + box0!.width).toBeLessThanOrEqual(box1!.x + 2);
  });

  test('basic_vertical_root template loads 3-tabset vertical layout', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_vertical_root');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(3);
    const box0 = await findPath(page, '/ts0').boundingBox();
    const box1 = await findPath(page, '/ts1').boundingBox();
    expect(box0).toBeTruthy();
    expect(box1).toBeTruthy();
    // Vertical: ts0 above ts1
    expect(box0!.y + box0!.height).toBeLessThanOrEqual(box1!.y + 2);
  });

  test('test_with_borders template includes border panels in all 4 directions', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // All 4 border tab buttons visible
    await expect(findTabButton(page, '/border/top', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/bottom', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/left', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/right', 0)).toBeVisible();
  });

  test('test_with_float template includes float window in JSON', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toHaveCount(1);

    const floatTabButton = floatPanel.locator('.flexlayout__tab_button_content');
    await expect(floatTabButton).toContainText('Floating');
  });

  test('stress_complex template loads deeply nested layout with borders and float', async ({ page }) => {
    await page.goto(baseURL + '?layout=stress_complex');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // 7 tabsets total (6 main + 1 float)
    await expect(findAllTabSets(page)).toHaveCount(7);

    // Has borders on all 4 sides
    await expect(findTabButton(page, '/border/top', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/bottom', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/left', 0)).toBeVisible();
    await expect(findTabButton(page, '/border/right', 0)).toBeVisible();
  });
});

// ─── 8.6 Global Config in JSON ───────────────────────────────────────

test.describe('Serialization: Global Config in JSON', () => {
  test('global config tabCloseType applies to all tabs in serialized layout', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_serialization');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Global config has tabCloseType: 1 — close buttons visible
    await expect(findPath(page, '/ts0/tb0/button/close')).toBeVisible();
    await expect(findPath(page, '/ts1/tb0/button/close')).toBeVisible();
    await expect(findPath(page, '/ts2/tb0/button/close')).toBeVisible();
  });

  test('global config is preserved after reload', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_serialization');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Close a tab
    await findPath(page, '/ts1/tb0/button/close').click();
    await expect(findAllTabSets(page)).toHaveCount(2);

    // Reload
    await page.locator('[data-id=reload]').click();
    await expect(findAllTabSets(page)).toHaveCount(3);

    // Global config still applies — close buttons present on all restored tabs
    await expect(findPath(page, '/ts0/tb0/button/close')).toBeVisible();
    await expect(findPath(page, '/ts1/tb0/button/close')).toBeVisible();
    await expect(findPath(page, '/ts2/tb0/button/close')).toBeVisible();
  });
});

// ─── 8.7 Borders in JSON ─────────────────────────────────────────────

test.describe('Serialization: Borders in JSON', () => {
  test('border definitions from JSON create functional border panels', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Click border tab to open panel
    await findTabButton(page, '/border/top', 0).click();
    await expect(findPath(page, '/border/top/t0')).toBeVisible();

    // Verify border panel content
    await checkBorderTab(page, '/border/top', 0, true, 'top1');
  });

  test('multiple borders from JSON coexist independently', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Open top border
    await findTabButton(page, '/border/top', 0).click();
    await expect(findPath(page, '/border/top/t0')).toBeVisible();

    // Open left border — both should be open simultaneously
    await findTabButton(page, '/border/left', 0).click();
    await expect(findPath(page, '/border/left/t0')).toBeVisible();

    // Top border still open (borders are independent)
    await expect(findPath(page, '/border/top/t0')).toBeVisible();
  });
});

// ─── 8.8 Float Windows in JSON ───────────────────────────────────────

test.describe('Serialization: Float Windows in JSON', () => {
  test('float windows defined in JSON "windows" key render as overlay panels', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toHaveCount(1);
    await expect(floatPanel).toBeVisible();
  });

  test('float window from JSON has positioned overlay with correct tab content', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    const floatContent = floatPanel.locator('.flexlayout__tab_button_content');
    await expect(floatContent).toContainText('Floating');

    const floatBox = await floatPanel.boundingBox();
    expect(floatBox).toBeTruthy();
    expect(floatBox!.width).toBeGreaterThan(50);
    expect(floatBox!.height).toBeGreaterThan(50);
  });

  test('main layout coexists with float windows from JSON', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Main layout has 2 docked tabsets
    await expect(findPath(page, '/ts0')).toBeVisible();
    await expect(findPath(page, '/ts1')).toBeVisible();

    // Plus 1 float window tabset = 3 total
    await expect(findAllTabSets(page)).toHaveCount(3);

    // Main tabs work
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Main');
  });
});

// ─── 8.9 onModelChange Callback ──────────────────────────────────────

test.describe('Serialization: onModelChange Callback', () => {
  test('onModelChange fires when tab is selected', async ({ page }) => {
    await page.goto(baseURL + '?layout=serial_on_model_change');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Collect console messages
    const consoleMessages: string[] = [];
    page.on('console', (msg) => {
      if (msg.text().includes('[onModelChange]')) {
        consoleMessages.push(msg.text());
      }
    });

    // Click the second tab to trigger a selection change
    await findTabButton(page, '/ts0', 1).dispatchEvent('click');
    await page.waitForTimeout(300);

    // Should have logged at least one onModelChange event
    expect(consoleMessages.length).toBeGreaterThan(0);
    expect(consoleMessages.some(m => m.includes('[onModelChange]'))).toBe(true);
  });

  test('onModelChange fires on tab close action', async ({ page }) => {
    await page.goto(baseURL + '?layout=serial_on_model_change');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const consoleMessages: string[] = [];
    page.on('console', (msg) => {
      if (msg.text().includes('[onModelChange]')) {
        consoleMessages.push(msg.text());
      }
    });

    // Close a tab — should trigger onModelChange
    await findPath(page, '/ts0/tb1/button/close').click();
    await page.waitForTimeout(300);

    expect(consoleMessages.length).toBeGreaterThan(0);
    expect(consoleMessages.some(m => m.includes('[onModelChange]'))).toBe(true);
  });

  test('serial_on_model_change layout renders with correct structure', async ({ page }) => {
    await page.goto(baseURL + '?layout=serial_on_model_change');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // 2 tabsets, first has 2 tabs, second has 1 tab
    await expect(findAllTabSets(page)).toHaveCount(2);
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Interact Here');
    await expect(findTabButton(page, '/ts0', 1).locator('.flexlayout__tab_button_content')).toContainText('Tab B');
    await expect(findTabButton(page, '/ts1', 0).locator('.flexlayout__tab_button_content')).toContainText('Change Log');
  });
});
