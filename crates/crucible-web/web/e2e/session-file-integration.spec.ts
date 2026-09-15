import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { openSessionsList } from './helpers/nav';

/**
 * E2E: Session + File Tab Integration
 *
 * Verifies that session (chat) and file tabs coexist in the center pane,
 * without either replacing the other.
 */

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
          { name: 'My Note', path: '/home/user/notes/My Note.md', is_dir: false },
        ],
      });
    });

    await page.goto('/');
    await openSessionsList(page);

    // Wait for session list
    await expect(page.getByTestId('session-list')).toBeVisible({ timeout: 10000 });

    // Click session in sidebar → opens chat tab
    await page.getByTestId('session-item-test-session-001').click();
    const chatTab = page.locator('[data-tab-id^="tab-chat-"]');
    await expect(chatTab).toBeVisible({ timeout: 5000 });

    // Open a file → opens file tab
    await openFile(page, '/home/user/notes/My Note.md', 'My Note.md');
    const fileTab = page.locator('[data-tab-id^="tab-file-"]');
    await expect(fileTab).toBeVisible({ timeout: 5000 });

    // Assert: both tabs exist in the center pane
    await expect(chatTab).toHaveCount(1);
    await expect(fileTab).toHaveCount(1);
  });

});
