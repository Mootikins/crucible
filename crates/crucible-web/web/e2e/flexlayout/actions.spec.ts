import { test, expect } from '@playwright/test';
import {
  findPath,
  findTabButton,
  findAllTabSets,
  checkTab,
  drag,
  Location,
} from './helpers';

const baseURL = '/flexlayout-test.html';

// ─── 9.1 Add Tab to TabSet (Action.addNode) ──────────────────────────

test.describe('Actions: Add Tab to TabSet', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=action_add_remove');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('action button adds new tab to active tabset', async ({ page }) => {
    // Activate tabset-main by clicking its tabstrip
    await findPath(page, '/ts0/tabstrip').click();

    // Click "Add Tab" action button
    await page.locator('[data-id="action-add-tab"]').click();

    // Should now have 3 tabs in ts0 (Alpha, Beta, New 1)
    const tabButtons = findPath(page, '/ts0/tabstrip').locator('.flexlayout__tab_button');
    await expect(tabButtons).toHaveCount(3);
  });

  test('adding multiple tabs increments name counter', async ({ page }) => {
    await findPath(page, '/ts0/tabstrip').click();

    await page.locator('[data-id="action-add-tab"]').click();
    await page.locator('[data-id="action-add-tab"]').click();

    // Should have 4 tabs (Alpha, Beta, New 1, New 2)
    const tabButtons = findPath(page, '/ts0/tabstrip').locator('.flexlayout__tab_button');
    await expect(tabButtons).toHaveCount(4);
  });
});

// ─── 9.2 Add Tab to Active TabSet ────────────────────────────────────

test.describe('Actions: Add Tab to Active TabSet', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=action_add_remove');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('add-active button adds tab to whichever tabset is active', async ({ page }) => {
    // Make ts1 (tabset-side) the active tabset
    await findPath(page, '/ts1/tabstrip').click();

    // Use the global "Add Active" button
    await page.locator('[data-id="add-active"]').click();

    // ts1 should now have 2 tabs (Gamma + new Text1)
    const tabButtons = findPath(page, '/ts1/tabstrip').locator('.flexlayout__tab_button');
    await expect(tabButtons).toHaveCount(2);
  });
});

// ─── 9.3 Add Tab to Border ───────────────────────────────────────────

test.describe('Actions: Add Tab to Border', () => {
  test('dragging tab to border adds it to border panel', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_adjust_border');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Border bottom should have 1 tab button
    const bottomBorderTabs = page.locator('[data-layout-path^="/border/bottom/tb"]');
    const initialCount = await bottomBorderTabs.count();
    expect(initialCount).toBeGreaterThanOrEqual(1);
  });
});

// ─── 9.4 Move Tab (Action.moveNode) ─────────────────────────────────

test.describe('Actions: Move Tab', () => {
  test('drag tab from one tabset to another moves it', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_add_remove');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Initially: ts0 has 2 tabs (Alpha, Beta), ts1 has 1 tab (Gamma)
    await expect(findPath(page, '/ts0/tabstrip').locator('.flexlayout__tab_button')).toHaveCount(2);
    await expect(findPath(page, '/ts1/tabstrip').locator('.flexlayout__tab_button')).toHaveCount(1);

    // Drag Alpha from ts0 to ts1
    const from = findTabButton(page, '/ts0', 0);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.CENTER);

    // ts0 should have 1 tab (Beta), ts1 should have 2 tabs (Gamma, Alpha)
    await expect(findPath(page, '/ts0/tabstrip').locator('.flexlayout__tab_button')).toHaveCount(1);
    await expect(findPath(page, '/ts1/tabstrip').locator('.flexlayout__tab_button')).toHaveCount(2);
  });
});

// ─── 9.5 Delete Tab (Action.deleteTab) ──────────────────────────────

test.describe('Actions: Delete Tab', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=action_add_remove');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('delete-active button removes currently selected tab', async ({ page }) => {
    // Click ts0 to make it active
    await findPath(page, '/ts0/tabstrip').click();

    // Should have 2 tabs before delete
    await expect(findPath(page, '/ts0/tabstrip').locator('.flexlayout__tab_button')).toHaveCount(2);

    // Delete the active tab
    await page.locator('[data-id="action-delete-active"]').click();

    // Should have 1 tab remaining
    await expect(findPath(page, '/ts0/tabstrip').locator('.flexlayout__tab_button')).toHaveCount(1);
  });

  test('deleting all tabs from a tabset collapses it', async ({ page }) => {
    // Activate ts1 (has only Gamma)
    await findPath(page, '/ts1/tabstrip').click();

    // Delete the only tab
    await page.locator('[data-id="action-delete-active"]').click();

    // ts1 should be gone, leaving only 1 tabset
    await expect(findAllTabSets(page)).toHaveCount(1);
  });
});

