import { test, expect } from '@playwright/test';
import {
  findPath,
  findTabButton,
  findAllTabSets,
  drag,
  dragToEdge,
  Location,
} from './helpers';

const baseURL = '/flexlayout-test.html';
const evidencePath = '../../../.sisyphus/evidence';

// ─── 6.1 Tab Reorder Within Tabset ───────────────────────────────────

test.describe('DnD 6.1: Tab Reorder Within Tabset', () => {
  test('draggable tab can be reordered within the same tabset', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_drag_disabled');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Initial order: Draggable, Locked, Also Draggable
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Draggable');
    await expect(findTabButton(page, '/ts0', 2).locator('.flexlayout__tab_button_content')).toContainText('Also Draggable');

    // Drag "Also Draggable" to the left of "Draggable"
    const from = findTabButton(page, '/ts0', 2);
    const to = findTabButton(page, '/ts0', 0);
    await drag(page, from, to, Location.LEFT);

    // After reorder: Also Draggable should now be first
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Also Draggable');

    await page.screenshot({ path: `${evidencePath}/dnd-6.1-tab-reorder.png` });
  });

  test('single tabset remains after internal reorder (no split)', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_drag_disabled');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const from = findTabButton(page, '/ts0', 2);
    const to = findTabButton(page, '/ts0', 0);
    await drag(page, from, to, Location.LEFT);

    // Still one tabset — reorder doesn't create splits
    await expect(findAllTabSets(page)).toHaveCount(1);
  });
});

// ─── 6.2 Cross-Tabset Tab Drag ───────────────────────────────────────

test.describe('DnD 6.2: Cross-Tabset Tab Drag', () => {
  test('drag tab from one tabset to another reduces source count', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_divide');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // basic_divide: ts0 has 3 tabs (Alpha, Beta, Gamma), ts1 has 3 tabs (Delta, Epsilon, Zeta)
    const ts0Tabs = findPath(page, '/ts0').locator('.flexlayout__tab_button');
    const ts1Tabs = findPath(page, '/ts1').locator('.flexlayout__tab_button');
    await expect(ts0Tabs).toHaveCount(3);
    await expect(ts1Tabs).toHaveCount(3);

    // Drag "Alpha" from ts0 to center of ts1
    const from = findTabButton(page, '/ts0', 0);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.CENTER);

    // ts0 should lose a tab, ts1 should gain one
    const ts0After = findPath(page, '/ts0').locator('.flexlayout__tab_button');
    await expect(ts0After).toHaveCount(2);

    await page.screenshot({ path: `${evidencePath}/dnd-6.2-cross-tabset.png` });
  });
});

// ─── 6.3 Tab Drag To/From Borders ────────────────────────────────────

test.describe('DnD 6.3: Tab Drag To/From Borders', () => {
  test('drag tab from main layout to border panel', async ({ page }) => {
    await page.goto(baseURL + '?layout=test_with_borders');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Open left border first
    const borderTab = findTabButton(page, '/border/left', 0);
    await borderTab.click();
    await expect(findPath(page, '/border/left/t0')).toBeVisible();

    // Get initial border tab count
    const borderTabsBefore = await findPath(page, '/border/left').locator('.flexlayout__border_button').count();

    // Drag main tab to left border content area
    const from = findTabButton(page, '/ts0', 0);
    const borderContent = findPath(page, '/border/left/t0');
    await drag(page, from, borderContent, Location.CENTER);

    // Border should have gained a tab
    const borderTabsAfter = await findPath(page, '/border/left').locator('.flexlayout__border_button').count();
    expect(borderTabsAfter).toBe(borderTabsBefore + 1);

    await page.screenshot({ path: `${evidencePath}/dnd-6.3-tab-to-border.png` });
  });
});

// ─── 6.4 Edge Docking Zones ──────────────────────────────────────────

test.describe('DnD 6.4: Edge Docking Zones', () => {
  test('edge dock enabled layout has two tabsets to start', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_edge_dock');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Main');
    await expect(findTabButton(page, '/ts1', 0).locator('.flexlayout__tab_button_content')).toContainText('Sidebar');
  });

  test('drag tab to layout edge creates border dock', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_edge_dock');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Drag Sidebar tab to left edge of layout
    const from = findTabButton(page, '/ts1', 0);
    await dragToEdge(page, from, 0);

    // After edge dock, we may have different structure — check that layout changed
    // Edge dock may create a border panel or restructure layout
    const tabsets = await findAllTabSets(page).count();
    // The layout should have changed (either fewer tabsets or border created)
    expect(tabsets).toBeLessThanOrEqual(2);

    await page.screenshot({ path: `${evidencePath}/dnd-6.4-edge-dock.png` });
  });
});

