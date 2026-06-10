import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';

/**
 * E2E: File Tab Flows
 *
 * Verifies empty state on fresh load, file tab creation via openFileInEditor,
 * and file tab deduplication (same file opened twice → single tab).
 *
 * File tabs are opened programmatically via the same code path as FilesPanel:
 * openFileInEditor() → windowActions.addTab() → center pane renders file tab.
 *
 * Note: We intercept the panel registry to use the Pane's built-in file
 * renderer instead of FileViewerPanel (which requires full EditorContext
 * wiring that isn't exercised in isolated E2E tests).
 */

/** Helper: open a file tab in the center pane via the file-actions module. */
async function openFile(page: import('@playwright/test').Page, path: string, name: string) {
  await page.evaluate(
    async ({ filePath, fileName }) => {
      // Redirect 'file' content type lookup away from FileViewerPanel so the
      // Pane component uses its built-in file renderer (avoids EditorContext
      // dependency in isolated E2E tests).
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

test.describe('File tab flows', () => {
  test('shows EmptyState on fresh load', async ({ page }) => {
    await setupBasicMocks(page);
    await page.goto('/');

    // Center pane starts with no tabs → EmptyState should render
    await expect(page.getByText('No session open')).toBeVisible({ timeout: 10000 });
    await expect(
      page.getByText('Select a session from the left panel or create a new one to get started.'),
    ).toBeVisible();
  });

  test('opening a file creates a file tab in the center pane', async ({ page }) => {
    await setupBasicMocks(page);

    // Mock notes API (file viewer fetches note content via getNote)
    await page.route('**/api/notes**', (route) => {
      const url = route.request().url();
      if (url.includes('/api/notes/')) {
        route.fulfill({
          json: {
            name: 'My Note',
            content: '# Hello World\n\nThis is test content.',
            path: '/home/user/.crucible/kiln/My Note.md',
          },
        });
      } else {
        route.fulfill({
          json: [
            { name: 'My Note', path: '/home/user/.crucible/kiln/My Note.md', is_dir: false },
          ],
        });
      }
    });

    await page.goto('/');
    await expect(page.getByText('No session open')).toBeVisible({ timeout: 10000 });

    // Open a file (same code path as clicking in FilesPanel)
    await openFile(page, '/home/user/.crucible/kiln/My Note.md', 'My Note.md');

    // Assert: a file tab appears in the center pane
    const fileTab = page.locator('[data-tab-id^="tab-file-"]');
    await expect(fileTab).toBeVisible({ timeout: 5000 });

    // Assert: EmptyState disappears (tab is active)
    await expect(page.getByText('No session open')).not.toBeVisible();
  });

  test('file tab deduplication — opening same file twice creates only one tab', async ({
    page,
  }) => {
    await setupBasicMocks(page);
    await page.goto('/');
    await expect(page.getByText('No session open')).toBeVisible({ timeout: 10000 });

    // Open the same file twice
    await openFile(page, '/home/user/.crucible/kiln/My Note.md', 'My Note.md');
    await openFile(page, '/home/user/.crucible/kiln/My Note.md', 'My Note.md');

    // Assert: only ONE file tab exists (deduplication works)
    const fileTabs = page.locator('[data-tab-id^="tab-file-"]');
    await expect(fileTabs).toHaveCount(1);
  });
});
