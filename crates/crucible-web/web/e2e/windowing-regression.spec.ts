import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';

/**
 * E2E: Windowing System Regression Guard
 *
 * Verifies that the core windowing system (WindowManager, layout, edge panels)
 * remains unbroken. This test protects against accidental modifications to:
 * - windowStore.ts
 * - WindowManager.tsx
 * - SplitPane.tsx
 * - CenterTiling.tsx
 * - layout-serializer.ts
 * - layout-persistence.ts
 * - windowTypes.ts
 *
 * Mocked API throughout: the vite dev server proxies /api to whatever runs
 * on :3000, so an unmocked run against a live daemon imports a real saved
 * layout mid-test and races the interactions below.
 */

test.beforeEach(async ({ page }) => {
  await setupBasicMocks(page);
  // The pop-out/dock tests open a real file tab; serve its bytes.
  await page.route('**/api/kiln/file**', (route) => {
    const m = route.request().method();
    if (m === 'GET') return route.fulfill({ json: { content: '# note\n' } });
    if (m === 'PUT') return route.fulfill({ status: 200, body: '' });
    return route.continue();
  });
});

test('WindowManager renders with all layout regions', async ({ page }) => {
  await page.goto('/');

  // Root container: flex flex-col h-screen
  const rootContainer = page.locator('div.flex.flex-col.h-screen.bg-shell-bg');
  await expect(rootContainer).toBeVisible();

  // Ribbon with the command palette button (no header bar).
  await expect(page.getByTestId('ribbon-cmd-palette')).toBeVisible();

  // Main content area between the ribbons.
  const mainContent = page.locator('div.flex-1.flex.flex-col.overflow-hidden');
  await expect(mainContent).toBeVisible();
});

test('Left edge panel toggles open and closed via its ribbon', async ({ page }) => {
  await page.goto('/');

  const toggleButton = page.getByTestId('ribbon-toggle-left');
  await expect(toggleButton).toBeVisible();

  // Panel starts open — the toggle offers to collapse it.
  await expect(toggleButton).toHaveAttribute('title', 'Collapse panel');

  await toggleButton.click();
  await expect(toggleButton).toHaveAttribute('title', 'Expand panel');

  await toggleButton.click();
  await expect(toggleButton).toHaveAttribute('title', 'Collapse panel');
});

test('Ribbons carry the shell controls — no header bar', async ({ page }) => {
  await page.goto('/');

  // Left ribbon: palette, new session, settings gear.
  await expect(page.getByTestId('ribbon-cmd-palette')).toBeVisible();
  await expect(page.getByTestId('ribbon-cmd-new-session')).toBeVisible();
  await expect(page.getByTestId('ribbon-cmd-settings')).toBeVisible();

  // Every edge exposes its own toggle.
  await expect(page.getByTestId('ribbon-toggle-left')).toBeVisible();
  await expect(page.getByTestId('ribbon-toggle-right')).toBeVisible();
  await expect(page.getByTestId('ribbon-toggle-bottom')).toBeVisible();

  // The header bar is gone: its Inbox pill and Ctrl+P kbd hint (the ribbon's
  // palette button shares the palette title, so Inbox is the discriminator)
  // no longer exist anywhere.
  await expect(page.locator('button[title="Inbox"]')).toHaveCount(0);
  await expect(page.locator('kbd:has-text("Ctrl+P")')).toHaveCount(0);
});

test('Center tiling area is visible and interactive', async ({ page }) => {
  await page.goto('/');

  // The center tiling area should be visible
  const centerArea = page.locator('div.flex-1.flex.flex-col.overflow-hidden');
  await expect(centerArea).toBeVisible();

  // There should be at least one pane in the center area
  // (The exact structure depends on the layout, but there should be content)
  const content = page.locator('div.flex-1');
  const count = await content.count();
  expect(count).toBeGreaterThan(0);
});