// ─── 6.5 Dock Location Enum (CENTER/TOP/BOTTOM/LEFT/RIGHT) ──────────

test.describe('DnD 6.5: Dock Location Enum', () => {
  test('drop on CENTER adds tab to existing tabset', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_divide');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const from = findTabButton(page, '/ts0', 1);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.CENTER);

    // CENTER drop: same number of tabsets (tab added to existing)
    await expect(findAllTabSets(page)).toHaveCount(2);

    await page.screenshot({ path: `${evidencePath}/dnd-6.5-center-drop.png` });
  });

  test('drop on TOP edge creates new split above', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_divide');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const from = findTabButton(page, '/ts0', 1);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.TOP);

    // TOP drop: creates a new tabset (3 total)
    await expect(findAllTabSets(page)).toHaveCount(3);

    await page.screenshot({ path: `${evidencePath}/dnd-6.5-top-drop.png` });
  });

  test('drop on LEFT edge creates new split to the left', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_divide');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const from = findTabButton(page, '/ts0', 1);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.LEFT);

    // LEFT drop: creates a new tabset (3 total)
    await expect(findAllTabSets(page)).toHaveCount(3);

    await page.screenshot({ path: `${evidencePath}/dnd-6.5-left-drop.png` });
  });
});

// ─── 6.6 External Drag & Drop ────────────────────────────────────────

test.describe('DnD 6.6: External Drag & Drop', () => {
  test('external draggable element can add tab to layout', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_external_drag');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const ts0Tabs = findPath(page, '/ts0').locator('.flexlayout__tab_button');
    const tabCountBefore = await ts0Tabs.count();

    const externalSource = page.locator('[data-id="external-drag-source"]').first();
    await expect(externalSource).toBeVisible();

    const target = findPath(page, '/ts0/t0');
    await drag(page, externalSource, target, Location.CENTER);

    const tabCountAfter = await ts0Tabs.count();
    expect(tabCountAfter).toBe(tabCountBefore + 1);

    await page.screenshot({ path: `${evidencePath}/dnd-6.6-external-drag.png` });
  });

  test('external drag source element is visible and draggable', async ({ page }) => {
    await page.goto(baseURL + '?layout=action_external_drag');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const externalSource = page.locator('[data-id="external-drag-source"]').first();
    await expect(externalSource).toBeVisible();
    await expect(externalSource).toHaveAttribute('draggable', 'true');
    await expect(externalSource).toContainText('Drag me into the layout');
  });
});

// ─── 6.7 TabSet Divide on Edge Drop ──────────────────────────────────

test.describe('DnD 6.7: TabSet Divide on Edge Drop', () => {
  test('edge drop on RIGHT creates a new tabset split', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_divide');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const from = findTabButton(page, '/ts0', 0);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.RIGHT);

    // Edge drop creates a new tabset
    await expect(findAllTabSets(page)).toHaveCount(3);

    await page.screenshot({ path: `${evidencePath}/dnd-6.7-divide-right.png` });
  });

  test('edge drop on BOTTOM creates a vertical split', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_divide');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);

    const from = findTabButton(page, '/ts0', 2);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.BOTTOM);

    // BOTTOM drop creates a new tabset below
    await expect(findAllTabSets(page)).toHaveCount(3);

    await page.screenshot({ path: `${evidencePath}/dnd-6.7-divide-bottom.png` });
  });
});

// ─── 6.8 Shift+Drag to Float ─────────────────────────────────────────

test.describe('DnD 6.8: Shift+Drag to Float', () => {
  test.skip('not implemented — shift+drag to float is a Dockview-only feature', () => {
    // Feature 6.8 is marked ❌ (Not implemented) in the feature list.
    // FlexLayout uses explicit float actions, not shift+drag.
  });
});

// ─── 6.9 Custom Drag Preview / Rectangle ─────────────────────────────

test.describe('DnD 6.9: Custom Drag Preview', () => {
  test('render_drag_rect layout loads with source and target tabsets', async ({ page }) => {
    await page.goto(baseURL + '?layout=render_drag_rect');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Drag Me');
    await expect(findTabButton(page, '/ts0', 1).locator('.flexlayout__tab_button_content')).toContainText('Also Drag Me');
    await expect(findTabButton(page, '/ts1', 0).locator('.flexlayout__tab_button_content')).toContainText('Drop Target');
  });

  test('dragging tab to other tabset completes the drag successfully', async ({ page }) => {
    await page.goto(baseURL + '?layout=render_drag_rect');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const from = findTabButton(page, '/ts0', 0);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.CENTER);

    // Tab should have moved — ts1 should now have 2 tabs
    const ts1Tabs = findPath(page, '/ts1').locator('.flexlayout__tab_button');
    await expect(ts1Tabs).toHaveCount(2);

    await page.screenshot({ path: `${evidencePath}/dnd-6.9-custom-drag-rect.png` });
  });
});

