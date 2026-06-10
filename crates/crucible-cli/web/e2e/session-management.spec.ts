import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION, MOCK_SESSION_2 } from './helpers/fixtures';

/**
 * E2E: Session Management
 *
 * Verifies session list display, creation, and selection flows.
 */

test.describe('Session Management', () => {
  test('displays sessions in the session panel', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [MOCK_SESSION, MOCK_SESSION_2] });
    await page.goto('/');

    // Wait for session list to be visible (requires project to load first)
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });

    // Assert both session items are visible
    await expect(page.getByTestId('session-item-test-session-001')).toBeVisible();
    await expect(page.getByTestId('session-item-test-session-002')).toBeVisible();

    // Assert session titles are visible in the panel
    await expect(page.getByText('Test Session')).toBeVisible();
    await expect(page.getByText('Second Session')).toBeVisible();
  });

  test('creates a new session when new session button is clicked', async ({ page }) => {
    const newSession = { ...MOCK_SESSION, session_id: 'test-session-new', title: 'New Session' };
    await setupBasicMocks(page, { sessionCreate: newSession });
    await page.goto('/');

    // Wait for the new session button to be visible
    const newSessionBtn = page.getByTestId('new-session-button');
    await expect(newSessionBtn).toBeVisible({ timeout: 10000 });

    // Set up request interception before clicking
    const requestPromise = page.waitForRequest(
      (req) => req.url().includes('/api/session') && req.method() === 'POST',
    );

    // Click the new session button
    await newSessionBtn.click();

    // Assert: the POST request was made (waitForRequest resolves = success)
    await requestPromise;
  });

  test('selects a session when clicked', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [MOCK_SESSION, MOCK_SESSION_2] });

    // Add specific route for session-002 details (LIFO = higher priority than wildcard)
    await page.route('**/api/session/test-session-002', (route) =>
      route.fulfill({ json: MOCK_SESSION_2 }),
    );

    await page.goto('/');

    // Wait for session list to be visible
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });

    // Set up request interception for session-002 before clicking
    const requestPromise = page.waitForRequest(
      (req) => req.url().includes('test-session-002') && req.method() === 'GET',
    );

    // Click the second session
    await page.getByTestId('session-item-test-session-002').click();

    // Assert: GET request for session-002 was made
    await requestPromise;
  });
});