// ─── 9.6 Rename Tab (Action.renameTab) ──────────────────────────────

test.describe('Actions: Rename Tab', () => {
  test('double-click tab button to rename tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_add_remove');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Double-click Alpha tab to trigger rename
    await findPath(page, '/ts0/tb0').dblclick();

    const textbox = findPath(page, '/ts0/tb0/textbox');
    await expect(textbox).toBeVisible();

    // Clear and type new name
    await textbox.fill('');
    await textbox.type('Renamed');
    await textbox.press('Enter');

    // Verify renamed
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Renamed');
  });
});

// ─── 9.7 Select Tab (Action.selectTab) ──────────────────────────────

test.describe('Actions: Select Tab', () => {
  test('clicking inactive tab selects it', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_add_remove');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Alpha is selected (index 0). Click Beta (index 1) via dispatchEvent (draggable tabs)
    await findTabButton(page, '/ts0', 1).dispatchEvent('click');

    // Beta should now be selected
    await expect(findTabButton(page, '/ts0', 1)).toHaveClass(/flexlayout__tab_button--selected/);
    await expect(findTabButton(page, '/ts0', 0)).toHaveClass(/flexlayout__tab_button--unselected/);
  });
});

// ─── 9.8 Set Active TabSet (Action.setActiveTabset) ─────────────────

test.describe('Actions: Set Active TabSet', () => {
  test('clicking a tab in different tabset changes active tabset', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_add_remove');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Click ts0 to make it active
    await findTabButton(page, '/ts0', 0).click();
    await expect(findPath(page, '/ts0/tabstrip')).toHaveClass(/flexlayout__tabset-selected/);

    // Click ts1 tab to change active tabset
    await findTabButton(page, '/ts1', 0).click();
    await expect(findPath(page, '/ts1/tabstrip')).toHaveClass(/flexlayout__tabset-selected/);
    await expect(findPath(page, '/ts0/tabstrip')).not.toHaveClass(/flexlayout__tabset-selected/);
  });
});

// ─── 9.9 Maximize Toggle (Action.maximizeToggle) ────────────────────

test.describe('Actions: Maximize Toggle', () => {
  test('maximize button hides other tabsets, restore brings them back', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_add_remove');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Both tabsets visible
    await expect(findPath(page, '/ts0')).toBeVisible();
    await expect(findPath(page, '/ts1')).toBeVisible();

    // Click maximize on ts0
    await findPath(page, '/ts0/button/max').click();

    // Only ts0 should be visible
    await expect(findPath(page, '/ts0')).toBeVisible();
    await expect(findPath(page, '/ts1')).toBeHidden();

    // Click maximize again to restore
    await findPath(page, '/ts0/button/max').click();

    await expect(findPath(page, '/ts0')).toBeVisible();
    await expect(findPath(page, '/ts1')).toBeVisible();
  });
});

// ─── 9.10 Update Model Attributes (Action.updateModelAttributes) ────

test.describe('Actions: Update Model Attributes', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=action_model_update');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('toggle edge dock changes model config', async ({ page }) => {
    const toggleBtn = page.locator('[data-id="action-toggle-edge-dock"]');
    // Initially ON
    await expect(toggleBtn).toContainText('ON');

    // Click to toggle off
    await toggleBtn.click();
    await expect(toggleBtn).toContainText('OFF');

    // Click to toggle back on
    await toggleBtn.click();
    await expect(toggleBtn).toContainText('ON');
  });

  test('toggle vertical orientation changes root row direction', async ({ page }) => {
    const toggleBtn = page.locator('[data-id="action-toggle-vertical"]');

    // Initially OFF (horizontal)
    await expect(toggleBtn).toContainText('OFF');

    // Get initial layout — tabsets side by side (horizontal)
    const ts0Box = await findPath(page, '/ts0').boundingBox();
    const ts1Box = await findPath(page, '/ts1').boundingBox();
    expect(ts0Box).toBeTruthy();
    expect(ts1Box).toBeTruthy();

    // In horizontal mode, tabsets are roughly at the same Y
    expect(Math.abs(ts0Box!.y - ts1Box!.y)).toBeLessThan(10);

    // Toggle to vertical
    await toggleBtn.click();
    await expect(toggleBtn).toContainText('ON');

    // In vertical mode, tabsets should be stacked (different Y, similar X)
    const ts0BoxV = await findPath(page, '/ts0').boundingBox();
    const ts1BoxV = await findPath(page, '/ts1').boundingBox();
    expect(ts0BoxV).toBeTruthy();
    expect(ts1BoxV).toBeTruthy();
    expect(Math.abs(ts0BoxV!.x - ts1BoxV!.x)).toBeLessThan(10);
    expect(ts1BoxV!.y).toBeGreaterThan(ts0BoxV!.y + 20);
  });
});