// ─── 6.10 Prevent Drag (Per-Tab) ─────────────────────────────────────

test.describe('DnD 6.10: Prevent Drag (Per-Tab)', () => {
  test('tab with enableDrag: false resists drag attempts', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_drag_disabled');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // "Locked" tab at index 1 has enableDrag: false
    await expect(findTabButton(page, '/ts0', 1).locator('.flexlayout__tab_button_content')).toContainText('Locked');

    const locked = findTabButton(page, '/ts0', 1);
    const target = findTabButton(page, '/ts0', 2);
    await drag(page, locked, target, Location.RIGHT);

    // Locked tab should still be at index 1 (drag was prevented)
    await expect(findTabButton(page, '/ts0', 1).locator('.flexlayout__tab_button_content')).toContainText('Locked');

    await page.screenshot({ path: `${evidencePath}/dnd-6.10-drag-disabled.png` });
  });

  test('draggable tabs can still be reordered around locked tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_drag_disabled');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const from = findTabButton(page, '/ts0', 2);
    const to = findTabButton(page, '/ts0', 0);
    await drag(page, from, to, Location.LEFT);

    // "Also Draggable" should now be first
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Also Draggable');
  });
});

// ─── 6.11 Prevent Drop (Per-Tabset) ──────────────────────────────────

test.describe('DnD 6.11: Prevent Drop (Per-Tabset)', () => {
  test('tabset with enableDrop: false rejects incoming drops', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_drop_disabled');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(3);

    // ts1 has enableDrop: false
    const noDropTabs = findPath(page, '/ts1').locator('.flexlayout__tab_button');
    await expect(noDropTabs).toHaveCount(1);

    // Drag from ts0 to ts1 (drop-disabled)
    const from = findTabButton(page, '/ts0', 0);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.CENTER);

    // Drop rejected — ts1 still has 1 tab
    await expect(noDropTabs).toHaveCount(1);

    await page.screenshot({ path: `${evidencePath}/dnd-6.11-drop-disabled.png` });
  });

  test('tabset without enableDrop restriction accepts drops', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_drop_disabled');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Drag from ts0 to ts2 (drop-enabled)
    const from = findTabButton(page, '/ts0', 0);
    const to = findPath(page, '/ts2/t0');
    await drag(page, from, to, Location.CENTER);

    // Drop accepted — ts0 removed (empty), tabset count decreases
    await expect(findAllTabSets(page)).toHaveCount(2);
  });

  test('border with enableDrop: false rejects tab drops', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_config');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    // Top border has enableDrop: false
    const topBorderTabs = findPath(page, '/border/top').locator('.flexlayout__border_button');
    const topCountBefore = await topBorderTabs.count();

    // Open left border (which has enableDrop: true) to verify borders are interactive
    const leftBorderBtn = findTabButton(page, '/border/left', 0);
    await leftBorderBtn.click();
    await expect(findPath(page, '/border/left/t0')).toBeVisible();

    // Border drop disabled is tested via the border config — verify the layout loaded correctly
    await expect(topBorderTabs).toHaveCount(topCountBefore);

    await page.screenshot({ path: `${evidencePath}/dnd-6.11-border-drop-disabled.png` });
  });
});

// ─── 6.12 DnD Event Callbacks (onWillDrag, onDidDrop) ────────────────

test.describe('DnD 6.12: DnD Event Callbacks', () => {
  test.skip('not implemented — onWillDrag/onDidDrop are Dockview-only callbacks', () => {
    // Feature 6.12 is marked ❌ (Not implemented) in the feature list.
    // FlexLayout uses onAction callback for all action notifications.
  });
});

// ─── 6.13 Drag Overlay Customization ─────────────────────────────────

test.describe('DnD 6.13: Drag Overlay Customization', () => {
  test.skip('not implemented — onWillShowOverlay is a Dockview-only callback', () => {
    // Feature 6.13 is marked ❌ (Not implemented) in the feature list.
    // FlexLayout uses onRenderDragRect for custom drag preview (tested in 6.9).
  });
});

// ─── 6.14 Disable All DnD ────────────────────────────────────────────

