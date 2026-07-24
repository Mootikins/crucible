import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION } from './helpers/fixtures';
import { openSessionsList } from './helpers/nav';

/**
 * E2E: the Navigator's session list.
 *
 * The old active/all/archived <select> is gone. The list is recency-ordered
 * and archived sessions live behind a collapsible "Archived · N" section.
 */

const archived = { ...MOCK_SESSION, session_id: 'archived-001', title: 'Archived Session', archived: true };
const ended = { ...MOCK_SESSION, session_id: 'ended-001', title: 'Ended Session', state: 'ended' as const };

test.describe('Session list', () => {
  test('lists non-archived sessions and hides archived ones behind the toggle', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [MOCK_SESSION, archived, ended] });
    await page.goto('/');
    await openSessionsList(page);
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });

    // Active and ended sessions both belong to the main list; only archiving
    // moves a session out of it.
    await expect(page.getByTestId('session-item-test-session-001')).toBeVisible();
    await expect(page.getByTestId('session-item-ended-001')).toBeVisible();
    await expect(page.getByTestId('session-item-archived-001')).toHaveCount(0);
  });

  test('expanding Archived reveals the archived sessions', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [MOCK_SESSION, archived] });
    await page.goto('/');
    await openSessionsList(page);
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });

    const toggle = page.getByRole('button', { name: /Archived · 1/ });
    await expect(toggle).toBeVisible();
    await toggle.click();

    await expect(page.getByTestId('session-item-archived-001')).toBeVisible();
    // The active session stays listed alongside it.
    await expect(page.getByTestId('session-item-test-session-001')).toBeVisible();

    // Collapsing hides them again.
    await toggle.click();
    await expect(page.getByTestId('session-item-archived-001')).toHaveCount(0);
  });

  test('shows an empty-state line when there are no sessions', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [] });
    await page.goto('/');
    await openSessionsList(page);
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });
    await expect(page.getByText('No sessions yet')).toBeVisible();
  });
});
