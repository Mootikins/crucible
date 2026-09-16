import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION, MOCK_SESSION_2 } from './helpers/fixtures';
import { openSessionsList } from './helpers/nav';

/**
 * Windowing rules that belong to the app: session tabs, and what the app
 * does NOT draw in an emptied centre. The mechanics (splits, tab moves,
 * rails, floating windows) are core specs in e2e/windowing/, which drive the
 * harness page with no app.
 */

type LayoutNode = {
  type: 'pane' | 'split';
  id: string;
  tabGroupId?: string | null;
  first?: LayoutNode;
  second?: LayoutNode;
};

type WindowStoreShape = {
  layout: LayoutNode;
  tabGroups: Record<string, { tabs: Array<{ id: string }>; activeTabId: string | null }>;
};

type WindowActionsShape = {
  removeTab: (groupId: string, tabId: string) => void;
};

async function waitForApp(page: Page) {
  await setupBasicMocks(page, { sessions: [MOCK_SESSION, MOCK_SESSION_2] });
  await page.route('**/api/layout', async (route) => {
    const method = route.request().method();
    if (method === 'GET') {
      await route.fulfill({ status: 404, contentType: 'application/json', body: '{}' });
      return;
    }
    if (method === 'POST' || method === 'DELETE') {
      await route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
      return;
    }
    await route.continue();
  });
  await page.goto('/');
  await openSessionsList(page);
  // Readiness is asserted by the beforeEach (session-item visible), which only
  // renders once the app has fully mounted — no fixed settle needed.
}

test.describe('Windowing in the app', () => {
  test.beforeEach(async ({ page }) => {
    await waitForApp(page);
    const sessionItem = page.getByTestId('session-item-test-session-001');
    await expect(sessionItem).toBeVisible({ timeout: 5000 });
    await sessionItem.click();
    await expect(page.locator('[data-tab-id^="tab-chat-"]')).toBeVisible({ timeout: 5000 });
  });

  test('opening multiple sessions creates two unique chat tabs without duplicates', async ({ page }) => {
    await page.getByTestId('session-item-test-session-002').click();
    await expect(page.locator('[data-tab-id="tab-chat-test-session-001"]')).toBeVisible({ timeout: 3000 });
    await expect(page.locator('[data-tab-id="tab-chat-test-session-002"]')).toBeVisible({ timeout: 3000 });

    await page.getByTestId('session-item-test-session-001').click();
    await expect(page.locator('[data-tab-id^="tab-chat-"]')).toHaveCount(2, { timeout: 3000 });
  });

  test('shows center empty state after all center tabs are removed', async ({ page }) => {
    // The beforeEach's session click docks a chat tab in the right EDGE
    // panel — the center tiling keeps its own group(s). Closing a pane's last tab collapses that (now-empty)
    // pane out of the layout tree entirely (see removeTab/collapseEmptyNodes
    // in src/windowing/store/tabActions.ts + src/windowing/model/tree.ts) — so emptying
    // only ONE of the two groups just leaves its still-non-empty sibling
    // occupying the whole layout, and no EmptyState ever renders. Verified
    // via page.evaluate store dumps: emptying BOTH groups collapses the
    // layout down to a single pane node whose tabGroupId no longer resolves
    // to any group, which Pane.tsx renders as empty — the EmptyState. (No
    // Home tab auto-reopens here: that startup catch-up in App.tsx only
    // runs once on mount, not on later tab removal.)
    await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      const windowActions = (window as unknown as Record<string, unknown>).__windowActions as WindowActionsShape;

      const findAllPaneGroupIds = (node: LayoutNode): string[] => {
        if (node.type === 'pane') return node.tabGroupId ? [node.tabGroupId] : [];
        return [
          ...(node.first ? findAllPaneGroupIds(node.first) : []),
          ...(node.second ? findAllPaneGroupIds(node.second) : []),
        ];
      };

      for (const groupId of findAllPaneGroupIds(windowStore.layout)) {
        const tabs = [...(windowStore.tabGroups[groupId]?.tabs ?? [])];
        for (const tab of tabs) {
          windowActions.removeTab(groupId, tab.id);
        }
      }
    });

    // An emptied center pane is void — no composer splash, no tab strip.
    // The empty pane shows first, so the absence checks below are not vacuous.
    await expect(page.getByTestId('empty-pane')).toBeVisible();
    await expect(page.getByTestId('center-composer')).toHaveCount(0);
    await expect(page.getByTestId('composer-input')).toHaveCount(0);
  });
});