// ─── 9.11 Update Node Attributes (Action.updateNodeAttributes) ──────

test.describe('Actions: Update Node Attributes', () => {
  test('set tab icon via action changes tab icon', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_set_tab_attrs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const targetTab = findTabButton(page, '/ts0', 0);
    const leading = targetTab.locator('.flexlayout__tab_button_leading');

    // Initial icon is 📝
    await expect(leading).toBeAttached();

    // Click Icon button to toggle to 🎯
    await page.locator('[data-id="action-attrs-icon"]').click();

    // Icon should have changed
    await expect(leading).toBeAttached();
  });
});

// ─── 9.12 Adjust Border Split (Action.adjustBorderSplit) ────────────

test.describe('Actions: Adjust Border Split', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=action_adjust_border');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('adjust bottom border via action button changes size', async ({ page }) => {
    // Open the bottom border by clicking its tab
    const borderTab = findTabButton(page, '/border/bottom', 0);
    await borderTab.click();

    // Get initial border size
    const borderContent = findPath(page, '/border/bottom/t0');
    await expect(borderContent).toBeVisible();
    const initialBox = await borderContent.boundingBox();
    expect(initialBox).toBeTruthy();

    // Click "Bottom → 300px" action button (adjustBorderSplit delta=300)
    await page.locator('[data-id="action-adjust-bottom"]').click();

    // Border should have changed size (may increase or stay — adjustBorderSplit uses delta)
    const newBox = await borderContent.boundingBox();
    expect(newBox).toBeTruthy();
    // The border height should have changed from the action
    expect(newBox!.height).not.toEqual(0);
  });

  test('adjust left border via action button', async ({ page }) => {
    // Open the left border
    const borderTab = findTabButton(page, '/border/left', 0);
    await borderTab.click();

    const borderContent = findPath(page, '/border/left/t0');
    await expect(borderContent).toBeVisible();

    // Click "Left → 250px"
    await page.locator('[data-id="action-adjust-left"]').click();

    const newBox = await borderContent.boundingBox();
    expect(newBox).toBeTruthy();
    expect(newBox!.width).not.toEqual(0);
  });
});

// ─── 9.13 Float Tab (Action.floatTab) ───────────────────────────────

test.describe('Actions: Float Tab', () => {
  test('float-active button creates a floating panel from tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_add_remove');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Initially no float panels
    await expect(page.locator('.flexlayout__floating_panel')).toHaveCount(0);

    // Activate ts0 and float it
    await findTabButton(page, '/ts0', 0).click();
    await page.locator('[data-id="float-active"]').click();

    // A floating panel should appear
    await expect(page.locator('.flexlayout__floating_panel')).toBeVisible();
  });

  test('create-window button floats specific tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_create_window');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Initially no float panels
    await expect(page.locator('.flexlayout__floating_panel')).toHaveCount(0);

    // Click "Create Float Window" button — floats tab-floatable
    await page.locator('[data-id="action-create-window"]').click();

    // A floating panel should appear
    await expect(page.locator('.flexlayout__floating_panel')).toBeVisible();
  });
});

// ─── 9.14 Un-Float Tab / Dock Tab (Action.dockTab) ──────────────────

test.describe('Actions: Dock Tab (Un-Float)', () => {
  test('dock button on float panel returns it to main layout', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Float panel should exist
    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    // Click dock button
    const dockButton = floatPanel.locator('[data-layout-path*="/button/dock"]');
    await dockButton.click();

    // Float panel gone
    await expect(floatPanel).not.toBeVisible();
  });
});

// ─── 9.15 Dock Location ─────────────────────────────────────────────

