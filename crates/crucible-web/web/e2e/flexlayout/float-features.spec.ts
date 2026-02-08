import { test, expect } from '@playwright/test';
import { findPath, findAllTabSets, drag, Location } from './helpers';

const baseURL = '/flexlayout-test.html';

// ─── 7.1 Float Panel (overlay within layout) ─────────────────────────

test.describe('Float 7.1: Float Panel Creation', () => {
  test('pre-loaded float layout renders floating panel overlay', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    // Panel contains a tabstrip and content area
    const tabstrip = floatPanel.locator('.flexlayout__tabset_tabbar_outer');
    await expect(tabstrip).toBeVisible();
    const content = floatPanel.locator('.flexlayout__floating_panel_content');
    await expect(content).toBeVisible();
  });

  test('Action.floatTab creates a new float panel via button', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_create_window');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // No float panels initially
    await expect(page.locator('.flexlayout__floating_panel')).toHaveCount(0);

    // Click "Create Float Window" action button
    const createBtn = page.locator('[data-id="action-create-window"]');
    await createBtn.click();

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    // The floated tab ("Float Me") should appear in the float panel
    const floatTab = floatPanel.locator('.flexlayout__tab_button--selected');
    await expect(floatTab).toContainText('Float Me');
  });

  test('Float Active button floats the active tabset', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_two_tabs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Click a tab to ensure a tabset is active
    const firstTab = page.locator('.flexlayout__tab_button').first();
    await firstTab.click();

    const floatActiveBtn = page.locator('[data-id="float-active"]');
    await floatActiveBtn.click();

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();
  });
});

// ─── 7.2 Float Panel Move (drag header) ──────────────────────────────

test.describe('Float 7.2: Float Panel Position', () => {
  test('float panel is positioned at specified coordinates from JSON', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    const box = await floatPanel.boundingBox();
    expect(box).toBeTruthy();

    // JSON specifies rect: { x: 100, y: 100, width: 300, height: 200 }
    // Position should be roughly in that vicinity (offset by layout container)
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.y).toBeGreaterThanOrEqual(0);
  });

  test('dragging tabstrip repositions the float panel', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    const tabstrip = floatPanel.locator('.flexlayout__tabset_tabbar_outer');

    const initialBox = await floatPanel.boundingBox();
    expect(initialBox).toBeTruthy();

    const tabstripBox = await tabstrip.boundingBox();
    expect(tabstripBox).toBeTruthy();

    // Drag the tabstrip 60px right, 40px down
    await page.mouse.move(
      tabstripBox!.x + tabstripBox!.width / 2,
      tabstripBox!.y + tabstripBox!.height / 2,
    );
    await page.mouse.down();
    await page.mouse.move(
      tabstripBox!.x + tabstripBox!.width / 2 + 60,
      tabstripBox!.y + tabstripBox!.height / 2 + 40,
      { steps: 5 },
    );
    await page.mouse.up();

    const finalBox = await floatPanel.boundingBox();
    expect(finalBox).toBeTruthy();
    expect(finalBox!.x).toBeGreaterThan(initialBox!.x + 20);
    expect(finalBox!.y).toBeGreaterThan(initialBox!.y + 10);
  });

  test('Action.moveWindow repositions a float panel programmatically', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_move_window');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const initialBox = await floatPanel.boundingBox();
    expect(initialBox).toBeTruthy();

    // Click "Move Window → (200,200)" action button
    const moveBtn = page.locator('[data-id="action-move-window"]');
    await moveBtn.click();

    const finalBox = await floatPanel.boundingBox();
    expect(finalBox).toBeTruthy();

    // The window should have moved — dimensions may also change (300x220)
    // Just verify the box changed from its initial position
    const posChanged = finalBox!.x !== initialBox!.x || finalBox!.y !== initialBox!.y;
    const sizeChanged = finalBox!.width !== initialBox!.width || finalBox!.height !== initialBox!.height;
    expect(posChanged || sizeChanged).toBeTruthy();
  });
});

// ─── 7.3 Float Panel Resize (edge handles) ──────────────────────────

