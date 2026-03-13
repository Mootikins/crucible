import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { MOCK_SESSION } from './helpers/fixtures';

/**
 * E2E: Session + File Tab Integration
 *
 * Verifies that session (chat) and file tabs coexist in the center pane,
 * ended sessions show the "Continue as new session" button, and clicking
 * that button creates a new session via POST.
 */

const ENDED_SESSION = {
  ...MOCK_SESSION,
  state: 'ended' as const,
};

/** Helper: open a file tab (same approach as file-tab.spec.ts). */
async function openFile(page: import('@playwright/test').Page, path: string, name: string) {
  await page.evaluate(
    async ({ filePath, fileName }) => {
      const { getGlobalRegistry } = await import('/src/lib/panel-registry.ts');
      const registry = getGlobalRegistry();
      const origGet = registry.get.bind(registry);
      registry.get = (id: string) => (id === 'file' ? undefined : origGet(id));

      const { openFileInEditor } = await import('/src/lib/file-actions.ts');
      openFileInEditor(filePath, fileName);
    },
    { filePath: path, fileName: name },
  );
}

test.describe('Session and file tab integration', () => {
  test('session and file tabs coexist in center pane', async ({ page }) => {
    await setupBasicMocks(page);

    // Mock notes API
    await page.route('**/api/notes**', (route) => {
      route.fulfill({
        json: [
          { name: 'My Note', path: '/home/user/.crucible/kiln/My Note.md', is_dir: false },
        ],
      });
    });

    await page.goto('/');

    // Wait for session list
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });

    // Click session in sidebar → opens chat tab
    await page.getByTestId('session-item-test-session-001').click();
    const chatTab = page.locator('[data-tab-id^="tab-chat-"]');
    await expect(chatTab).toBeVisible({ timeout: 5000 });

    // Open a file → opens file tab
    await openFile(page, '/home/user/.crucible/kiln/My Note.md', 'My Note.md');
    const fileTab = page.locator('[data-tab-id^="tab-file-"]');
    await expect(fileTab).toBeVisible({ timeout: 5000 });

    // Assert: both tabs exist in the center pane
    await expect(chatTab).toHaveCount(1);
    await expect(fileTab).toHaveCount(1);
  });

  test('ended session shows chat input (no continue button)', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [ENDED_SESSION] });

    // Override specific session GET to return ended state (LIFO priority over wildcard)
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

    // Assert: no 'Continue as new session' button (removed in lifecycle redesign)
    await expect(page.getByRole('button', { name: /Continue as new session/ })).toHaveCount(0);

    // Assert: 'This session has ended' text is NOT visible (removed in lifecycle redesign)
    await expect(page.getByText('This session has ended')).toHaveCount(0);

    // Assert: chat input IS visible (always shown regardless of session state)
    await expect(page.getByTestId('chat-input')).toBeVisible({ timeout: 5000 });
  });

  test('ended session does not show continue button (lifecycle redesign)', async ({ page }) => {
    await setupBasicMocks(page, { sessions: [ENDED_SESSION] });

    // Override specific session GET to return ended state
    await page.route('**/api/session/test-session-001', (route) => {
      if (route.request().method() === 'GET') {
        route.fulfill({ json: ENDED_SESSION });
      } else {
        route.continue();
      }
    });

    await page.goto('/');

    // Open the ended session
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });
    await page.getByTestId('session-item-test-session-001').click();

    // Assert: 'Continue as new session' button is NOT present
    await expect(page.getByRole('button', { name: /Continue as new session/ })).toHaveCount(0);

    // Assert: chat input IS visible (always shown)
    await expect(page.getByTestId('chat-input')).toBeVisible({ timeout: 5000 });
  });
});
