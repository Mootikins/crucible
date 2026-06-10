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

  // Close all tabs in the center pane by clicking close buttons
  // The close button is a button inside a tab item (div with data-tab-id)
  let tabItems = page.locator('[data-tab-id]');
  let count = await tabItems.count();
  
  // Close all tabs
  while (count > 0) {
    // Find the first tab and click its close button
    const firstTab = tabItems.first();
    const closeButton = firstTab.locator('button').last(); // The close button is the last button in the tab
    if (await closeButton.isVisible()) {
      await closeButton.click();
      await page.waitForTimeout(100);
    }
    count = await tabItems.count();
  }

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
