import { test, expect } from '@playwright/test';

test.describe('Notes Browser', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    
    const projectButton = page.getByText('/home/moot/crucible').first();
    await expect(projectButton).toBeVisible();
    await projectButton.click();
  });

  test('displays notes panel header', async ({ page }) => {
    await expect(page.locator('text=Notes')).toBeVisible();
  });

  test('displays kiln section', async ({ page }) => {
    await expect(page.locator('text=Kiln')).toBeVisible();
  });

  test('shows loading state when fetching notes', async ({ page }) => {
    await page.route('**/api/notes*', async (route) => {
      await new Promise(resolve => setTimeout(resolve, 2000));
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([]),
      });
    });

    await page.reload();
    
    const loadingIndicator = page.locator('text=Loading..., [class*="animate-spin"]');
    await expect(loadingIndicator.first()).toBeVisible();
  });

  test('displays file tree with icons', async ({ page }) => {
    await page.route('**/api/notes*', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([
          { name: 'Index.md', path: '/kiln/Index.md', is_dir: false },
          { name: 'Help', path: '/kiln/Help', is_dir: true },
        ]),
      });
    });

    await page.reload();
    await page.waitForTimeout(500);
    
    const fileItem = page.locator('text=Index.md');
    await expect(fileItem).toBeVisible();
  });

  test('can click on a note to open it', async ({ page }) => {
    await page.route('**/api/notes*', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([
          { name: 'Test.md', path: '/kiln/Test.md', is_dir: false },
        ]),
      });
    });

    await page.route('**/api/notes/Test.md', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'text/plain',
        body: '# Test Note\n\nContent here.',
      });
    });

    await page.reload();
    await page.waitForTimeout(500);
    
    const noteLink = page.locator('text=Test.md');
    await expect(noteLink).toBeVisible();
    await noteLink.click();
    await page.waitForTimeout(500);
  });

  test('can expand and collapse folders', async ({ page }) => {
    await page.route('**/api/notes*', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([
          { 
            name: 'Guides', 
            path: '/kiln/Guides', 
            is_dir: true,
            children: [
              { name: 'Getting Started.md', path: '/kiln/Guides/Getting Started.md', is_dir: false }
            ]
          },
        ]),
      });
    });

    await page.reload();
    await page.waitForTimeout(500);
    
    const folderButton = page.locator('text=Guides').first();
    await expect(folderButton).toBeVisible();
    await folderButton.click();
    
    await page.waitForTimeout(300);
    
    const childNote = page.locator('text=Getting Started.md');
    await expect(childNote).toBeVisible();
  });

  test('displays empty state when no notes', async ({ page }) => {
    await page.route('**/api/notes*', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([]),
      });
    });

    await page.reload();
    await page.waitForTimeout(500);
    
    const emptyMessage = page.locator('text=No files');
    await expect(emptyMessage).toBeVisible();
  });

  test('displays error state on API failure', async ({ page }) => {
    await page.route('**/api/notes*', async (route) => {
      await route.fulfill({
        status: 500,
        contentType: 'application/json',
        body: JSON.stringify({ error: 'Failed to load notes' }),
      });
    });

    await page.reload();
    await page.waitForTimeout(500);
    
    const errorMessage = page.locator('[class*="text-red"]');
    await expect(errorMessage.first()).toBeVisible();
  });
});
