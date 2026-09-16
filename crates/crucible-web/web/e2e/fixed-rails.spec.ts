import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady } from './helpers/nav';

/**
 * The app's fixed rails (WS-324): the last Sessions panel and the last Files
 * panel do not close, though a user may move them. The layout menu on the
 * left rail puts a closed panel back.
 *
 * These are app rules. The core lets every tab close; the app's window
 * policy refuses the last Sessions and Files tabs.
 */

test.beforeEach(async ({ page }) => {
  await setupBasicMocks(page);
  await page.goto('/');
  await appReady(page);
  await expect(page.getByTestId('edge-tab-left-sessions-tab')).toBeVisible();
});

/** The left rail's tab ids, in strip order, across its panes. */
function leftTabIds(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const store = (window as unknown as Record<string, any>).__windowStore;
    const ids: string[] = [];
    const walk = (n: any): void => {
      if (n.type === 'pane') {
        for (const t of store.tabGroups[n.tabGroupId]?.tabs ?? []) ids.push(t.id);
        return;
      }
      walk(n.first);
      walk(n.second);
    };
    walk(store.edgePanels.left.layout);
    return ids;
  });
}

/** The group that holds a tab, wherever it is. */
function groupOf(page: Page, tabId: string): Promise<string | null> {
  return page.evaluate((id) => {
    const store = (window as unknown as Record<string, any>).__windowStore;
    const entry = Object.entries(store.tabGroups as Record<string, { tabs: { id: string }[] }>).find(
      ([, g]) => g.tabs.some((t) => t.id === id),
    );
    return entry ? entry[0] : null;
  }, tabId);
}

function windowAction(page: Page, name: string, ...args: unknown[]): Promise<unknown> {
  return page.evaluate(
    ([n, a]) => (window as unknown as Record<string, any>).__windowActions[n as string](...(a as unknown[])),
    [name, args] as const,
  );
}

test('the last Sessions tab shows no close button', async ({ page }) => {
  const sessionsTab = page.getByTestId('edge-tab-left-sessions-tab');
  const closeOf = (tab: typeof sessionsTab) => tab.getByRole('button', { name: 'Close tab' });
  await expect(closeOf(sessionsTab)).toHaveCount(0);

  // A second Sessions tab makes the first one closable, which proves that
  // the absence above comes from the rule and not from the locator.
  const group = (await groupOf(page, 'sessions-tab'))!;
  await windowAction(page, 'addTab', group, { id: 'sessions-tab-2', title: 'Sessions 2', contentType: 'sessions' });
  await windowAction(page, 'setActiveTab', group, 'sessions-tab');
  const second = page.getByTestId('edge-tab-left-sessions-tab-2');
  await expect(second).toBeVisible();
  await expect(closeOf(sessionsTab)).toHaveCount(1);
  await expect(closeOf(second)).toHaveCount(1);

  await windowAction(page, 'removeTab', group, 'sessions-tab-2');
  await expect(second).toHaveCount(0);
  await expect(closeOf(sessionsTab)).toHaveCount(0);
});

test('removeTab on the last Sessions tab does nothing', async ({ page }) => {
  const before = await leftTabIds(page);
  expect(before).toContain('sessions-tab');
  const group = (await groupOf(page, 'sessions-tab'))!;

  expect(await windowAction(page, 'canCloseTab', group, 'sessions-tab')).toBe(false);
  await windowAction(page, 'removeTab', group, 'sessions-tab');

  expect(await leftTabIds(page)).toEqual(before);
  await expect(page.getByTestId('edge-tab-left-sessions-tab')).toBeVisible();
  await expect(page.getByTestId('session-list')).toBeAttached();
});

test('the last Files tab does not close', async ({ page }) => {
  await windowAction(page, 'setEdgePanelCollapsed', 'right', false);
  const filesTab = page.getByTestId('edge-tab-right-files-tab');
  await expect(filesTab).toBeVisible();
  await expect(filesTab.getByRole('button', { name: 'Close tab' })).toHaveCount(0);

  const group = (await groupOf(page, 'files-tab'))!;
  expect(await windowAction(page, 'canCloseTab', group, 'files-tab')).toBe(false);
  await windowAction(page, 'removeTab', group, 'files-tab');
  expect(await groupOf(page, 'files-tab')).toBe(group);
  await expect(filesTab).toBeVisible();
});

test('the last Sessions tab may move out of its rail', async ({ page }) => {
  // The rule refuses a close, not a move. Moving the tab is how a user
  // empties the rail, and the rail then shows its collapsed strip.
  const leftGroup = (await groupOf(page, 'sessions-tab'))!;
  const centreGroup = await page.evaluate(() => {
    const store = (window as unknown as Record<string, any>).__windowStore;
    const first = (n: any): string | null =>
      n.type === 'pane' ? (n.tabGroupId ?? null) : (first(n.first) ?? first(n.second));
    return first(store.layout);
  });
  expect(centreGroup).not.toBeNull();

  await windowAction(page, 'moveTab', leftGroup, centreGroup, 'sessions-tab');

  await expect(page.getByTestId('edge-collapsed-drop-left')).toBeVisible();
  await expect(page.getByTestId('edge-tabbar-left')).not.toBeVisible();
  expect(await leftTabIds(page)).toEqual([]);
  expect(await groupOf(page, 'sessions-tab')).toBe(centreGroup);
  await expect(page.locator('[data-tab-id="sessions-tab"]:not([data-testid^="edge-tab-"])')).toBeVisible();
});

test('the layout menu re-adds a closed panel', async ({ page }) => {
  // Close Backlinks the way a user does: the close button on its tab.
  await windowAction(page, 'setEdgePanelCollapsed', 'right', false);
  const backlinksTab = page.getByTestId('edge-tab-right-backlinks-tab');
  await expect(backlinksTab).toBeVisible();
  await backlinksTab.hover();
  await backlinksTab.getByRole('button', { name: 'Close tab' }).click();
  await expect(backlinksTab).toHaveCount(0);
  expect(await groupOf(page, 'backlinks-tab')).toBeNull();

  await page.getByTestId('layout-menu').click();
  await page.getByTestId('layout-readd').click();
  const item = page.getByTestId('layout-readd-backlinks');
  await expect(item).toBeVisible();
  // An open panel is not offered.
  await expect(page.getByTestId('layout-readd-sessions')).toHaveCount(0);
  await expect(page.getByTestId('layout-readd-files')).toHaveCount(0);

  await item.click();

  // The panel opens in its registered zone, the right rail, which expands.
  await expect.poll(() => groupOf(page, 'tab-backlinks')).not.toBeNull();
  await expect(page.getByTestId('edge-tab-right-tab-backlinks')).toBeVisible();

  // Open again: the menu no longer offers it.
  await page.getByTestId('layout-menu').click();
  await page.getByTestId('layout-readd').click();
  await expect(page.getByTestId('layout-readd-popout')).toBeVisible();
  await expect(page.getByTestId('layout-readd-backlinks')).toHaveCount(0);
});
