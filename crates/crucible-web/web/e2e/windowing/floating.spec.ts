import { test, expect } from '@playwright/test';
import { act, groupIds, openHarness, readStore, tabIds } from './harness';

/** Floating windows: creation at a position, pop-out, and dock. */

test.beforeEach(async ({ page }) => openHarness(page));

test('creates a floating window at requested position', async ({ page }) => {
  const [centre] = await groupIds(page, 'center');
  await act(page, 'createFloatingWindow', centre, 320, 180, 420, 260);

  const { floatingWindows } = await readStore(page);
  expect(floatingWindows).toHaveLength(1);
  expect(floatingWindows[0]).toMatchObject({ x: 320, y: 180, width: 420, height: 260 });
  await expect(page.locator('div[style*="left: 320px"][style*="top: 180px"]')).toBeVisible();
});

test('pop-out MOVES the tabs to a floating window (no mirrored group)', async ({ page }) => {
  // The drag registry must stay coherent while the group moves between tab
  // bars. A "Cannot remove nonexistent draggable/droppable" warning means a
  // cleanup took the new container's registration, and the tab can no longer
  // be dragged.
  const dndWarnings: string[] = [];
  page.on('console', (msg) => {
    if (msg.text().includes('nonexistent')) dndWarnings.push(msg.text());
  });

  await page.locator('button[title="Pop out to floating window"]').first().click();

  const floating = page.locator('[data-window-id]');
  await expect(floating).toHaveCount(1);
  await expect(floating.locator('[data-tab-id="tab-alpha"]')).toBeVisible();

  // Each tab exists exactly ONCE across all tab strips. The old pop-out
  // shared the group between the pane and the window: two tab strips, two
  // drag registrations under one id.
  await expect(page.locator('[data-tab-id="tab-alpha"]')).toHaveCount(1);
  await expect(page.locator('[data-tab-id="tab-beta"]')).toHaveCount(1);
  const s = await readStore(page);
  expect(s.floatingWindows).toHaveLength(1);
  expect(s.tabGroups[s.floatingWindows[0]!.tabGroupId]!.tabs.map((t) => t.id)).toEqual([
    'tab-alpha',
    'tab-beta',
  ]);
  expect(await tabIds(page, 'center')).toEqual([]);

  // Closing the floating window closes its tabs with it. Nothing is orphaned.
  await page.locator('button[title="Close (closes its tabs)"]').click();
  await expect(page.locator('[data-tab-id="tab-alpha"]')).toHaveCount(0);
  await expect(page.locator('[data-tab-id="tab-beta"]')).toHaveCount(0);
  expect((await readStore(page)).floatingWindows).toHaveLength(0);
  expect(dndWarnings).toEqual([]);
});

test('dock button moves a floating window back into the layout', async ({ page }) => {
  await page.locator('button[title="Pop out to floating window"]').first().click();
  const dock = page.locator('button[title="Dock back into the layout"]');
  await expect(dock).toBeVisible();

  await dock.click();

  // The floating window is gone, and the tabs are back in a pane, still unique.
  await expect(dock).toHaveCount(0);
  await expect(page.locator('[data-window-id]')).toHaveCount(0);
  await expect(page.locator('[data-tab-id="tab-alpha"]')).toHaveCount(1);
  expect((await readStore(page)).floatingWindows).toHaveLength(0);
  expect((await tabIds(page, 'center')).sort()).toEqual(['tab-alpha', 'tab-beta']);
});