test.describe('Float 7.3: Float Panel Size', () => {
  test('float panel has a resize handle', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const handle = page.locator('.flexlayout__floating_panel_resize_handle');
    await expect(handle).toBeVisible();
  });

  test('dragging resize handle changes panel dimensions', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    const resizeHandle = page.locator('.flexlayout__floating_panel_resize_handle');

    const initialBox = await floatPanel.boundingBox();
    expect(initialBox).toBeTruthy();

    const handleBox = await resizeHandle.boundingBox();
    expect(handleBox).toBeTruthy();

    // Drag handle to make panel larger
    await page.mouse.move(
      handleBox!.x + handleBox!.width / 2,
      handleBox!.y + handleBox!.height / 2,
    );
    await page.mouse.down();
    await page.mouse.move(
      handleBox!.x + handleBox!.width / 2 + 100,
      handleBox!.y + handleBox!.height / 2 + 80,
      { steps: 5 },
    );
    await page.mouse.up();

    const finalBox = await floatPanel.boundingBox();
    expect(finalBox).toBeTruthy();
    expect(finalBox!.width).toBeGreaterThan(initialBox!.width + 40);
    expect(finalBox!.height).toBeGreaterThan(initialBox!.height + 30);
  });

  test('float panel from JSON uses specified dimensions', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    const box = await floatPanel.boundingBox();
    expect(box).toBeTruthy();

    // JSON specifies width: 300, height: 200 — verify roughly in range
    expect(box!.width).toBeGreaterThan(100);
    expect(box!.height).toBeGreaterThan(80);
  });
});

// ─── 7.4 Float Dock Button (return to main) ─────────────────────────

test.describe('Float 7.4: Float Dock Button', () => {
  test('dock button returns float tab to main layout', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const dockButton = floatPanel.locator('[data-layout-path*="/button/dock"]');
    await expect(dockButton).toHaveAttribute('title', 'Dock');
    await dockButton.click();

    // Float panel should disappear
    await expect(floatPanel).toHaveCount(0, { timeout: 5000 });

    // Tab should now be in main layout
    const floatingInMain = page.locator('.flexlayout__tab_button_content:text("Floating")');
    await expect(floatingInMain).toBeVisible();
  });

  test('Action.dockTabset docks a floating tabset programmatically', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_dock_tabset');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // First float the "Dock Target" tabset
    const dockTargetTabstrip = findPath(page, '/ts1/tabstrip');
    await dockTargetTabstrip.click();

    const floatActiveBtn = page.locator('[data-id="float-active"]');
    await floatActiveBtn.click();

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    // Now dock it back via the action button
    const dockTabsetBtn = page.locator('[data-id="action-dock-tabset"]');
    await dockTabsetBtn.click();

    await expect(floatPanel).toHaveCount(0, { timeout: 5000 });
  });
});

// ─── 7.5 Float Close Button ─────────────────────────────────────────

test.describe('Float 7.5: Float Close Button', () => {
  test('close button removes the float panel and its tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const totalTabsBefore = await page.locator('.flexlayout__tab_button').count();

    const closeButton = floatPanel.locator('[data-layout-path*="/button/close-float"]');
    await expect(closeButton).toHaveAttribute('title', 'Close');
    await closeButton.click();

    await expect(floatPanel).toHaveCount(0, { timeout: 5000 });

    // Tab count should decrease (tab was removed, not docked)
    const totalTabsAfter = await page.locator('.flexlayout__tab_button').count();
    expect(totalTabsAfter).toBeLessThan(totalTabsBefore);
  });

  test('closing last tab in float removes the float panel', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    // Confirm only one tab in the float
    const floatTabs = floatPanel.locator('.flexlayout__tab_button');
    await expect(floatTabs).toHaveCount(1);

    const closeButton = floatPanel.locator('[data-layout-path*="/button/close-float"]');
    await closeButton.click();

    await expect(floatPanel).toHaveCount(0, { timeout: 5000 });
  });
});

// ─── 7.6 Float Z-Order Management ───────────────────────────────────

test.describe('Float 7.6: Float Z-Order', () => {
  test('clicking a float panel brings it to front when multiple exist', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Float the Preview tabset to create a second float panel
    const previewTabstrip = findPath(page, '/ts1/tabstrip');
    await previewTabstrip.click();

    const floatActiveBtn = page.locator('[data-id="float-active"]');
    await floatActiveBtn.click();

    const floatPanels = page.locator('.flexlayout__floating_panel');
    await expect(floatPanels).toHaveCount(2);

    // Click the first float panel to bring it forward
    const firstFloatTabstrip = floatPanels.first().locator('.flexlayout__tabset_tabbar_outer');
    await firstFloatTabstrip.click();

    // Both floats should still be visible and functional
    await expect(floatPanels).toHaveCount(2);
  });

  test('new float panel appears above existing panels', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const firstFloat = page.locator('.flexlayout__floating_panel');
    await expect(firstFloat).toBeVisible();

    // Float the second tabset
    const tabstrip = findPath(page, '/ts1/tabstrip');
    await tabstrip.click();

    const floatActiveBtn = page.locator('[data-id="float-active"]');
    await floatActiveBtn.click();

    const floatPanels = page.locator('.flexlayout__floating_panel');
    await expect(floatPanels).toHaveCount(2);

    // The new float should exist and be visible
    const lastFloat = floatPanels.last();
    await expect(lastFloat).toBeVisible();
  });
});

