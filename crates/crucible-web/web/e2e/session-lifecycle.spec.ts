import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION, MOCK_SESSION_2 } from './helpers/fixtures';
import { openSessionsList } from './helpers/nav';

/**
 * E2E: Session Lifecycle — browser-boundary coverage only.
 *
 * Each surviving test verifies a behavior that depends on the real
 * browser-to-application boundary (DOM rendering, hover-revealed controls,
 * native dialog handling, page refresh, SSE-driven history hydration) and
 * cannot be replaced by `SessionContext.test.tsx` or another playwright spec.
 *
 * Trimmed coverage (moved / already covered elsewhere):
 *   - Create-on-first-message → session-management.spec.ts:30 + new-session-chat-tab.spec.ts:69
 *   - Send-and-stream        → chat-happy-path.spec.ts:39 (identical)
 *   - End-state absence #1   → Flow 10 below + SessionContext.test.tsx:155 (resume logic)
 *   - Cross-client listing   → session-management.spec.ts:13 (multi-session list + titles)
 */

test.describe('Session Lifecycle', () => {
  // ── Resume Session: history events hydrate the DOM ─────────────────
  // E2E: verifies session history payload (events from another time) renders into the chat DOM when a session is selected — not coverable by the isolated SessionContext unit test, which only asserts the API call dispatch.
  test('resumes a session and loads history', async ({ page }) => {
    const historyEvents = {
      session_id: MOCK_SESSION.session_id,
      history: [
        {
          type: 'event',
          session_id: MOCK_SESSION.session_id,
          event: 'user_message',
          data: { content: 'Previous user message', message_id: 'hist-msg-001' },
        },
        {
          type: 'event',
          session_id: MOCK_SESSION.session_id,
          event: 'message_complete',
          data: { full_response: 'Previous assistant response', message_id: 'hist-msg-002' },
        },
      ],
      total_events: 2,
    };

    await setupBasicMocks(page, { sessionHistory: historyEvents });
    await page.goto('/');
    await openSessionsList(page);

    // Wait for session list
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });

    // Intercept the GET for session details
    const detailsPromise = page.waitForRequest(
      (req) => req.url().includes('test-session-001') && req.method() === 'GET',
    );

    // Click the session to resume
    await page.getByTestId('session-item-test-session-001').click();

    // Assert: GET request was made for the session
    const detailsRequest = await detailsPromise;
    expect(detailsRequest).toBeTruthy();

    // Assert: history messages are rendered
    const userMessage = page.getByTestId('message-user');
    await expect(userMessage.first()).toContainText('Previous user message', {
      timeout: 10000,
    });

    const assistantMessage = page.getByTestId('message-assistant');
    await expect(assistantMessage.first()).toContainText('Previous assistant response', {
      timeout: 10000,
    });
  });

  // ── Archive Session: hover-revealed action → POST /archive ─────────
  // E2E: verifies the CSS-hover-revealed archive button is reachable via real pointer hover and fires POST /api/session/<id>/archive — the hover transition + button targeting is purely browser-boundary.
  test('archives a session via hover action button', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [MOCK_SESSION, MOCK_SESSION_2] });

    // Mock the archive endpoint
    await page.route('**/api/session/*/archive', (route) =>
      route.fulfill({ json: { archived: true } }),
    );

    // Mock unarchive endpoint too
    await page.route('**/api/session/*/unarchive', (route) =>
      route.fulfill({ json: { archived: false } }),
    );

    await page.goto('/');
    await openSessionsList(page);

    // Wait for session list with both sessions visible
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });
    await expect(page.getByTestId('session-item-test-session-001')).toBeVisible();

    // Intercept the archive POST
    const archivePromise = page.waitForRequest(
      (req) => req.url().includes('/archive') && req.method() === 'POST',
    );

    // Hover over the session row to reveal action buttons
    const sessionRow = page.getByTestId('session-item-test-session-001');
    await sessionRow.hover();

    // Wait for the archive button to become visible (opacity transition)
    const archiveButton = sessionRow.getByTitle('Archive session');
    await expect(archiveButton).toBeVisible({ timeout: 5000 });

    // Click archive
    await archiveButton.click();

    // Assert: archive API was called
    const archiveRequest = await archivePromise;
    expect(archiveRequest).toBeTruthy();
    expect(archiveRequest.url()).toContain('test-session-001/archive');
  });

  // ── Delete Session: native confirm() dialog + DELETE request ───────
  // E2E: verifies the browser-native window.confirm() dialog is accepted by the page's dialog handler and the DELETE method actually fires — the confirm() round-trip only exists at the browser boundary.
  test('deletes a session via hover action button with confirmation', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [MOCK_SESSION, MOCK_SESSION_2] });

    // Mock the DELETE endpoint
    await page.route('**/api/session/test-session-001', async (route) => {
      if (route.request().method() === 'DELETE') {
        route.fulfill({ json: { deleted: true } });
      } else if (route.request().method() === 'GET') {
        route.fulfill({ json: MOCK_SESSION });
      } else {
        route.continue();
      }
    });

    await page.goto('/');
    await openSessionsList(page);

    // Wait for session list
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });
    await expect(page.getByTestId('session-item-test-session-001')).toBeVisible();

    // Set up dialog handler to accept confirm() BEFORE triggering delete
    page.on('dialog', (dialog) => dialog.accept());

    // Intercept the DELETE request
    const deletePromise = page.waitForRequest(
      (req) => req.url().includes('test-session-001') && req.method() === 'DELETE',
    );

    // Hover over the session row to reveal action buttons
    const sessionRow = page.getByTestId('session-item-test-session-001');
    await sessionRow.hover();

    // Wait for the delete button to become visible (opacity transition)
    const deleteButton = sessionRow.getByTitle('Delete session');
    await expect(deleteButton).toBeVisible({ timeout: 5000 });

    // Click delete
    await deleteButton.click();

    // Assert: DELETE API was called
    const deleteRequest = await deletePromise;
    expect(deleteRequest).toBeTruthy();
    expect(deleteRequest.url()).toContain('test-session-001');
  });

  // ── Persistence: page refresh re-fetches and re-renders ───────────
  // E2E: verifies that after a real page.reload(), the app re-bootstraps, re-fetches the session list, and re-renders both items with their titles — the reload + re-fetch lifecycle is browser-boundary only.
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
  // E2E: verifies a positive load signal (chat-input visible) precedes the DOM absence assertion for the End button on an active session — guards against false-pass on an unmounted panel.
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
  // E2E: verifies a positive load signal (chat-input visible) precedes the DOM absence assertions for ended-state affordances — locks the current "ended sessions are transparently resumable, no dead-end UI" contract at the rendered-DOM level (logic counterpart: SessionContext.test.tsx:155).
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
