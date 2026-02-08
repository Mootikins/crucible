import { test, expect } from '@playwright/test';
import { findPath, findAllTabSets, drag, Location } from './helpers';

const baseURL = '/flexlayout-test.html';

// ─── Float → Main ─────────────────────────────────────────────────────

test.describe('Cross-Window DnD: Float to Main', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('dock button moves float tab back to main layout', async ({ page }) => {
    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const floatTab = floatPanel.locator('.flexlayout__tab_button--selected');
    await expect(floatTab).toContainText('Floating');

    const allTabsBefore = await page.locator('.flexlayout__tab_button').count();

    const dockButton = floatPanel.locator('[data-layout-path*="/button/dock"]');
    await dockButton.click();

    await expect(floatPanel).toHaveCount(0, { timeout: 5000 });

    const floatingInMain = page.locator('.flexlayout__tab_button_content:text("Floating")');
    await expect(floatingInMain).toBeVisible();

    const allTabsAfter = await page.locator('.flexlayout__tab_button').count();
    expect(allTabsAfter).toBe(allTabsBefore);
  });

  test('docked tab is selectable and shows content', async ({ page }) => {
    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const dockButton = floatPanel.locator('[data-layout-path*="/button/dock"]');
    await dockButton.click();
    await expect(floatPanel).toHaveCount(0, { timeout: 5000 });

    const floatingTab = page.locator('.flexlayout__tab_button_content:text("Floating")');
    await floatingTab.click();

    const tabContent = page.locator('[data-testid="panel-Floating"]');
    await expect(tabContent).toBeVisible();
    await expect(tabContent).toContainText('Floating');
  });

  test('can drag float tab into main layout tabset', async ({ page }) => {
    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const mainTabsBefore = await findPath(page, '/ts0').locator('.flexlayout__tab_button').count();

    const floatTab = floatPanel.locator('.flexlayout__tab_button').first();
    await expect(floatTab).toContainText('Floating');

    const mainTabContent = findPath(page, '/ts0/t0');
    await drag(page, floatTab, mainTabContent, Location.CENTER);

    await expect(floatPanel).toHaveCount(0, { timeout: 5000 });

    const mainTabsAfter = await findPath(page, '/ts0').locator('.flexlayout__tab_button').count();
    expect(mainTabsAfter).toBe(mainTabsBefore + 1);

    const floatingInMain = page.locator('.flexlayout__tab_button_content:text("Floating")');
    await expect(floatingInMain).toBeVisible();
  });
});

// ─── Main → Float ─────────────────────────────────────────────────────

test.describe('Cross-Window DnD: Main to Float', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('can drag tab from main layout to float panel', async ({ page }) => {
    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const floatTabsBefore = await floatPanel.locator('.flexlayout__tab_button').count();

    const mainTab = findPath(page, '/ts0/tb0');
    const floatContent = floatPanel.locator('.flexlayout__floating_panel_content');
    await drag(page, mainTab, floatContent, Location.CENTER);

    const floatTabsAfter = await floatPanel.locator('.flexlayout__tab_button').count();
    expect(floatTabsAfter).toBe(floatTabsBefore + 1);
  });
});

// ─── Roundtrip: Dock → Float → Dock ──────────────────────────────────