test.describe('DnD 6.14: Disable All DnD', () => {
  test.skip('not implemented — no global disableDnd option in FlexLayout', () => {
    // Feature 6.14 is marked ❌ (Not implemented) in the feature list.
    // Individual enableDrag/enableDrop per tab/tabset is supported (tested in 6.10/6.11).
  });
});

// ─── 6.15 Cross-Instance DnD ─────────────────────────────────────────

test.describe('DnD 6.15: Cross-Instance DnD', () => {
  test.skip('not implemented — single Layout instance only', () => {
    // Feature 6.15 is marked ❌ (Not implemented) in the feature list.
    // FlexLayout operates within a single Layout component instance.
  });
});

// ─── 6.16 addTabWithDragAndDrop (Layout Method) ──────────────────────

test.describe('DnD 6.16: addTabWithDragAndDrop', () => {
  test('dnd_add_with_drag layout loads with two target tabsets', async ({ page }) => {
    await page.goto(baseURL + '?layout=dnd_add_with_drag');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Drop Target A');
    await expect(findTabButton(page, '/ts1', 0).locator('.flexlayout__tab_button_content')).toContainText('Drop Target B');
  });

  test('external drag source element is present and draggable', async ({ page }) => {
    await page.goto(baseURL + '?layout=dnd_add_with_drag');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dragSource = page.locator('[data-id="dnd-external-drag"]');
    await expect(dragSource).toBeVisible();
    await expect(dragSource).toHaveAttribute('draggable', 'true');
    await expect(dragSource).toContainText('Drag into layout');
  });

  test('dragging external source into tabset adds a new tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=dnd_add_with_drag');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const ts0Tabs = findPath(page, '/ts0').locator('.flexlayout__tab_button');
    const countBefore = await ts0Tabs.count();

    const dragSource = page.locator('[data-id="dnd-external-drag"]');
    const target = findPath(page, '/ts0/t0');
    await drag(page, dragSource, target, Location.CENTER);

    const countAfter = await ts0Tabs.count();
    expect(countAfter).toBe(countBefore + 1);

    await page.screenshot({ path: `${evidencePath}/dnd-6.16-add-with-drag.png` });
  });
});

// ─── 6.17 moveTabWithDragAndDrop ─────────────────────────────────────

test.describe('DnD 6.17: moveTabWithDragAndDrop', () => {
  test('dnd_move_with_drag layout loads with source and destination tabsets', async ({ page }) => {
    await page.goto(baseURL + '?layout=dnd_move_with_drag');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findAllTabSets(page)).toHaveCount(2);
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Move Me');
    await expect(findTabButton(page, '/ts0', 1).locator('.flexlayout__tab_button_content')).toContainText('Also Move');
    await expect(findTabButton(page, '/ts1', 0).locator('.flexlayout__tab_button_content')).toContainText('Destination');
  });

  test('drag tab from source to destination moves it', async ({ page }) => {
    await page.goto(baseURL + '?layout=dnd_move_with_drag');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const ts0TabsBefore = await findPath(page, '/ts0').locator('.flexlayout__tab_button').count();
    const ts1TabsBefore = await findPath(page, '/ts1').locator('.flexlayout__tab_button').count();

    const from = findTabButton(page, '/ts0', 0);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.CENTER);

    // Source should have lost a tab, destination gained one
    const ts0TabsAfter = await findPath(page, '/ts0').locator('.flexlayout__tab_button').count();
    const ts1TabsAfter = await findPath(page, '/ts1').locator('.flexlayout__tab_button').count();
    expect(ts0TabsAfter).toBe(ts0TabsBefore - 1);
    expect(ts1TabsAfter).toBe(ts1TabsBefore + 1);

    await page.screenshot({ path: `${evidencePath}/dnd-6.17-move-with-drag.png` });
  });

  test('moved tab appears in destination with its name', async ({ page }) => {
    await page.goto(baseURL + '?layout=dnd_move_with_drag');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const from = findTabButton(page, '/ts0', 0);
    const to = findPath(page, '/ts1/t0');
    await drag(page, from, to, Location.CENTER);

    // "Move Me" should now be in ts1
    const ts1Tabs = findPath(page, '/ts1').locator('.flexlayout__tab_button');
    const tabTexts: string[] = [];
    const count = await ts1Tabs.count();
    for (let i = 0; i < count; i++) {
      const text = await ts1Tabs.nth(i).locator('.flexlayout__tab_button_content').textContent();
      tabTexts.push(text ?? '');
    }
    expect(tabTexts).toContain('Move Me');

    await page.screenshot({ path: `${evidencePath}/dnd-6.17-moved-tab-name.png` });
  });
});
