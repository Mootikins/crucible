import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';


type PanelPosition = 'left' | 'right' | 'bottom';

async function waitForApp(page: Page) {
  await setupBasicMocks(page);
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

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/**
 * Pointer-based drag: mousedown -> mousemove(steps) -> mouseup.
 * Required because @thisbeyond/solid-dnd uses pointer events, not HTML5 DnD.
 */
async function pointerDrag(
  page: Page,
  from: { x: number; y: number },
  to: { x: number; y: number },
  steps = 10,
) {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps });
  await page.mouse.up();
}

async function getCenter(page: Page, selector: string) {
  const loc = page.locator(selector);
  await loc.waitFor({ state: 'visible', timeout: 3000 });
  const box = await loc.boundingBox();
  expect(box).toBeTruthy();
  return { x: box!.x + box!.width / 2, y: box!.y + box!.height / 2 };
}

async function getCenterOf(page: Page, locator: ReturnType<Page['locator']>) {
  await locator.waitFor({ state: 'visible', timeout: 3000 });
  const box = await locator.boundingBox();
  expect(box).toBeTruthy();
  return { x: box!.x + box!.width / 2, y: box!.y + box!.height / 2 };
}

async function getCenterPaneDropPoint(page: Page): Promise<{ x: number; y: number }> {
  const chatTab = page.locator('[data-tab-id^="tab-chat-"]').first();
  await chatTab.waitFor({ state: 'visible', timeout: 3000 });
  const box = await chatTab.boundingBox();
  expect(box).toBeTruthy();
  return { x: box!.x + box!.width / 2, y: box!.y + box!.height + 40 };
}

async function ensureBottomPanelExpanded(page: Page) {
  const collapsedStripBottom = page.locator('[data-testid="edge-collapsed-drop-bottom"]');
  if (await collapsedStripBottom.isVisible()) {
    await collapsedStripBottom.locator('button[title="Expand panel"]').click();
    await page.waitForTimeout(300);
  }
}

