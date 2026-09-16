import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION, MOCK_SESSION_2 } from './helpers/fixtures';
import { openSessionsList } from './helpers/nav';

/**
 * UI: Session Lifecycle — browser-boundary coverage only.
 *
 * What is left here is what the daemon does not decide: a reload, and two
 * DOM-absence contracts. The API is mocked, so a refusal is invisible by
 * construction — which is why the three tests that used to live here moved.
 *
 * Moved to the live tier (`e2e/live/session-lifecycle.live.spec.ts`) because
 * each asserted only that a REQUEST fired, and a request firing is not the
 * daemon acting:
 *   - resume-and-load-history  → a real turn, answered by the fake model
 *   - archive via hover        → the daemon's list, after the click
 *   - delete via context menu  → the daemon's list, archived included
 *
 * Trimmed earlier (moved / already covered elsewhere):
 *   - Create-on-first-message → live session-management.live.spec.ts
 *   - Send-and-stream        → chat-happy-path.spec.ts:39 (identical)
 *   - End-state absence #1   → Flow 10 below + SessionContext.test.tsx:155 (resume logic)
 *   - Cross-client listing   → live session-management.live.spec.ts
 */

test.describe('Session Lifecycle', () => {

  // ── Persistence: page refresh re-fetches and re-renders ───────────
  // UI: verifies that after a real page.reload(), the app re-bootstraps, re-fetches the session list, and re-renders both items with their titles — the reload + re-fetch lifecycle is browser-boundary only.
  test('sessions persist across page refresh', async ({ page }) => {
    // Set up initial sessions
    await setupBasicMocks(page, {
      sessions: [MOCK_SESSION, MOCK_SESSION_2],
    });

    await page.goto('/');
    await openSessionsList(page);

    // Wait for session list with both sessions
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });
    await expect(page.getByTestId('session-item-test-session-001')).toBeVisible();
    await expect(page.getByTestId('session-item-test-session-002')).toBeVisible();

    // Re-register mocks (page.route is cleared on navigation/reload)
    await setupBasicMocks(page, {
      sessions: [MOCK_SESSION, MOCK_SESSION_2],
    });

    // Refresh the page
    await page.reload();
    await openSessionsList(page);

    // Assert: sessions are still visible after refresh
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });
    await expect(page.getByTestId('session-item-test-session-001')).toBeVisible();
    await expect(page.getByTestId('session-item-test-session-002')).toBeVisible();

    // Assert: session titles still display correctly
    await expect(page.getByTestId('session-list').getByText('Test Session')).toBeVisible();
    await expect(page.getByTestId('session-list').getByText('Second Session')).toBeVisible();
  });

  // ── Active session: no End button rendered ─────────────────────────
  // UI: verifies a positive load signal (chat-input visible) precedes the DOM absence assertion for the End button on an active session — guards against false-pass on an unmounted panel.
  test('no End button visible for active session', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [MOCK_SESSION] });
    await page.goto('/');
    await openSessionsList(page);

    // Wait for session list
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });

    // Click session to open it
    await page.getByTestId('session-item-test-session-001').click();

    // Positive load signal FIRST: the session panel rendered its chat input.
    // Otherwise `toHaveCount(0)` below is a false pass on an unloaded panel.
    await expect(page.getByTestId('chat-input')).toBeVisible({ timeout: 5000 });

    // Assert no End button exists anywhere on the loaded session panel.
    const endButton = page.locator('button:has-text("End")');
    await expect(endButton).toHaveCount(0);
  });

  // ── Ended session: no Continue / no ended banner rendered ──────────
  // UI: verifies a positive load signal (chat-input visible) precedes the DOM absence assertions for ended-state affordances — locks the current "ended sessions are transparently resumable, no dead-end UI" contract at the rendered-DOM level (logic counterpart: SessionContext.test.tsx:155).
  test('no Continue as new session button in ended session', async ({ page }) => {
    const endedSession = { ...MOCK_SESSION, state: 'ended' as const };
    await setupBasicMocks(page, { sessions: [endedSession] });

    // Override specific session GET to return ended state
    await page.route('**/api/session/test-session-001', (route) => {
      if (route.request().method() === 'GET') {
        route.fulfill({ json: endedSession });
      } else {
        route.continue();
      }
    });

    await page.goto('/');
    await openSessionsList(page);

    // Wait for session list and click the ended session
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });
    // Switch to 'all' filter so ended sessions are visible
    await page.getByTestId('session-item-test-session-001').click();

    // Positive load signal FIRST: the ended session's chat input rendered.
    // The absence checks below are only meaningful once the panel is loaded.
    await expect(page.getByTestId('chat-input')).toBeVisible({ timeout: 5000 });

    // Assert: "Continue as new session" button is NOT visible
    await expect(page.getByRole('button', { name: /Continue as new session/ })).toHaveCount(0);

    // Assert: "This session has ended" text is NOT visible
    await expect(page.getByText('This session has ended')).toHaveCount(0);
  });
});
