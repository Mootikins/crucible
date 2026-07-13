import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION } from './helpers/fixtures';

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

    await setupBasicMocks(page, {
      sessions: [UNTITLED_SESSION],
      sseEvents: [
        {
          type: 'title_changed',
          data: { type: 'title_changed', title: generatedTitle },
        },
      ],
    });
    await mockUntitledSessionGet(page);
    const legacyCalls = await watchLegacyTitleCalls(page);

    await page.goto('/');

    // Untitled sessions render the id-based fallback until a title arrives.
    const sessionButton = page.getByTestId(`session-item-${SESSION_ID}`);
    await expect(sessionButton).toBeVisible({ timeout: 5000 });
    await expect(sessionButton).toContainText(/Session test-ses/);

    // Opening the session connects its SSE stream, which delivers the
    // daemon's title_changed event.
    await sessionButton.click();
    await expect(sessionButton).toContainText(generatedTitle, { timeout: 5000 });
    await expect(sessionButton).not.toContainText(/Session test-ses/);

    // The daemon owns titling — the client must not call the title endpoints.
    expect(legacyCalls).toHaveLength(0);
  });

  test('untitled sessions keep the fallback label until the daemon titles them', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [UNTITLED_SESSION], sseEvents: [] });
    await mockUntitledSessionGet(page);
    const legacyCalls = await watchLegacyTitleCalls(page);

    await page.goto('/');

    const sessionButton = page.getByTestId(`session-item-${SESSION_ID}`);
    await expect(sessionButton).toBeVisible({ timeout: 5000 });
    await sessionButton.click();

    // Chat is usable; no title event arrived, so the fallback stays and no
    // client-side title generation is attempted.
    const chatInput = page.getByTestId('chat-input');
    await expect(chatInput).toBeVisible({ timeout: 5000 });
    await expect(sessionButton).toContainText(/Session test-ses/);
    expect(legacyCalls).toHaveLength(0);
  });
});
