import { test, expect, type Page } from '@playwright/test';
import { act, openHarness, readStore } from './harness';

/**
 * Regression: a layout restore replaces the pane ids in the store under Pane
 * components that are already mounted. Their drop targets kept the pane id
 * of the first render, so every pane drop (split zones and centre) carried a
 * stale id and did nothing. The restore below lands after the first render,
 * as a restore from the server does in the app.
 *
 * The payload is v10 and names every group that its panes use, so the core
 * reads it with no legacy upgrade and no rail repair.
 */
const RESTORED_LAYOUT = {
  version: 10,
  layout: { id: 'restored-pane-1', type: 'pane', tabGroupId: 'restored-group-1' },
  tabGroups: {
    'restored-group-1': {
      id: 'restored-group-1',
      tabs: [
        { id: 'tab-beta', title: 'Beta', contentType: 'beta' },
        { id: 'tab-gamma', title: 'Gamma', contentType: 'gamma' },
      ],
      activeTabId: 'tab-gamma',
    },
    'restored-left': {
      id: 'restored-left',
      tabs: [{ id: 'tab-left', title: 'Left', contentType: 'gamma' }],
      activeTabId: 'tab-left',
    },
    'restored-right': {
      id: 'restored-right',
      tabs: [{ id: 'tab-right', title: 'Right', contentType: 'gamma' }],
      activeTabId: 'tab-right',
    },
  },
  edgePanels: {
    left: {
      id: 'left-panel',
      layout: { id: 'restored-left-pane', type: 'pane', tabGroupId: 'restored-left' },
      mode: 'strip',
      width: 280,
    },
    right: {
      id: 'right-panel',
      layout: { id: 'restored-right-pane', type: 'pane', tabGroupId: 'restored-right' },
      mode: 'strip',
      width: 250,
    },
  },
  floatingWindows: [],
};

async function openRestored(page: Page): Promise<void> {
  await openHarness(page);
  // The harness has painted its seed. Restore over it now.
  await act(page, 'importLayout', RESTORED_LAYOUT);
  // tab-gamma exists only in the restored layout, so it marks the restore.
  await expect(page.locator('[data-tab-id="tab-gamma"]')).toBeVisible();
  await expect(page.locator('[data-tab-id="tab-alpha"]')).toHaveCount(0);
  const s = await readStore(page);
  expect(s.layout.id).toBe('restored-pane-1');
}

/**
 * The right fifth of the centre pane that holds Gamma. The pane's own box is
 * the only honest source for a drop inside it.
 */
async function centrePaneRightFifth(page: Page): Promise<{ x: number; y: number }> {
  const pane = page.locator('[data-pane-id]', { has: page.locator('[data-tab-id="tab-gamma"]') }).last();
  const box = (await pane.boundingBox())!;
  return { x: Math.floor(box.x + box.width * 0.9), y: Math.floor(box.y + box.height / 2) };
}

/** Drag, wait for `highlight` (the active drop indicator), then release. */
async function pointerDragUntil(
  page: Page,
  from: { x: number; y: number },
  to: { x: number; y: number },
  highlight: string,
) {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 15 });
  await expect(page.locator(highlight).first()).toBeVisible();
  await page.mouse.up();
}

async function splitByDrag(page: Page): Promise<void> {
  const box = (await page.locator('[data-tab-id="tab-gamma"]').boundingBox())!;
  await pointerDragUntil(
    page,
    { x: box.x + box.width / 2, y: box.y + box.height / 2 },
    await centrePaneRightFifth(page),
    '[class*="bg-primary/30"]',
  );
  // A real split creates a resize splitter between the two panes. DOM
  // ancestry is a false positive, because a rail move also changes it.
  await expect(page.locator('[data-testid="resize-splitter"]')).toHaveCount(1);
}

test('pane split by drag works after a delayed layout restore', async ({ page }) => {
  await openRestored(page);
  await expect(page.locator('[data-testid="resize-splitter"]')).toHaveCount(0);

  await splitByDrag(page);

  const { layout } = await readStore(page);
  expect(layout.type).toBe('split');
  expect(layout.direction).toBe('horizontal');
});

test('drop onto the tab bar of a restored group still moves tabs', async ({ page }) => {
  await openRestored(page);
  await splitByDrag(page);

  // Drag Gamma back onto the first group's tab bar. This drives the restored
  // `tabgroup:` drop target.
  const betaBox = (await page.locator('[data-tab-id="tab-beta"]').boundingBox())!;
  const box = (await page.locator('[data-tab-id="tab-gamma"]').boundingBox())!;
  await pointerDragUntil(
    page,
    { x: box.x + box.width / 2, y: box.y + box.height / 2 },
    { x: betaBox.x + betaBox.width + 40, y: betaBox.y + betaBox.height / 2 },
    // The centre tab bar's active-drop underline.
    '[class*="h-0.5"][class*="bg-primary"]',
  );

  // The tab is back in the first group, and the empty pane is pruned.
  await expect(page.locator('[data-testid="resize-splitter"]')).toHaveCount(0);
  const s = await readStore(page);
  expect(s.layout).toMatchObject({ type: 'pane', tabGroupId: 'restored-group-1' });
  expect(s.tabGroups['restored-group-1']!.tabs.map((t) => t.id).sort()).toEqual(['tab-beta', 'tab-gamma']);
});