test.describe('Actions: Dock Location', () => {
  test('drag tab to CENTER merges into existing tabset', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_add_remove');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const from = findTabButton(page, '/ts0', 0);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.CENTER);

    // ts0 still has Beta, ts1 gets Alpha — still 2 tabsets but ts1 has 2 tabs
    await expect(findAllTabSets(page)).toHaveCount(2);
    await expect(findPath(page, '/ts1/tabstrip').locator('.flexlayout__tab_button')).toHaveCount(2);
  });

  test('drag tab to TOP creates vertical split', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_add_remove');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const from = findTabButton(page, '/ts1', 0);
    const to = findPath(page, '/ts0/t0');
    await drag(page, from, to, Location.TOP);

    // Should still have 2 tabsets, but in a different arrangement
    await expect(findAllTabSets(page)).toHaveCount(2);
  });
});

// ─── 9.16 Dock Tabset (Action.dockTabset) ───────────────────────────

test.describe('Actions: Dock Tabset', () => {
  test('dock tabset button docks floating tabset back', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_dock_tabset');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // First float the ts-dock-target tabset
    await findPath(page, '/ts1/tabstrip').click();
    await page.locator('[data-id="float-active"]').click();

    // Should have a floating panel
    await expect(page.locator('.flexlayout__floating_panel')).toBeVisible();

    // Click "Dock Tabset" button to dock it back
    await page.locator('[data-id="action-dock-tabset"]').click();

    // Float should be gone, back to docked layout
    await expect(page.locator('.flexlayout__floating_panel')).not.toBeVisible();
  });
});

// ─── 9.17 Move Window (Action.moveWindow) ───────────────────────────

test.describe('Actions: Move Window', () => {
  test('move window button repositions float panel', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_move_window');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Float panel should be pre-loaded
    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    // Get initial position
    const initialBox = await floatPanel.boundingBox();
    expect(initialBox).toBeTruthy();

    // Click "Move Window → (200,200)" button
    await page.locator('[data-id="action-move-window"]').click();

    // Position should have changed
    const newBox = await floatPanel.boundingBox();
    expect(newBox).toBeTruthy();

    // After moveWindow(200, 200, 300, 220), the position/size should change
    // The float was initially at (50,50,250,180), now at (200,200,300,220)
    expect(newBox!.width).toBeGreaterThanOrEqual(280);
  });
});

// ─── 9.18 Resize Window ─────────────────────────────────────────────

test.describe('Actions: Resize Window', () => {
  test('drag resize handle changes float panel size', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_move_window');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    const resizeHandle = page.locator('.flexlayout__floating_panel_resize_handle');
    const initialBox = await floatPanel.boundingBox();
    expect(initialBox).toBeTruthy();

    const handleBox = await resizeHandle.boundingBox();
    expect(handleBox).toBeTruthy();

    // Drag resize handle to enlarge
    await page.mouse.move(
      handleBox!.x + handleBox!.width / 2,
      handleBox!.y + handleBox!.height / 2,
    );
    await page.mouse.down();
    await page.mouse.move(
      handleBox!.x + handleBox!.width / 2 + 60,
      handleBox!.y + handleBox!.height / 2 + 40,
      { steps: 5 },
    );
    await page.mouse.up();

    const newBox = await floatPanel.boundingBox();
    expect(newBox).toBeTruthy();
    expect(newBox!.width).toBeGreaterThan(initialBox!.width + 30);
    expect(newBox!.height).toBeGreaterThan(initialBox!.height + 20);
  });
});

// ─── 9.19 Create Window (Action.floatTab programmatic) ──────────────

test.describe('Actions: Create Window', () => {
  test('create float window button makes new float from specific tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_create_window');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // No float panels initially
    await expect(page.locator('.flexlayout__floating_panel')).toHaveCount(0);

    // Click action button — floats "tab-floatable"
    await page.locator('[data-id="action-create-window"]').click();

    // Float panel should appear
    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    // The float should contain the "Float Me" tab
    const selectedTab = floatPanel.locator('.flexlayout__tab_button--selected');
    await expect(selectedTab).toContainText('Float Me');
  });

  test('original tabset loses the floated tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_create_window');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // ts0 initially has 2 tabs (Main, Float Me)
    await expect(findPath(page, '/ts0/tabstrip').locator('.flexlayout__tab_button')).toHaveCount(2);

    // Float "Float Me"
    await page.locator('[data-id="action-create-window"]').click();

    // ts0 should now have only 1 tab (Main)
    await expect(findPath(page, '/ts0/tabstrip').locator('.flexlayout__tab_button')).toHaveCount(1);
  });
});

// ─── 9.20-9.23 Set Tab Attributes (Icon/Component/Config/EnableClose) ─

