import { test, expect } from '@playwright/test';
import { findPath, findAllTabSets } from './helpers';

const baseURL = '/flexlayout-test.html';

test.describe('Float Windows: Pre-loaded Float Layout', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('renders main layout with tabsets plus float tabset', async ({ page }) => {
    await expect(findAllTabSets(page)).toHaveCount(3);
  });

  test('renders floating panel overlay', async ({ page }) => {
    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();
  });

  test('floating panel has tabstrip with selected tab', async ({ page }) => {
    const floatPanel = page.locator('.flexlayout__floating_panel');
    const tabstrip = floatPanel.locator('.flexlayout__tabset_tabbar_outer');
    await expect(tabstrip).toBeVisible();
    const selectedTab = floatPanel.locator('.flexlayout__tab_button--selected');
    await expect(selectedTab).toContainText('Floating');
  });

  test('floating panel has dock and close buttons in tabset toolbar', async ({ page }) => {
    const floatPanel = page.locator('.flexlayout__floating_panel');
    const dockButton = floatPanel.locator('[data-layout-path*="/button/dock"]');
    const closeButton = floatPanel.locator('[data-layout-path*="/button/close-float"]');
    await expect(dockButton).toBeVisible();
    await expect(dockButton).toHaveAttribute('title', 'Dock');
    await expect(closeButton).toBeVisible();
    await expect(closeButton).toHaveAttribute('title', 'Close');
  });

  test('floating panel has resize handle', async ({ page }) => {
    const handle = page.locator('.flexlayout__floating_panel_resize_handle');
    await expect(handle).toBeVisible();
  });

  test('floating panel renders tab content', async ({ page }) => {
    const floatPanel = page.locator('.flexlayout__floating_panel');
    const tabContent = floatPanel.locator('[data-testid="panel-Floating"]');
    await expect(tabContent).toBeVisible();
    await expect(tabContent).toContainText('Floating');
  });
});

test.describe('Float Windows: Float via Button', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=test_two_tabs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    await expect(findAllTabSets(page)).toHaveCount(2);
  });

  test('float button exists in tabset toolbar', async ({ page }) => {
    const floatButton = page.locator('[data-layout-path*="/button/float"]').first();
    await expect(floatButton).toBeVisible();
    await expect(floatButton).toHaveAttribute('title', 'Float');
  });

  test('clicking float button creates floating panel', async ({ page }) => {
    const floatButton = page.locator('[data-layout-path*="/button/float"]').first();
    await floatButton.click();

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();
  });

  test('floating a tabset removes it from main layout', async ({ page }) => {
    await expect(findAllTabSets(page)).toHaveCount(2);

    const floatButton = page.locator('[data-layout-path*="/button/float"]').first();
    await floatButton.click();

    const mainTabsets = page.locator('.flexlayout__layout_main .flexlayout__tabset, .flexlayout__layout_border_container .flexlayout__tabset');
    const mainCount = await mainTabsets.count();
    expect(mainCount).toBeLessThanOrEqual(1);
  });

  test('Float Active button creates floating panel', async ({ page }) => {
    const firstTabButton = page.locator('.flexlayout__tab_button').first();
    await firstTabButton.click();

    const floatActiveButton = page.locator('[data-id="float-active"]');
    await floatActiveButton.click();

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();
  });
});

test.describe('Float Windows: Dock Back', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('dock button returns float to main layout', async ({ page }) => {
    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const dockButton = floatPanel.locator('[data-layout-path*="/button/dock"]');
    await dockButton.click();

    await expect(floatPanel).not.toBeVisible();
  });

  test('close button removes float window', async ({ page }) => {
    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const closeButton = floatPanel.locator('[data-layout-path*="/button/close-float"]');
    await closeButton.click();

    await expect(floatPanel).not.toBeVisible();
  });
});

test.describe('Float Windows: Drag and Resize', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('dragging tabstrip moves the floating panel', async ({ page }) => {
    const floatPanel = page.locator('.flexlayout__floating_panel');
    const tabstrip = floatPanel.locator('.flexlayout__tabset_tabbar_outer');

    const initialBox = await floatPanel.boundingBox();
    expect(initialBox).toBeTruthy();

    const tabstripBox = await tabstrip.boundingBox();
    expect(tabstripBox).toBeTruthy();

    await page.mouse.move(
      tabstripBox!.x + tabstripBox!.width / 2,
      tabstripBox!.y + tabstripBox!.height / 2,
    );
    await page.mouse.down();
    await page.mouse.move(
      tabstripBox!.x + tabstripBox!.width / 2 + 50,
      tabstripBox!.y + tabstripBox!.height / 2 + 30,
      { steps: 5 },
    );
    await page.mouse.up();

    const finalBox = await floatPanel.boundingBox();
    expect(finalBox).toBeTruthy();
    expect(finalBox!.x).toBeGreaterThan(initialBox!.x + 20);
    expect(finalBox!.y).toBeGreaterThan(initialBox!.y + 10);
  });

  test('dragging resize handle changes panel size', async ({ page }) => {
    const resizeHandle = page.locator('.flexlayout__floating_panel_resize_handle');
    const floatPanel = page.locator('.flexlayout__floating_panel');

    const initialBox = await floatPanel.boundingBox();
    expect(initialBox).toBeTruthy();

    const handleBox = await resizeHandle.boundingBox();
    expect(handleBox).toBeTruthy();

    await page.mouse.move(
      handleBox!.x + handleBox!.width / 2,
      handleBox!.y + handleBox!.height / 2,
    );
    await page.mouse.down();
    await page.mouse.move(
      handleBox!.x + handleBox!.width / 2 + 80,
      handleBox!.y + handleBox!.height / 2 + 60,
      { steps: 5 },
    );
    await page.mouse.up();

    const finalBox = await floatPanel.boundingBox();
    expect(finalBox).toBeTruthy();
    expect(finalBox!.width).toBeGreaterThan(initialBox!.width + 40);
    expect(finalBox!.height).toBeGreaterThan(initialBox!.height + 20);
  });
});
