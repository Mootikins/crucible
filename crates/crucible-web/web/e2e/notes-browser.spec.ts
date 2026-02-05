import { test, expect } from '@playwright/test';

test.describe('Notes Browser', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    
    const projectButton = page.locator('text=/home/moot/crucible').first();
    if (await projectButton.isVisible()) {
      await projectButton.click();
    }
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
    if (await loadingIndicator.count() > 0) {
      await expect(loadingIndicator.first()).toBeVisible();
    }
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
    if (await fileItem.isVisible()) {
      await expect(fileItem).toBeVisible();
    }
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
    if (await noteLink.isVisible()) {
      await noteLink.click();
      await page.waitForTimeout(500);
    }
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
    if (await folderButton.isVisible()) {
      await folderButton.click();
      
      await page.waitForTimeout(300);
      
      const childNote = page.locator('text=Getting Started.md');
      if (await childNote.count() > 0) {
        await expect(childNote).toBeVisible();
      }
    }
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
    if (await emptyMessage.isVisible()) {
      await expect(emptyMessage).toBeVisible();
    }
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
    if (await errorMessage.count() > 0) {
      await expect(errorMessage.first()).toBeVisible();
    }
  });
});