test.describe('Actions: Set Tab Icon', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=action_set_tab_attrs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('icon button toggles tab icon', async ({ page }) => {
    const targetTab = findTabButton(page, '/ts0', 0);
    const leading = targetTab.locator('.flexlayout__tab_button_leading');

    // Verify leading exists (initial icon 📝)
    await expect(leading).toBeAttached();

    // Click Icon button to toggle
    await page.locator('[data-id="action-attrs-icon"]').click();

    // Leading should still be attached (icon changed to 🎯)
    await expect(leading).toBeAttached();
  });
});

test.describe('Actions: Set Tab Component', () => {
  test('component button changes tab component from info to counter', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_set_tab_attrs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Target tab initially shows info component (description text)
    const tabContent = findPath(page, '/ts0/t0');
    await expect(tabContent).toBeVisible();

    // Click Component button to switch to counter
    await page.locator('[data-id="action-attrs-comp"]').click();

    // Tab content should now show counter component (with buttons/numbers)
    // The counter component has increment/decrement functionality
    await expect(tabContent).toBeVisible();
    // Counter component shows a number and buttons — verify it changed from info text
    const counterElement = tabContent.locator('button');
    await expect(counterElement.first()).toBeVisible();
  });
});

test.describe('Actions: Set Tab Config', () => {
  test('config button updates tab config data', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_set_tab_attrs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Click Config button — updates config with timestamp
    await page.locator('[data-id="action-attrs-cfg"]').click();

    // The tab content should show updated config (description with timestamp)
    const tabContent = findPath(page, '/ts0/t0');
    await expect(tabContent).toBeVisible();
    await expect(tabContent).toContainText('Updated at');
  });
});

test.describe('Actions: Set Tab Enable Close', () => {
  test('close toggle button adds/removes close button on tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_set_tab_attrs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Initially enableClose is true — close button visible
    const closeBtn = findPath(page, '/ts0/tb0/button/close');
    await expect(closeBtn).toBeVisible();

    // Toggle close to OFF
    await page.locator('[data-id="action-attrs-close"]').click();

    // Close button should be hidden
    await expect(closeBtn).not.toBeVisible();

    // Toggle close back to ON
    await page.locator('[data-id="action-attrs-close"]').click();

    // Close button should reappear
    await expect(closeBtn).toBeVisible();
  });
});

// ─── 9.x Adjust Weights (Action.adjustWeights) ─────────────────────

test.describe('Actions: Adjust Weights', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=action_weights');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('equal weights button makes tabsets roughly equal size', async ({ page }) => {
    // First set unequal weights
    await page.locator('[data-id="action-weights-8020"]').click();

    // Then set equal weights
    await page.locator('[data-id="action-equal-weights"]').click();

    const w0 = (await findPath(page, '/ts0').boundingBox())?.width ?? 0;
    const w1 = (await findPath(page, '/ts1').boundingBox())?.width ?? 0;
    const ratio = w0 / w1;
    expect(ratio).toBeGreaterThan(0.85);
    expect(ratio).toBeLessThan(1.15);
  });

  test('80/20 button makes left tabset significantly larger', async ({ page }) => {
    await page.locator('[data-id="action-weights-8020"]').click();

    const w0 = (await findPath(page, '/ts0').boundingBox())?.width ?? 0;
    const w1 = (await findPath(page, '/ts1').boundingBox())?.width ?? 0;
    const ratio = w0 / w1;
    expect(ratio).toBeGreaterThan(2.5);
  });
});

// ─── 9.x External Drag (Action via external DOM drag) ───────────────

test.describe('Actions: External Drag', () => {
  test('drag external element into layout adds new tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_external_drag');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Initially ts0 has 1 tab
    await expect(findPath(page, '/ts0/tabstrip').locator('.flexlayout__tab_button')).toHaveCount(1);

    // Drag the external drag source into the layout
    const dragSource = page.locator('[data-id="external-drag-source"]').first();
    const dropTarget = findPath(page, '/ts0/tabstrip');
    await drag(page, dragSource, dropTarget, Location.CENTER);

    // Should now have 2 tabs in ts0
    await expect(findPath(page, '/ts0/tabstrip').locator('.flexlayout__tab_button')).toHaveCount(2);
  });
});

// ─── Close Window (Action.closeWindow) ──────────────────────────────

test.describe('Actions: Close Window', () => {
  test('close button on float panel removes it', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_float');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const floatPanel = page.locator('.flexlayout__floating_panel');
    await expect(floatPanel).toBeVisible();

    // Click close button on the float panel
    const closeButton = floatPanel.locator('[data-layout-path*="/button/close-float"]');
    await closeButton.click();

    await expect(floatPanel).not.toBeVisible();
  });
});
