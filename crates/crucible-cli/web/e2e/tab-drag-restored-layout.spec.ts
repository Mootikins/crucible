import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';

// Regression: after a server layout restore (GET /api/layout), pane ids in
// the store are replaced under the already-mounted Pane components. Their
// solid-dnd droppables were registered with the boot-time pane id, so every
// pane drop (split zones AND center) carried a stale id and silently
// no-opped — "tab dragging for paned flows doesn't work at all". The restore
// is delayed below so it lands after first render, exactly like production.

const RESTORED_LAYOUT = JSON.stringify({
  version: 2,
  layout: { id: 'restored-pane-1', type: 'pane', tabGroupId: 'restored-group-1' },
  tabGroups: {
    'restored-group-1': {
      id: 'restored-group-1',
      tabs: [
        { id: 'tab-home', title: 'Home', contentType: 'home' },
        { id: 'tab-inbox', title: 'Inbox', contentType: 'inbox' },
      ],
      activeTabId: 'tab-inbox',
    },
  },
  edgePanels: {
    left: { id: 'left-panel', tabGroupId: 'left-group', isCollapsed: true, width: 280 },
    right: { id: 'right-panel', tabGroupId: 'right-group', isCollapsed: true, width: 250 },
    bottom: { id: 'bottom-panel', tabGroupId: 'bottom-group', isCollapsed: true, height: 200 },
  },
  floatingWindows: [],
});

async function openRestoredApp(page: Page) {
  await setupBasicMocks(page);
  await page.route('**/api/layout', async (route) => {
    if (route.request().method() === 'GET') {
      // Land the restore after first render, as in production.
      await new Promise((r) => setTimeout(r, 500));
      await route.fulfill({ status: 200, contentType: 'application/json', body: RESTORED_LAYOUT });
      return;
    }
    await route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
  });
  await page.goto('/');
  // tab-inbox only exists in the restored layout — visibility means the
  // restore has been applied over the boot-time default.
  await expect(page.locator('[data-tab-id="tab-inbox"]')).toBeVisible({ timeout: 10_000 });
}

/** Drag with a condition wait on `highlight` (the active drop indicator) before releasing. */
async function pointerDragUntil(
  page: Page,
  from: { x: number; y: number },
  to: { x: number; y: number },
  highlight: string,
) {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 15 });
  await expect(page.locator(highlight).first()).toBeVisible({ timeout: 3000 });
  await page.mouse.up();
}

test('pane split by drag works after a delayed layout restore', async ({ page }) => {
  await openRestoredApp(page);

  const inboxTab = page.locator('[data-tab-id="tab-inbox"]');
  await expect(page.locator('[data-testid="resize-splitter"]')).toHaveCount(0);

  const box = (await inboxTab.boundingBox())!;
  const viewport = page.viewportSize()!;
  // Right fifth of the center pane, clear of the collapsed right edge rail.
  await pointerDragUntil(
    page,
    { x: box.x + box.width / 2, y: box.y + box.height / 2 },
    { x: Math.floor(viewport.width * 0.85), y: Math.floor(viewport.height / 2) },
    '[class*="bg-primary/30"]',
  );

  // A real split creates a resize splitter between the two panes — asserting
  // on DOM ancestry is a false positive (edge-panel moves also change it).
  await expect(page.locator('[data-testid="resize-splitter"]')).toHaveCount(1);
});

test('drop onto the tab bar of a restored group still moves tabs', async ({ page }) => {
  await openRestoredApp(page);

  // Split first (works after the fix), then drag Inbox back onto the first
  // group's tab bar — exercises the restored `tabgroup:` droppable.
  const inboxTab = page.locator('[data-tab-id="tab-inbox"]');
  const viewport = page.viewportSize()!;
  let box = (await inboxTab.boundingBox())!;
  await pointerDragUntil(
    page,
    { x: box.x + box.width / 2, y: box.y + box.height / 2 },
    { x: Math.floor(viewport.width * 0.85), y: Math.floor(viewport.height / 2) },
    '[class*="bg-primary/30"]',
  );
  await expect(page.locator('[data-testid="resize-splitter"]')).toHaveCount(1);

  const homeTab = page.locator('[data-tab-id="tab-home"]');
  const homeBox = (await homeTab.boundingBox())!;
  box = (await inboxTab.boundingBox())!;
  await pointerDragUntil(
    page,
    { x: box.x + box.width / 2, y: box.y + box.height / 2 },
    { x: homeBox.x + homeBox.width + 40, y: homeBox.y + homeBox.height / 2 },
    // CenterTabBar's active-drop underline indicator.
    '[class*="h-0.5"][class*="bg-primary"]',
  );

  // Tab moved back into the first group; the empty pane is pruned.
  await expect(page.locator('[data-testid="resize-splitter"]')).toHaveCount(0);
});
