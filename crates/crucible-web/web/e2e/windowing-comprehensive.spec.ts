import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION, MOCK_SESSION_2 } from './helpers/fixtures';

type LayoutNode = {
  type: 'pane' | 'split';
  id: string;
  tabGroupId?: string | null;
  direction?: 'horizontal' | 'vertical';
  splitRatio?: number;
  first?: LayoutNode;
  second?: LayoutNode;
};

type WindowStoreShape = {
  layout: LayoutNode;
  tabGroups: Record<string, { tabs: Array<{ id: string }>; activeTabId: string | null }>;
  edgePanels: Record<'left' | 'right' | 'bottom', { tabGroupId: string; isCollapsed: boolean }>;
  floatingWindows: Array<{ id: string; x: number; y: number; width: number; height: number }>;
};

type WindowActionsShape = {
  splitPane: (paneId: string, direction: 'horizontal' | 'vertical') => void;
  removeTab: (groupId: string, tabId: string) => void;
  setEdgePanelCollapsed: (position: 'left' | 'right' | 'bottom', collapsed: boolean) => void;
  createFloatingWindow: (groupId: string, x: number, y: number, width?: number, height?: number) => string;
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
  // Readiness is asserted by the beforeEach (session-item visible), which only
  // renders once the app has fully mounted — no fixed settle needed.
}