test('Layout structure remains stable after interaction', async ({ page }) => {
  await page.goto('/');

  // Get initial structure
  const rootContainer = page.locator('div.flex.flex-col.h-screen.bg-shell-bg');
  await expect(rootContainer).toBeVisible();

  // Collapse the left panel
  const toggleButton = page.getByTestId('ribbon-toggle-left');
  await toggleButton.click();

  // Root container should still be visible and stable
  await expect(rootContainer).toBeVisible();

  // Ribbon should still be visible
  const ribbonPalette = page.getByTestId('ribbon-cmd-palette');
  await expect(ribbonPalette).toBeVisible();

  // Expand the left panel again
  await toggleButton.click();

  // Everything should still be visible
  await expect(rootContainer).toBeVisible();
  await expect(ribbonPalette).toBeVisible();
});

test('No critical console errors on initial load', async ({ page }) => {
  const errors: string[] = [];
  
  page.on('console', (msg) => {
    if (msg.type() === 'error') {
      // Filter out expected API errors (backend not running in test environment)
      const text = msg.text();
      if (!text.includes('Failed to load resource') && 
          !text.includes('ECONNREFUSED') &&
          !text.includes('http proxy error')) {
        errors.push(text);
      }
    }
  });

  await page.goto('/');
  
  // Wait a moment for any async operations
  await page.waitForLoadState('domcontentloaded');

  // There should be no critical console errors (excluding expected API errors)
  expect(errors).toEqual([]);
});

test('pop-out MOVES the tabs to a floating window (no mirrored group)', async ({ page }) => {
  // The DnD registry must stay coherent while the group moves between tab
  // bars — a "Cannot remove nonexistent draggable/droppable" warning means a
  // cleanup stole the new container's registration (tab silently undraggable).
  const dndWarnings: string[] = [];
  page.on('console', (msg) => {
    if (msg.text().includes('nonexistent')) dndWarnings.push(msg.text());
  });
  await page.goto('/');

  // Put a file tab in the center pane via the product open-file path.
  await page.evaluate(() => {
    window.dispatchEvent(
      new CustomEvent('crucible:open-file', {
        detail: { path: '/kiln/popout-note.md', name: 'popout-note.md' },
      }),
    );
  });
  await expect(page.locator('[data-tab-id^="tab-file-"]')).toHaveCount(1);

  // Pop the pane out.
  await page.locator('button[title="Pop out to floating window"]').first().click();

  // A floating window appears, titled after the tab...
  const floating = page.locator('div.absolute.flex.flex-col').filter({ hasText: 'popout-note.md' });
  await expect(floating.first()).toBeVisible();

  // ...and the tab exists exactly ONCE across all tab strips. The old
  // pop-out shared the group between pane and window: two tab strips, two
  // solid-dnd registrations under one id (drag silently broke).
  await expect(page.locator('[data-tab-id^="tab-file-"]')).toHaveCount(1);

  // Closing the floating window closes its tabs with it — nothing orphaned.
  await page.locator('button[title="Close (closes its tabs)"]').click();
  await expect(page.locator('[data-tab-id^="tab-file-"]')).toHaveCount(0);
  expect(dndWarnings).toEqual([]);
});

test('dock button moves a floating window back into the layout', async ({ page }) => {
  await page.goto('/');

  await page.evaluate(() => {
    window.dispatchEvent(
      new CustomEvent('crucible:open-file', {
        detail: { path: '/kiln/dock-note.md', name: 'dock-note.md' },
      }),
    );
  });
  await expect(page.locator('[data-tab-id^="tab-file-"]')).toHaveCount(1);

  await page.locator('button[title="Pop out to floating window"]').first().click();
  await expect(page.locator('button[title="Dock back into the layout"]')).toBeVisible();

  await page.locator('button[title="Dock back into the layout"]').click();

  // The floating window is gone and the tab is back in a pane, still unique.
  await expect(page.locator('button[title="Dock back into the layout"]')).toHaveCount(0);
  await expect(page.locator('[data-tab-id^="tab-file-"]')).toHaveCount(1);
});