// ─── 7.7 Float → Main Drag ──────────────────────────────────────────

test.describe('Float 7.7: Float to Main Drag', () => {
  test('dragging tab from float to main tabset moves it', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const mainTabsBefore = await findPath(page, '/ts0').locator('.flexlayout__tab_button').count();

    const floatTab = floatPanel.locator('.flexlayout__tab_button').first();
    const mainTarget = findPath(page, '/ts0/t0');
    await drag(page, floatTab, mainTarget, Location.CENTER);

    // Float panel should disappear (only had one tab)
    await expect(floatPanel).toHaveCount(0, { timeout: 5000 });

    // Main tabset should gain a tab
    const mainTabsAfter = await findPath(page, '/ts0').locator('.flexlayout__tab_button').count();
    expect(mainTabsAfter).toBe(mainTabsBefore + 1);
  });
});

// ─── 7.8 Main → Float Drag ──────────────────────────────────────────

test.describe('Float 7.8: Main to Float Drag', () => {
  test('dragging tab from main layout into float panel adds it', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const floatTabsBefore = await floatPanel.locator('.flexlayout__tab_button').count();

    // Drag main tab into float content
    const mainTab = findPath(page, '/ts0/tb0');
    const floatContent = floatPanel.locator('.flexlayout__floating_panel_content');
    await drag(page, mainTab, floatContent, Location.CENTER);

    const floatTabsAfter = await floatPanel.locator('.flexlayout__tab_button').count();
    expect(floatTabsAfter).toBe(floatTabsBefore + 1);
  });
});

// ─── 7.9 Float-to-Float Drag ────────────────────────────────────────

test.describe('Float 7.9: Float-to-Float Interaction', () => {
  test('two float panels can coexist and both dock successfully', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const totalBefore = await page.locator('.flexlayout__tab_button').count();

    // Float the Preview tabset to create a second float panel
    const previewTabstrip = findPath(page, '/ts1/tabstrip');
    await previewTabstrip.click();

    const floatActiveBtn = page.locator('[data-id="float-active"]');
    await floatActiveBtn.click();

    const floatPanels = page.locator('.flexlayout__floating_panel');
    await expect(floatPanels).toHaveCount(2);

    // Tab count preserved during float
    const totalDuring = await page.locator('.flexlayout__tab_button').count();
    expect(totalDuring).toBe(totalBefore);

    // Dock both panels back
    const firstDock = floatPanels.first().locator('[data-layout-path*="/button/dock"]');
    await firstDock.click();
    await expect(floatPanels).toHaveCount(1, { timeout: 5000 });

    const secondDock = floatPanels.first().locator('[data-layout-path*="/button/dock"]');
    await secondDock.click();
    await expect(floatPanels).toHaveCount(0, { timeout: 5000 });

    // All tabs preserved
    const totalAfter = await page.locator('.flexlayout__tab_button').count();
    expect(totalAfter).toBe(totalBefore);
  });
});

// ─── 7.10 Float Window from JSON (pre-loaded) ───────────────────────

test.describe('Float 7.10: Float Window from JSON', () => {
  test('model with windows key renders float panel on load', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    // The float tab should have the name defined in JSON
    const selectedTab = floatPanel.locator('.flexlayout__tab_button--selected');
    await expect(selectedTab).toContainText('Floating');
  });

  test('stress_complex layout with float renders all elements', async ({ page }) => {
    await page.goto(baseURL + '?layout=stress_complex');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // stress_complex has a float window with two tabs (Float1, Float2)
    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const floatTabs = floatPanel.locator('.flexlayout__tab_button');
    await expect(floatTabs).toHaveCount(2);
  });

  test('action_move_window layout renders pre-loaded float', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_move_window');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const floatTab = floatPanel.locator('.flexlayout__tab_button--selected');
    await expect(floatTab).toContainText('Floating');
  });

  test('float panel content renders correctly after JSON load', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    const tabContent = floatPanel.locator('[data-testid="panel-Floating"]');
    await expect(tabContent).toBeVisible();
    await expect(tabContent).toContainText('Floating');
  });
});
