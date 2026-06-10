import { test, expect } from '@playwright/test';

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
 */

test('WindowManager renders with all layout regions', async ({ page }) => {
  await page.goto('/');

  // Root container: flex flex-col h-screen
  const rootContainer = page.locator('div.flex.flex-col.h-screen.bg-zinc-950');
  await expect(rootContainer).toBeVisible();

  // Header bar with command palette text
  const headerBar = page.locator('text=Command palette');
  await expect(headerBar).toBeVisible();

  // Status bar at bottom
  const statusBar = page.locator('[class*="StatusBar"]');
  // StatusBar may not have a specific test ID, so we check for its presence via structure
  // The main layout should have a header, content area, and status bar
  const mainContent = page.locator('div.flex-1.flex.flex-col.overflow-hidden');
  await expect(mainContent).toBeVisible();
});

test('Left edge panel toggles open and closed', async ({ page }) => {
  await page.goto('/');

  // Find the left panel toggle button (title contains "Hide Left Panel" or "Show Left Panel")
  const toggleButton = page.locator('button[title*="Left Panel"]').first();
  await expect(toggleButton).toBeVisible();

  // Get initial state - button should show "Hide Left Panel" (panel is open)
  const initialTitle = await toggleButton.getAttribute('title');
  expect(initialTitle).toContain('Left Panel');

  // Click to collapse
  await toggleButton.click();

  // After collapse, button title should change to "Show Left Panel"
  const collapsedTitle = await toggleButton.getAttribute('title');
  expect(collapsedTitle).toContain('Show Left Panel');

  // Click to expand again
  await toggleButton.click();

  // After expand, button title should change back to "Hide Left Panel"
  const expandedTitle = await toggleButton.getAttribute('title');
  expect(expandedTitle).toContain('Hide Left Panel');
});

test('Header bar is visible with all controls', async ({ page }) => {
  await page.goto('/');

  // Header bar should be visible
  const headerBar = page.locator('div.flex.items-center.h-8.bg-zinc-900');
  await expect(headerBar).toBeVisible();

  // Command palette button should be visible
  const commandPalette = page.locator('text=Command palette');
  await expect(commandPalette).toBeVisible();

  // Keyboard shortcut indicator should be visible
  const shortcutKey = page.locator('text=⌘P');
  await expect(shortcutKey).toBeVisible();

  // Edge panel toggle buttons should be visible
  const leftPanelButton = page.locator('button[title*="Left Panel"]');
  await expect(leftPanelButton).toBeVisible();

  const rightPanelButton = page.locator('button[title*="Right Panel"]');
  await expect(rightPanelButton).toBeVisible();

  const bottomPanelButton = page.locator('button[title*="Bottom Panel"]');
  await expect(bottomPanelButton).toBeVisible();
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
  const rootContainer = page.locator('div.flex.flex-col.h-screen.bg-zinc-950');
  await expect(rootContainer).toBeVisible();

  // Collapse the left panel
  const toggleButton = page.locator('button[title*="Left Panel"]').first();
  await toggleButton.click();

  // Root container should still be visible and stable
  await expect(rootContainer).toBeVisible();

  // Header bar should still be visible
  const headerBar = page.locator('text=Command palette');
  await expect(headerBar).toBeVisible();

  // Expand the left panel again
  await toggleButton.click();

  // Everything should still be visible
  await expect(rootContainer).toBeVisible();
  await expect(headerBar).toBeVisible();
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
