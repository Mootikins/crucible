import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady, openNewSessionTab } from './helpers/nav';

/**
 * E2E: Empty panes are VOID.
 *
 * Closing every center tab used to reveal a composer splash. The composer now
 * lives in its own New Session tab (ribbon → New session); a pane with no tabs
 * renders nothing at all — no splash, no tab strip, no hint.
 */

/** Close every tab in every center-tiling group, through the real action. */
async function closeAllCenterTabs(page: import('@playwright/test').Page) {
  // Click-driven close loops over a global `[data-tab-id]` selector that spans
  // every pane/edge-panel group; as groups empty out mid-loop (edge panels
  // auto-collapse, center panes collapse out of the layout tree) the tab count
  // and DOM order shift under the loop. Drive the store instead — the SAME
  // removeTab action the close button calls.
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
}

test('an emptied center pane renders nothing', async ({ page }) => {
  await setupBasicMocks(page);
  await page.goto('/');
  await appReady(page);

  // Put a tab in the CENTER (the New Session tab docks right), then empty it.
  await page.evaluate(async () => {
    const { openPanelTab } = await import('/src/lib/panel-actions.ts');
    openPanelTab('settings');
  });
  await expect(
    page.locator('[data-tab-id="tab-settings"]:not([data-testid^="edge-tab-"])'),
  ).toBeVisible({ timeout: 10000 });

  await closeAllCenterTabs(page);

  // No composer, no tab strip — void.
  await expect(page.getByTestId('composer-input')).toHaveCount(0);
  await expect(page.getByTestId('center-composer')).toHaveCount(0);
  // And no leftover "select a tab" style hint.
  await expect(page.getByText('Select a tab')).toHaveCount(0);
});

test('the session composer is reachable from the ribbon, not from an empty pane', async ({ page }) => {
  await setupBasicMocks(page);
  await page.goto('/');
  await appReady(page);

  // Nothing on a fresh center pane…
  await expect(page.getByTestId('composer-input')).toHaveCount(0);

  // …until New Session is opened deliberately.
  await openNewSessionTab(page);
  await expect(page.getByTestId('center-composer')).toBeVisible();
});
