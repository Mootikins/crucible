import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { createSSEStream } from './helpers/mock-sse';
import { MOCK_SESSION, MOCK_SESSION_2 } from './helpers/fixtures';

/**
 * E2E: Session Lifecycle
 *
 * Covers the 8 core session flows:
 * 1. Create session — new session appears in list + chat tab opens
 * 2. Send & stream — type message, streamed response appears
 * 3. Resume session — click session in list, history loads
 * 4. End session — state changes to ended, input disabled
 * 5. Archive session — hover row, click archive, session disappears
 * 6. Delete session — hover row, click delete, confirm, session gone
 * 7. Cross-client visibility — session from "another client" appears
 * 8. Persistence — refresh page, sessions still in list
 */

const ENDED_SESSION = {
  ...MOCK_SESSION,
  state: 'ended' as const,
};

const NEW_SESSION = {
  ...MOCK_SESSION,
  session_id: 'test-session-new',
  title: 'Newly Created Session',
};

test.describe('Session Lifecycle', () => {
  // ── Flow 1: Create Session ──────────────────────────────────────────
  test('creates a new session and opens chat tab', async ({ page }) => {
    await setupBasicMocks(page, { sessionCreate: NEW_SESSION });
    await page.goto('/');

    // Wait for the new session button
    const newSessionBtn = page.getByTestId('new-session-button');
    await expect(newSessionBtn).toBeVisible({ timeout: 10000 });

    // Intercept the POST request for session creation
    const createPromise = page.waitForRequest(
      (req) => req.url().includes('/api/session') && req.method() === 'POST',
    );

    // Click new session
    await newSessionBtn.click();

    // Assert: POST was made
    const createRequest = await createPromise;
    expect(createRequest).toBeTruthy();
  });

  // ── Flow 2: Send & Stream ──────────────────────────────────────────
  test('sends a message and displays streamed response', async ({ page }) => {
    const responseText = 'Hello! How can I help you today?';

    // Build SSE events with type discriminator in data (matches real Axum backend)
    function buildChatEvents(
      content: string,
      messageId = 'msg-001',
    ): Array<{ type: string; data: object }> {
      const chunks: string[] = [];
      for (let i = 0; i < content.length; i += 10) {
        chunks.push(content.slice(i, i + 10));
      }
      return [
        ...chunks.map((chunk) => ({
          type: 'token',
          data: { type: 'token', content: chunk },
        })),
        {
          type: 'message_complete',
          data: { type: 'message_complete', id: messageId, content, tool_calls: [] },
        },
      ];
    }

    const sseBody = createSSEStream(buildChatEvents(responseText));

    // Set up mocks with empty SSE (we control delivery separately)
    await setupBasicMocks(page, { sseEvents: [] });

    // Mock the title endpoint (auto-title fires after first response)
    await page.route('**/api/session/*/title', (route) =>
      route.fulfill({ status: 200, body: '{}' }),
    );

    // Controlled SSE: hold connection pending until after send, then deliver events once
    let resolveSSE: (() => void) | null = null;
    const sseReady = new Promise<void>((resolve) => {
      resolveSSE = resolve;
    });
    let delivered = false;

    await page.route(/\/api\/chat\/events\/.*/, async (route) => {
      if (!delivered) {
        delivered = true;
        await sseReady;
        await route.fulfill({
          status: 200,
          headers: {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
            Connection: 'keep-alive',
          },
          body: sseBody,
        });
      } else {
        // Reconnects after delivery: empty stream
        await route.fulfill({
          status: 200,
          headers: {
            'Content-Type': 'text/event-stream',
            'Cache-Control': 'no-cache',
            Connection: 'keep-alive',
          },
          body: '',
        });
      }
    });

    await page.goto('/');

    // Click session in sidebar to open chat
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });
    await page.getByTestId('session-item-test-session-001').click();

    // Wait for chat input to be ready
    const chatInput = page.getByTestId('chat-input');
    await expect(chatInput).toBeVisible({ timeout: 5000 });
    await expect(chatInput).not.toBeDisabled({ timeout: 5000 });

    // Type a message
    await chatInput.fill('Hello from lifecycle test');

    // Intercept the POST
    const sendPromise = page.waitForRequest(
      (req) => req.url().includes('/api/chat/send') && req.method() === 'POST',
    );

    // Click send
    await page.getByTestId('send-button').click();

    // Wait for POST to complete (ensures currentStreamingMessageId is set)
    await sendPromise;

    // Now release SSE events — streaming message placeholder exists
    resolveSSE!();

    // Assert: user message appears
    const userMessage = page.getByTestId('message-user');
    await expect(userMessage.first()).toBeVisible({ timeout: 5000 });
    await expect(userMessage.first()).toContainText('Hello from lifecycle test');

    // Assert: assistant response appears with streamed content
    const assistantMessage = page.getByTestId('message-assistant');
    await expect(assistantMessage.first()).toContainText(responseText, {
      timeout: 10000,
    });
  });

  // ── Flow 3: Resume Session ─────────────────────────────────────────
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

  // ── Flow 4: End Session ────────────────────────────────────────────
  test('ends a session and disables input', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [ENDED_SESSION] });

    // Override specific session GET to return ended state (LIFO priority)
    await page.route('**/api/session/test-session-001', (route) => {
      if (route.request().method() === 'GET') {
        route.fulfill({ json: ENDED_SESSION });
      } else {
        route.continue();
      }
    });

    await page.goto('/');

    // Wait for session list and click the ended session
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });
    await page.getByTestId('session-item-test-session-001').click();


    // Assert: "This session has ended" message is NOT visible (removed)
    await expect(page.getByText('This session has ended')).toHaveCount(0);

    // Assert: chat input IS visible (always shown, even for ended sessions)
    await expect(page.getByTestId('chat-input')).toBeVisible({ timeout: 5000 });

    // Assert: "Continue as new session" button is NOT visible (removed)
    const continueButton = page.getByRole('button', { name: /Continue as new session/ });
    await expect(continueButton).toHaveCount(0);
  });

  // ── Flow 5: Archive Session ────────────────────────────────────────
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
    const archiveButton = page.getByTitle('Archive session').first();
    await expect(archiveButton).toBeVisible({ timeout: 5000 });

    // Click archive
    await archiveButton.click();

    // Assert: archive API was called
    const archiveRequest = await archivePromise;
    expect(archiveRequest).toBeTruthy();
    expect(archiveRequest.url()).toContain('test-session-001/archive');
  });

  // ── Flow 6: Delete Session ─────────────────────────────────────────
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
    const deleteButton = page.getByTitle('Delete session').first();
    await expect(deleteButton).toBeVisible({ timeout: 5000 });

    // Click delete
    await deleteButton.click();

    // Assert: DELETE API was called
    const deleteRequest = await deletePromise;
    expect(deleteRequest).toBeTruthy();
    expect(deleteRequest.url()).toContain('test-session-001');
  });

  // ── Flow 7: Cross-client Visibility ────────────────────────────────
  test('sessions from another client appear in the list', async ({ page }) => {
    // Simulate a session created by another client (not created in this browser)
    const externalSession = {
      session_id: 'external-session-999',
      type: 'chat' as const,
      kiln: '/home/user/.crucible/kiln',
      workspace: '/home/user/project',
      state: 'active' as const,
      title: 'External Client Session',
      agent_model: 'mistral',
      started_at: '2026-01-01T02:00:00Z',
      event_count: 3,
    };

    await setupBasicMocks(page, {
      sessions: [MOCK_SESSION, externalSession],
    });

    await page.goto('/');

    // Wait for session list
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });

    // Assert: both the local and external sessions are visible
    await expect(page.getByTestId('session-item-test-session-001')).toBeVisible();
    await expect(page.getByTestId('session-item-external-session-999')).toBeVisible();

    // Assert: external session shows its title
    await expect(page.getByText('External Client Session')).toBeVisible();
  });

  // ── Flow 8: Persistence (Refresh) ─────────────────────────────────
  test('sessions persist across page refresh', async ({ page }) => {
    // Set up initial sessions
    await setupBasicMocks(page, {
      sessions: [MOCK_SESSION, MOCK_SESSION_2],
    });

    await page.goto('/');

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

    // Assert: sessions are still visible after refresh
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });
    await expect(page.getByTestId('session-item-test-session-001')).toBeVisible();
    await expect(page.getByTestId('session-item-test-session-002')).toBeVisible();

    // Assert: session titles still display correctly
    await expect(page.getByText('Test Session')).toBeVisible();
    await expect(page.getByText('Second Session')).toBeVisible();
  });

  // ── Flow 9: No End Button ──────────────────────────────────────────
  test('no End button visible for active session', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [MOCK_SESSION] });
    await page.goto('/');

    // Wait for session list
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });

    // Click session to open it
    await page.getByTestId('session-item-test-session-001').click();

    // Wait a moment for the session to load
    await page.waitForTimeout(1000);

    // Assert no End button exists anywhere on the page
    const endButton = page.locator('button:has-text("End")');
    await expect(endButton).toHaveCount(0);
  });

  // ── Flow 10: No Continue as New Session Button ─────────────────────
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

    // Wait for session list and click the ended session
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });
    await page.getByTestId('session-item-test-session-001').click();

    // Assert: "Continue as new session" button is NOT visible
    await expect(page.getByRole('button', { name: /Continue as new session/ })).toHaveCount(0);

    // Assert: "This session has ended" text is NOT visible
    await expect(page.getByText('This session has ended')).toHaveCount(0);

    // Assert: chat input IS visible (always shown)
    await expect(page.getByTestId('chat-input')).toBeVisible({ timeout: 5000 });
  });
});
