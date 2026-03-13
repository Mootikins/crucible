import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION, MOCK_SESSION_2 } from './helpers/fixtures';

/**
 * E2E: New Session Button Position
 *
 * Verifies that the "New Session" button appears ABOVE session items in the list,
 * not below them.
 */

test.describe('New Session Button Position', () => {
  test('New Session button appears BEFORE session items in the list', async ({ page }) => {
    // Set up mocks with multiple sessions to ensure button is visually above them
    await setupBasicMocks(page, {
      sessions: [MOCK_SESSION, MOCK_SESSION_2],
    });

    await page.goto('/');

    // Wait for the session list to be visible
    const sessionList = page.getByTestId('session-list');
    await expect(sessionList).toBeVisible({ timeout: 10000 });

    // Get the New Session button
    const newSessionBtn = page.getByTestId('new-session-button');
    await expect(newSessionBtn).toBeVisible({ timeout: 5000 });

    // Get the first session item
    const firstSessionItem = page.getByTestId('session-item-test-session-001');
    await expect(firstSessionItem).toBeVisible({ timeout: 5000 });

    // Get bounding boxes
    const buttonBox = await newSessionBtn.boundingBox();
    const firstItemBox = await firstSessionItem.boundingBox();

    // Assert: button's Y position must be LESS than first session item's Y position
    // (button appears above = smaller Y coordinate)
    expect(buttonBox).toBeTruthy();
    expect(firstItemBox).toBeTruthy();

    if (buttonBox && firstItemBox) {
      expect(buttonBox.y).toBeLessThan(firstItemBox.y);
    }
  });
});