test.describe('Cross-Window DnD: Roundtrip', () => {
  test('float then dock via buttons preserves tab count', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_two_tabs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const totalBefore = await page.locator('.flexlayout__tab_button').count();
    expect(totalBefore).toBe(2);

    await findPath(page, '/ts0/tabstrip').click();
    const floatActiveButton = page.locator('[data-id="float-active"]');
    await floatActiveButton.click();

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const totalDuring = await page.locator('.flexlayout__tab_button').count();
    expect(totalDuring).toBe(totalBefore);

    const dockButton = floatPanel.locator('[data-layout-path*="/button/dock"]');
    await dockButton.click();

    await expect(floatPanel).toHaveCount(0, { timeout: 5000 });

    const totalAfter = await page.locator('.flexlayout__tab_button').count();
    expect(totalAfter).toBe(totalBefore);
  });

  test('dock→float→dock roundtrip preserves tab name', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const floatSelectedTab = floatPanel.locator('.flexlayout__tab_button--selected');
    await expect(floatSelectedTab).toContainText('Floating');

    const dockButton = floatPanel.locator('[data-layout-path*="/button/dock"]');
    await dockButton.click();
    await expect(floatPanel).toHaveCount(0, { timeout: 5000 });

    const floatingInMain = page.locator('.flexlayout__tab_button_content:text("Floating")');
    await expect(floatingInMain).toBeVisible();

    await floatingInMain.click();
    const floatActiveButton = page.locator('[data-id="float-active"]');
    await floatActiveButton.click();

    await expect(floatPanel).toBeVisible();
    const refloatedTab = floatPanel.locator('.flexlayout__tab_button--selected');
    await expect(refloatedTab).toContainText('Floating');
  });
});

// ─── Last Tab Drain ───────────────────────────────────────────────────

test.describe('Cross-Window DnD: Last Tab Drain', () => {
  test('docking last tab closes the float panel', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const floatTabs = floatPanel.locator('.flexlayout__tab_button');
    await expect(floatTabs).toHaveCount(1);

    const dockButton = floatPanel.locator('[data-layout-path*="/button/dock"]');
    await dockButton.click();

    await expect(floatPanel).toHaveCount(0, { timeout: 5000 });
  });

  test('closing last tab via close button removes float panel', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const floatTabs = floatPanel.locator('.flexlayout__tab_button');
    await expect(floatTabs).toHaveCount(1);

    const closeButton = floatPanel.locator('[data-layout-path*="/button/close-float"]');
    await closeButton.click();

    await expect(floatPanel).toHaveCount(0, { timeout: 5000 });
  });

  test('float panel with multiple tabs survives single tab close', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const mainTab = findPath(page, '/ts0/tb0');
    const floatContent = floatPanel.locator('.flexlayout__floating_panel_content');
    await drag(page, mainTab, floatContent, Location.CENTER);

    const floatTabs = floatPanel.locator('.flexlayout__tab_button');
    const tabCount = await floatTabs.count();

    if (tabCount > 1) {
      const closeBtn = floatPanel.locator('[data-layout-path$="/button/close"]').first();
      await closeBtn.click();

      await expect(floatPanel).toBeVisible();
      const remaining = await floatPanel.locator('.flexlayout__tab_button').count();
      expect(remaining).toBe(tabCount - 1);
    }
  });
});

// ─── Float ↔ Float ────────────────────────────────────────────────────

test.describe('Cross-Window DnD: Float to Float', () => {
  test('creating two floats then docking both preserves all tabs', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const totalBefore = await page.locator('.flexlayout__tab_button').count();

    const previewTabstrip = findPath(page, '/ts1/tabstrip');
    await previewTabstrip.click();

    const floatActiveButton = page.locator('[data-id="float-active"]');
    await floatActiveButton.click();

    const floatPanels = page.locator('.flexlayout__floating_panel');
    await expect(floatPanels).toHaveCount(2);

    const totalDuringFloat = await page.locator('.flexlayout__tab_button').count();
    expect(totalDuringFloat).toBe(totalBefore);

    const firstDock = floatPanels.first().locator('[data-layout-path*="/button/dock"]');
    await firstDock.click();
    await expect(floatPanels).toHaveCount(1, { timeout: 5000 });

    const secondDock = floatPanels.first().locator('[data-layout-path*="/button/dock"]');
    await secondDock.click();
    await expect(floatPanels).toHaveCount(0, { timeout: 5000 });

    const totalAfter = await page.locator('.flexlayout__tab_button').count();
    expect(totalAfter).toBe(totalBefore);
  });
});
