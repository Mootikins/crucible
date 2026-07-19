import { test, expect } from '@playwright/test';

/**
 * E2E: Empty State Recovery
 * 
 * Verifies that when the center pane has no tabs, an empty state appears
 * with clear guidance.
 */

test('Empty state appears when all center tabs are closed', async ({ page }) => {
  await page.goto('/');

  // Wait for the app to load
  const mainLayout = page.locator('div[class*="flex-col"][class*="h-screen"]');
  await expect(mainLayout).toBeVisible({ timeout: 5000 });
  await expect(page.locator('[data-tab-id]').first()).toBeVisible({ timeout: 5000 });

  // Click-driven close loops over a global `[data-tab-id]` selector that
  // spans every pane/edge-panel group; as groups empty out mid-loop (edge
  // panels auto-collapse, center panes collapse out of the layout tree —
  // see collapseEmptyNodes/removeTab in src/stores/tabActions.ts) the tab
  // count and DOM order can shift under the loop. Go straight through the
  // store instead: close every tab in every pane-tiling group (this drives
  // the SAME removeTab action the close button calls, so it's not testing
  // a mock — it's the deterministic form of the same close), which is what
  // actually needs to happen for the center EmptyState to render (emptying
  // only one group just collapses it away, leaving its non-empty sibling
  // filling the layout with no empty state anywhere).
  await page.evaluate(() => {
    const store = (window as unknown as { __windowStore: any }).__windowStore;
    const actions = (window as unknown as { __windowActions: any }).__windowActions;

    const findAllPaneGroupIds = (node: any): string[] => {
      if (node.type === 'pane') return node.tabGroupId ? [node.tabGroupId] : [];
      return [...findAllPaneGroupIds(node.first), ...findAllPaneGroupIds(node.second)];
    };

    for (const groupId of findAllPaneGroupIds(store.layout)) {
      const tabs = [...(store.tabGroups[groupId]?.tabs ?? [])];
      for (const tab of tabs) {
        actions.removeTab(groupId, tab.id);
      }
    }
  });

  // The empty state should appear in the center pane when no tabs are open
  const emptyStateHeading = page.locator('text=No session open');
  await expect(emptyStateHeading).toBeVisible({ timeout: 2000 });

  // Verify the empty state has helpful text
  const emptyStateText = page.locator('text=Select a session from the left panel');
  await expect(emptyStateText).toBeVisible();
});

test('Empty state is not shown in non-center panes', async ({ page }) => {
  await page.goto('/');

  // Wait for the app to load
  const mainLayout = page.locator('div[class*="flex-col"][class*="h-screen"]');
  await expect(mainLayout).toBeVisible({ timeout: 5000 });

  // The left panel should show "Drop tabs here" or similar, not the empty state
  // This test ensures the empty state is only shown in the center pane
  const leftPanel = page.locator('[class*="EdgePanel"]').first();
  
  // The empty state message "No session open" should NOT appear in the left panel
  const emptyStateInLeftPanel = leftPanel.locator('text=No session open');
  await expect(emptyStateInLeftPanel).not.toBeVisible();
});
