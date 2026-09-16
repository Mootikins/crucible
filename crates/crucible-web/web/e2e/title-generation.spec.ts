import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION } from './helpers/fixtures';
import { openSessionsList } from './helpers/nav';

/**
 * E2E: Session auto-titles (daemon-owned).
 *
 * Titles are generated daemon-side on the first completed turn and pushed to
 * every client as a `title_changed` SSE event; the frontend only renders. The
 * old client-driven flow (POST /auto-title after the first assistant response,
 * then PUT /title) was removed with the daemon auto-title work — these tests
 * pin the new contract: the event updates the UI, and the client never calls
 * the title endpoints on its own.
 */

const SESSION_ID = MOCK_SESSION.session_id;

/** Session with no title (the daemon will auto-title it). */
const UNTITLED_SESSION = {
  ...MOCK_SESSION,
  title: null,
};

/** Track any client-initiated title traffic — there must be none. */
async function watchLegacyTitleCalls(page: import('@playwright/test').Page) {
  const calls: string[] = [];
  await page.route('**/api/session/*/auto-title', (route) => {
    calls.push('POST auto-title');
    return route.fulfill({ json: { title: 'should never be requested' } });
  });
  await page.route('**/api/session/*/title', (route) => {
    calls.push(`${route.request().method()} title`);
    return route.fulfill({ status: 200, body: '{}' });
  });
  return calls;
}

/**
 * Makes `session.list` answer the generated title from the moment the chat
 * stream is requested.
 *
 * Both routes are registered after `setupBasicMocks`, and Playwright matches
 * the most recently added route first, so these win over its defaults.
 */
async function mockListRenamedAfterTheStreamOpens(
  page: import('@playwright/test').Page,
  title: string,
) {
  let renamed = false;

  await page.route('**/api/session/list**', (route) =>
    route.fulfill({
      json: {
        sessions: [renamed ? { ...UNTITLED_SESSION, title } : UNTITLED_SESSION],
        total: 1,
      },
    }),
  );

  await page.route(/\/api\/chat\/events\/.*/, (route) => {
    renamed = true;
    route.fulfill({
      status: 200,
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        Connection: 'keep-alive',
      },
      body: `event: title_changed\ndata: ${JSON.stringify({
        type: 'title_changed',
        title,
      })}\n\n`,
    });
  });
}

async function mockUntitledSessionGet(page: import('@playwright/test').Page) {
  await page.route(`**/api/session/${SESSION_ID}`, async (route) => {
    if (route.request().method() === 'GET') {
      await route.fulfill({ json: UNTITLED_SESSION });
    } else {
      await route.fallback();
    }
  });
}

test.describe('daemon session auto-titles', () => {
  test('title_changed SSE event renames the session across the UI', async ({ page }) => {
    const generatedTitle = 'Help with project setup';

    // The stream itself is mocked below, together with the listing it renames.
    await setupBasicMocks(page, { sessions: [UNTITLED_SESSION], sseEvents: [] });
    await mockUntitledSessionGet(page);
    // The event also invalidates both session-list keys, so the row re-reads
    // `session.list` right after the rename. A mock that keeps answering
    // `title: null` overwrites the patch the event just made. No daemon does
    // that: the daemon renames the session BEFORE it announces the rename.
    // This route keeps that order. The stream request flips the listing.
    await mockListRenamedAfterTheStreamOpens(page, generatedTitle);
    const legacyCalls = await watchLegacyTitleCalls(page);

    await page.goto('/');
    await openSessionsList(page);

    // Untitled sessions render the "Untitled · <date>" fallback (session-
    // display.ts) until a title arrives.
    const sessionButton = page.getByTestId(`session-item-${SESSION_ID}`);
    await expect(sessionButton).toBeVisible({ timeout: 5000 });
    await expect(sessionButton).toContainText(/Untitled/);

    // Opening the session connects its SSE stream, which delivers the
    // daemon's title_changed event.
    await sessionButton.click();
    await expect(sessionButton).toContainText(generatedTitle, { timeout: 5000 });
    await expect(sessionButton).not.toContainText(/Untitled/);

    // The daemon owns titling — the client must not call the title endpoints.
    expect(legacyCalls).toHaveLength(0);
  });

  test('untitled sessions keep the fallback label until the daemon titles them', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [UNTITLED_SESSION], sseEvents: [] });
    await mockUntitledSessionGet(page);
    const legacyCalls = await watchLegacyTitleCalls(page);

    await page.goto('/');
    await openSessionsList(page);

    const sessionButton = page.getByTestId(`session-item-${SESSION_ID}`);
    await expect(sessionButton).toBeVisible({ timeout: 5000 });
    await sessionButton.click();

    // Chat is usable; no title event arrived, so the fallback stays and no
    // client-side title generation is attempted.
    const chatInput = page.getByTestId('chat-input');
    await expect(chatInput).toBeVisible({ timeout: 5000 });
    await expect(sessionButton).toContainText(/Untitled/);
    expect(legacyCalls).toHaveLength(0);
  });
});
