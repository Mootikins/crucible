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

  test('filter Active shows only non-archived non-ended sessions', async ({ page }) => {
    const archivedSession = { ...MOCK_SESSION, session_id: 'archived-001', title: 'Archived Session', archived: true };
    const endedSession = { ...MOCK_SESSION, session_id: 'ended-001', title: 'Ended Session', state: 'ended' as const };
    await setupBasicMocks(page, { sessions: [MOCK_SESSION, archivedSession, endedSession] });
    await page.goto('/');
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });
    // Default filter is 'active' — should show only MOCK_SESSION
    await expect(page.getByTestId('session-item-test-session-001')).toBeVisible();
    await expect(page.getByTestId('session-item-archived-001')).toHaveCount(0);
    await expect(page.getByTestId('session-item-ended-001')).toHaveCount(0);
  });

  test('filter All shows all sessions including archived', async ({ page }) => {
    const archivedSession = { ...MOCK_SESSION, session_id: 'archived-001', title: 'Archived Session', archived: true };
    await setupBasicMocks(page, { sessions: [MOCK_SESSION, archivedSession] });
    await page.goto('/');
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });
    // Switch to 'all'
    await page.getByTestId('session-filter-dropdown').selectOption('all');
    await expect(page.getByTestId('session-item-test-session-001')).toBeVisible();
    await expect(page.getByTestId('session-item-archived-001')).toBeVisible();
  });

  test('filter Archived shows only archived sessions', async ({ page }) => {
    const archivedSession = { ...MOCK_SESSION, session_id: 'archived-001', title: 'Archived Session', archived: true };
    await setupBasicMocks(page, { sessions: [MOCK_SESSION, archivedSession] });
    await page.goto('/');
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });
    // Switch to 'archived'
    await page.getByTestId('session-filter-dropdown').selectOption('archived');
    await expect(page.getByTestId('session-item-test-session-001')).toHaveCount(0);
    await expect(page.getByTestId('session-item-archived-001')).toBeVisible();
  });
});
