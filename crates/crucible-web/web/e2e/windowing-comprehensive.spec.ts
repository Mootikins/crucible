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
  await page.waitForTimeout(500);
}

test.describe('Comprehensive windowing behavior', () => {
  test.beforeEach(async ({ page }) => {
    await waitForApp(page);
    const sessionItem = page.getByTestId('session-item-test-session-001');
    await expect(sessionItem).toBeVisible({ timeout: 5000 });
    await sessionItem.click();
    await expect(page.locator('[data-tab-id^="tab-chat-"]')).toBeVisible({ timeout: 5000 });
  });

  test('creates a vertical split with row splitter semantics', async ({ page }) => {
    await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      const windowActions = (window as unknown as Record<string, unknown>).__windowActions as WindowActionsShape;
      if (windowStore.layout.type === 'pane') {
        windowActions.splitPane(windowStore.layout.id, 'vertical');
      }
    });
    await page.waitForTimeout(250);

    const splitter = page.locator('[data-split-id]').first();
    await expect(splitter).toBeVisible({ timeout: 3000 });
    await expect(splitter).toHaveClass(/cursor-row-resize/, { timeout: 3000 });

    const state = await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      return {
        type: windowStore.layout.type,
        direction: windowStore.layout.direction,
        splitRatio: windowStore.layout.splitRatio,
      };
    });
    expect(state.type).toBe('split');
    expect(state.direction).toBe('vertical');
    expect(state.splitRatio).toBe(0.5);
  });

  test('supports nested splits by splitting a child pane after initial split', async ({ page }) => {
    const nested = await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      const windowActions = (window as unknown as Record<string, unknown>).__windowActions as WindowActionsShape;

      if (windowStore.layout.type !== 'pane') return false;
      windowActions.splitPane(windowStore.layout.id, 'horizontal');

      const afterFirst = windowStore.layout;
      if (afterFirst.type !== 'split' || !afterFirst.first || afterFirst.first.type !== 'pane') return false;
      windowActions.splitPane(afterFirst.first.id, 'vertical');

      const root = windowStore.layout;
      return root.type === 'split' && root.first?.type === 'split';
    });

    await page.waitForTimeout(300);
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
    await page.waitForTimeout(250);

    await expect(page.locator('[data-testid="edge-collapsed-drop-left"]')).toBeVisible({ timeout: 3000 });
    await expect(page.locator('[data-testid="edge-tabbar-left"]')).not.toBeVisible({ timeout: 3000 });
    await expect(page.locator('[data-testid="edge-collapsed-drop-left"] [data-testid="collapsed-tab-button-left"]')).toHaveCount(0, { timeout: 3000 });
    await expect(page.locator('[data-testid="edge-collapsed-drop-left"] button[title="Expand panel"]')).toBeVisible({ timeout: 3000 });
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
    await page.waitForTimeout(200);

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
    await page.waitForTimeout(250);

    const ratio = await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      return windowStore.layout.type === 'split' ? windowStore.layout.splitRatio ?? 0.5 : 0.5;
    });
    expect(ratio).toBeGreaterThan(0.5);
    expect(Math.abs(ratio - 0.5)).toBeGreaterThan(0.02);
  });

  test('shows center empty state after all center tabs are removed', async ({ page }) => {
    await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as WindowStoreShape;
      const windowActions = (window as unknown as Record<string, unknown>).__windowActions as WindowActionsShape;

      const findFirstPaneGroupId = (node: LayoutNode): string | null => {
        if (node.type === 'pane') return node.tabGroupId ?? null;
        return (node.first ? findFirstPaneGroupId(node.first) : null) ?? (node.second ? findFirstPaneGroupId(node.second) : null);
      };

      const centerGroupId = findFirstPaneGroupId(windowStore.layout);
      if (!centerGroupId) return;

      const tabs = [...(windowStore.tabGroups[centerGroupId]?.tabs ?? [])];
      for (const tab of tabs) {
        windowActions.removeTab(centerGroupId, tab.id);
      }
    });

    await expect(page.locator('text=No session open')).toBeVisible({ timeout: 3000 });
    await expect(page.locator('text=Select a session from the left panel or create a new one to get started.')).toBeVisible({ timeout: 3000 });
  });
});