test.describe('Cross-zone tab drag and drop', () => {
  test.beforeEach(async ({ page }) => {
    await waitForApp(page);
    // Open a session to create a chat tab in the center pane
    const sessionItem = page.getByTestId('session-item-test-session-001');
    await expect(sessionItem).toBeVisible({ timeout: 5000 });
    await sessionItem.click();
    // Wait for the chat tab to appear
    await expect(page.locator('[data-tab-id^="tab-chat-"]')).toBeVisible({ timeout: 5000 });
  });

  test('drag edge tab from expanded left panel to center pane', async ({ page }) => {
    const from = await getCenter(page, '[data-testid="edge-tab-left-explorer-tab"]');
    const to = await getCenterPaneDropPoint(page);

    await pointerDrag(page, from, to, 50);
    await page.waitForTimeout(1000);

    await expect(page.locator('[data-testid="edge-tab-left-explorer-tab"]')).not.toBeVisible({ timeout: 2000 });
    await expect(page.locator('[data-tab-id="explorer-tab"]:not([data-testid^="edge-tab-"])')).toBeVisible({ timeout: 2000 });
  });

  test('drag center tab to left edge panel', async ({ page }) => {
    const centerTab = page.locator('[data-tab-id^="tab-chat-"]').first();
    const from = await getCenterOf(page, centerTab);
    const to = await getCenter(page, '[data-testid="edge-tabbar-left"]');

    await pointerDrag(page, from, to, 30);
    await page.waitForTimeout(300);

    const edgeTab = page.locator('[data-testid^="edge-tab-left-tab-chat-"]');
    await expect(edgeTab).toBeVisible({ timeout: 2000 });
    const tabInCenter = page.locator('[data-tab-id^="tab-chat-"]:not([data-testid^="edge-tab-"])');
    await expect(tabInCenter).not.toBeVisible({ timeout: 2000 });
  });

  test('drag edge tab from left panel to bottom panel', async ({ page }) => {
    await ensureBottomPanelExpanded(page);
    const from = await getCenter(page, '[data-testid="edge-tab-left-explorer-tab"]');
    const to = await getCenter(page, '[data-testid="edge-tabbar-bottom"]');

    await pointerDrag(page, from, to);
    await page.waitForTimeout(300);

    await expect(page.locator('[data-testid="edge-tab-left-explorer-tab"]')).not.toBeVisible({ timeout: 2000 });
    await expect(page.locator('[data-testid="edge-tab-bottom-explorer-tab"]')).toBeVisible({ timeout: 2000 });
  });

  test('dragging last tab out of edge panel auto-collapses it', async ({ page }) => {
    await page.evaluate(() => {
      const windowStore = (window as unknown as Record<string, unknown>).__windowStore as {
        layout: { type?: string; tabGroupId?: string; first?: unknown; second?: unknown };
        edgePanels: { left: { tabGroupId: string } };
      };
      const windowActions = (window as unknown as Record<string, unknown>).__windowActions as {
        moveTab: (from: string, to: string, tabId: string) => void;
      };
      const leftGroupId = windowStore.edgePanels.left.tabGroupId;

      const findFirstPaneGroupId = (node: unknown): string | null => {
        if (!node || typeof node !== 'object') return null;
        const typedNode = node as {
          type?: string;
          tabGroupId?: string;
          first?: unknown;
          second?: unknown;
        };
        if (typedNode.type === 'pane') return typedNode.tabGroupId ?? null;
        return findFirstPaneGroupId(typedNode.first) ?? findFirstPaneGroupId(typedNode.second);
      };

      const centerGroupId = findFirstPaneGroupId(windowStore.layout);
      if (!centerGroupId) return;

      for (const tabId of ['explorer-tab', 'search-tab', 'source-control-tab']) {
        windowActions.moveTab(leftGroupId, centerGroupId, tabId);
      }
    });
    await page.waitForTimeout(300);

    const centerTarget = await getCenterPaneDropPoint(page);

    const leftTabBar = page.locator('[data-testid="edge-tabbar-left"]');
    await expect(leftTabBar).toBeVisible({ timeout: 2000 });

    await pointerDrag(page, await getCenter(page, '[data-testid="edge-tab-left-sessions-tab"]'), centerTarget, 50);
    await page.waitForTimeout(500);

    // Then: left panel auto-collapses
    await expect(leftTabBar).not.toBeVisible({ timeout: 2000 });
  });

  test('drag center tab onto collapsed right panel expands it', async ({ page }) => {
    // Given: right panel starts collapsed
    const from = await getCenterOf(page, page.locator('[data-tab-id^="tab-chat-"]').first());
    const to = await getCenter(page, '[data-testid="edge-collapsed-drop-right"]');

    // When: drag center tab onto collapsed strip
    await pointerDrag(page, from, to);
    await page.waitForTimeout(300);

    // Then: right panel expands with the dropped tab
    await expect(page.locator('[data-testid="edge-tabbar-right"]')).toBeVisible({ timeout: 2000 });
    await expect(page.locator('[data-testid^="edge-tab-right-tab-chat-"]')).toBeVisible({ timeout: 2000 });
  });

  test('edge tab drag creates drag overlay with tab title', async ({ page }) => {
    const from = await getCenter(page, '[data-testid="edge-tab-left-explorer-tab"]');

    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(from.x + 30, from.y + 30, { steps: 5 });
    await page.waitForTimeout(200);

    const overlay = page.locator('text="Explorer"').last();
    expect(await overlay.isVisible()).toBe(true);

    await page.mouse.up();
    await page.waitForTimeout(200);

    await expect(page.locator('[data-testid="edge-tab-left-explorer-tab"]')).toBeVisible({ timeout: 2000 });
  });

  test('drag edge tab from bottom panel to center pane tab group', async ({ page }) => {
    // Bottom panel starts collapsed in new layout - expand it first
    await ensureBottomPanelExpanded(page);
    const from = await getCenter(page, '[data-testid="edge-tab-bottom-problems-tab"]');
    const to = await getCenterOf(page, page.locator('[data-tab-id^="tab-chat-"]').first());

    await pointerDrag(page, from, to);
    await page.waitForTimeout(300);

    await expect(page.locator('[data-testid="edge-tab-bottom-problems-tab"]')).not.toBeVisible({ timeout: 2000 });
    await expect(page.locator('[data-tab-id="problems-tab"]')).toBeVisible({ timeout: 2000 });
  });
});
