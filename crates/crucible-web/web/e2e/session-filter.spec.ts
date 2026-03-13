import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION } from './helpers/fixtures';

test.describe('Session Filter Dropdown', () => {
  test.beforeEach(async ({ page }) => {
    await setupBasicMocks(page, { sessions: [MOCK_SESSION] });
    await page.goto('/');
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });
  });

  test('filter dropdown renders with correct options', async ({ page }) => {
    const dropdown = page.getByTestId('session-filter-dropdown');
    await expect(dropdown).toBeVisible();
    await expect(dropdown.locator('option[value="active"]')).toHaveText('Active');
    await expect(dropdown.locator('option[value="all"]')).toHaveText('All');
    await expect(dropdown.locator('option[value="archived"]')).toHaveText('Archived');
    // Default is "active"
    await expect(dropdown).toHaveValue('active');
  });
});