test.describe('Comprehensive windowing behavior', () => {
  test.beforeEach(async ({ page }) => {
    await waitForApp(page);
    const sessionItem = page.getByTestId('session-item-test-session-001');
    await expect(sessionItem).toBeVisible({ timeout: 5000 });
    await sessionItem.click();
    await expect(page.locator('[data-tab-id^="tab-chat-"]')).toBeVisible({ timeout: 5000 });
  });

  // The beforeEach clicks a session, which opens the chat tab in the RIGHT
  // pane (see src/lib/session-actions.ts) — the layout root is ALREADY a
  // horizontal split (session column | chat pane) by the time these tests
  // start, so the old `layout.type === 'pane'` guard is always false now.
  // These tests split the LEFT pane (root.first, the original session/file
  // pane) instead, and assert against the resulting nested shape.

  test('creates a vertical split with row splitter semantics', async ({ page }) => {
    await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      const windowActions = (window as unknown as Record<string, unknown>).__windowActions as WindowActionsShape;
      const root = windowStore.layout;
      if (root.type === 'split' && root.first.type === 'pane') {
        windowActions.splitPane(root.first.id, 'vertical');
      }
    });

    // The root's own splitter (session column | chat pane) is a horizontal
    // split, i.e. cursor-col-resize — filter by class instead of `.first()`
    // to find the NEW row splitter produced by the vertical split above.
    const rowSplitter = page.locator('[data-split-id].cursor-row-resize');
    await expect(rowSplitter).toBeVisible({ timeout: 3000 });

    const state = await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      const root = windowStore.layout;
      const first = root.type === 'split' ? root.first : undefined;
      return {
        rootType: root.type,
        rootDirection: root.direction,
        firstType: first?.type,
        firstDirection: first?.type === 'split' ? first.direction : undefined,
        firstSplitRatio: first?.type === 'split' ? first.splitRatio : undefined,
      };
    });
    expect(state.rootType).toBe('split');
    expect(state.rootDirection).toBe('horizontal');
    expect(state.firstType).toBe('split');
    expect(state.firstDirection).toBe('vertical');
    expect(state.firstSplitRatio).toBe(0.5);
  });

  test('supports nested splits by splitting a child pane after initial split', async ({ page }) => {
    const nested = await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      const windowActions = (window as unknown as Record<string, unknown>).__windowActions as WindowActionsShape;

      // The "initial split" is the session column | chat pane split already
      // produced by the beforeEach. Splitting the left (session) pane again
      // nests a second split under it.
      const root = windowStore.layout;
      if (root.type !== 'split' || root.first.type !== 'pane') return false;
      windowActions.splitPane(root.first.id, 'vertical');

      const after = windowStore.layout;
      return after.type === 'split' && after.first?.type === 'split';
    });

    // One splitter already exists (the root session|chat divider); the
    // nested split above adds a second.
    await expect(page.locator('[data-split-id]')).toHaveCount(2, { timeout: 3000 });
    expect(nested).toBe(true);

    const directions = await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      const out: string[] = [];
      const visit = (node: LayoutNode) => {
        if (node.type === 'split') {
          out.push(node.direction ?? '');
          if (node.first) visit(node.first);
          if (node.second) visit(node.second);
        }
      };
      visit(windowStore.layout);
      return out;
    });
    expect(directions).toContain('horizontal');
    expect(directions).toContain('vertical');
  });

  test('collapses and re-expands left edge panel via store action', async ({ page }) => {
    await expect(page.locator('[data-testid="edge-tabbar-left"]')).toBeVisible({ timeout: 3000 });

    await page.evaluate(() => {
      const windowActions = (window as unknown as Record<string, unknown>).__windowActions as WindowActionsShape;
      windowActions.setEdgePanelCollapsed('left', true);
    });
    await expect(page.locator('[data-testid="edge-collapsed-drop-left"]')).toBeVisible({ timeout: 3000 });
    await expect(page.locator('[data-testid="edge-tabbar-left"]')).not.toBeVisible({ timeout: 3000 });

    await page.evaluate(() => {
      const windowActions = (window as unknown as Record<string, unknown>).__windowActions as WindowActionsShape;
      windowActions.setEdgePanelCollapsed('left', false);
    });
    await expect(page.locator('[data-testid="edge-tabbar-left"]')).toBeVisible({ timeout: 3000 });
  });

  test('shows valid collapsed strip state when edge panel has no tabs', async ({ page }) => {
    await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      const windowActions = (window as unknown as Record<string, unknown>).__windowActions as WindowActionsShape;
      const leftGroupId = windowStore.edgePanels.left.tabGroupId;
      const tabs = [...(windowStore.tabGroups[leftGroupId]?.tabs ?? [])];
      for (const tab of tabs) {
        windowActions.removeTab(leftGroupId, tab.id);
      }
    });

    await expect(page.locator('[data-testid="edge-collapsed-drop-left"]')).toBeVisible({ timeout: 3000 });
    await expect(page.locator('[data-testid="edge-tabbar-left"]')).not.toBeVisible({ timeout: 3000 });
    // The ribbon persists but has nothing to show for an empty panel —
    // no tab icons, and no expand affordance (icons ARE the toggles now).
    await expect(page.locator('[data-testid="edge-collapsed-drop-left"] [data-testid="collapsed-tab-button-left"]')).toHaveCount(0, { timeout: 3000 });
  });

  test('creates a floating window at requested position', async ({ page }) => {
    const result = await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      const windowActions = (window as unknown as Record<string, unknown>).__windowActions as WindowActionsShape;

      const findFirstPaneGroupId = (node: LayoutNode): string | null => {
        if (node.type === 'pane') return node.tabGroupId ?? null;
        return (node.first ? findFirstPaneGroupId(node.first) : null) ?? (node.second ? findFirstPaneGroupId(node.second) : null);
      };

      const centerGroupId = findFirstPaneGroupId(windowStore.layout);
      if (!centerGroupId) return { ok: false, count: 0, x: -1, y: -1 };

      windowActions.createFloatingWindow(centerGroupId, 320, 180, 420, 260);
      const floating = windowStore.floatingWindows[windowStore.floatingWindows.length - 1];
      return {
        ok: true,
        count: windowStore.floatingWindows.length,
        x: floating?.x ?? -1,
        y: floating?.y ?? -1,
      };
    });

    expect(result.ok).toBe(true);
    expect(result.count).toBeGreaterThan(0);
    expect(result.x).toBe(320);
    expect(result.y).toBe(180);
    await expect(page.locator('div[style*="left: 320px"][style*="top: 180px"]')).toBeVisible({ timeout: 3000 });
  });

  test('clicking tabs updates active tab in the center group', async ({ page }) => {
    const secondSessionItem = page.getByTestId('session-item-test-session-002');
    await expect(secondSessionItem).toBeVisible({ timeout: 5000 });
    await secondSessionItem.click();

    const firstTab = page.locator('[data-tab-id="tab-chat-test-session-001"]');
    const secondTab = page.locator('[data-tab-id="tab-chat-test-session-002"]');
    await expect(firstTab).toBeVisible({ timeout: 3000 });
    await expect(secondTab).toBeVisible({ timeout: 3000 });

    await secondTab.click();
    const activeAfterSecondClick = await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      const group = Object.values(windowStore.tabGroups).find((g) => g.tabs.some((t) => t.id === 'tab-chat-test-session-002'));
      return group?.activeTabId ?? null;
    });
    expect(activeAfterSecondClick).toBe('tab-chat-test-session-002');

    await firstTab.click();
    const activeAfterFirstClick = await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      const group = Object.values(windowStore.tabGroups).find((g) => g.tabs.some((t) => t.id === 'tab-chat-test-session-001'));
      return group?.activeTabId ?? null;
    });
    expect(activeAfterFirstClick).toBe('tab-chat-test-session-001');
  });

  test('closing a tab via store action updates center tab DOM', async ({ page }) => {
    await page.getByTestId('session-item-test-session-002').click();
    await expect(page.locator('[data-tab-id^="tab-chat-"]')).toHaveCount(2, { timeout: 3000 });

    await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      const windowActions = (window as unknown as Record<string, unknown>).__windowActions as WindowActionsShape;
      const groupEntry = Object.entries(windowStore.tabGroups).find(([, group]) =>
        group.tabs.some((t) => t.id === 'tab-chat-test-session-002')
      );
      if (groupEntry) {
        windowActions.removeTab(groupEntry[0], 'tab-chat-test-session-002');
      }
    });

    await expect(page.locator('[data-tab-id="tab-chat-test-session-002"]')).not.toBeVisible({ timeout: 3000 });
    await expect(page.locator('[data-tab-id^="tab-chat-"]')).toHaveCount(1, { timeout: 3000 });
  });

  test('opening multiple sessions creates two unique chat tabs without duplicates', async ({ page }) => {
    await page.getByTestId('session-item-test-session-002').click();
    await expect(page.locator('[data-tab-id="tab-chat-test-session-001"]')).toBeVisible({ timeout: 3000 });
    await expect(page.locator('[data-tab-id="tab-chat-test-session-002"]')).toBeVisible({ timeout: 3000 });

    await page.getByTestId('session-item-test-session-001').click();
    await expect(page.locator('[data-tab-id^="tab-chat-"]')).toHaveCount(2, { timeout: 3000 });
  });

  test('split ratio persists after dragging splitter away from default', async ({ page }) => {
    await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      const windowActions = (window as unknown as Record<string, unknown>).__windowActions as WindowActionsShape;
      if (windowStore.layout.type === 'pane') {
        windowActions.splitPane(windowStore.layout.id, 'horizontal');
      }
    });

    const splitter = page.locator('[data-split-id]').first();
    await splitter.waitFor({ state: 'visible', timeout: 3000 });
    const box = await splitter.boundingBox();
    expect(box).toBeTruthy();

    const startX = box!.x + box!.width / 2;
    const startY = box!.y + box!.height / 2;
    await page.mouse.move(startX, startY);
    await page.mouse.down();
    await page.mouse.move(startX + 110, startY, { steps: 8 });
    await page.mouse.up();

    // Poll the store until the drag-updated split ratio settles past the default.
    const readRatio = () =>
      page.evaluate(() => {
        const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
        return windowStore.layout.type === 'split' ? windowStore.layout.splitRatio ?? 0.5 : 0.5;
      });
    await expect.poll(readRatio, { timeout: 3000 }).toBeGreaterThan(0.5);

    const ratio = await readRatio();
    expect(Math.abs(ratio - 0.5)).toBeGreaterThan(0.02);
  });

  test('shows center empty state after all center tabs are removed', async ({ page }) => {
    // The beforeEach's session click puts a chat tab in its OWN right pane,
    // so there are now two center-pane groups (left "Home" pane + right
    // "chat" pane). Closing a pane's last tab collapses that (now-empty)
    // pane out of the layout tree entirely (see removeTab/collapseEmptyNodes
    // in src/stores/tabActions.ts + windowStoreInternals.ts) — so emptying
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

    await expect(page.locator('text=No session open')).toBeVisible({ timeout: 3000 });
    await expect(page.locator('text=Select a session from the left panel or create a new one to get started.')).toBeVisible({ timeout: 3000 });
  });
});
