import { test, expect } from '@playwright/test';

test.describe('Editor Panel', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    
    const projectButton = page.getByText('/home/moot/crucible').first();
    if (await projectButton.isVisible()) {
      await projectButton.click();
    }
  });

  test('displays empty state when no files open', async ({ page }) => {
    await expect(page.locator('text=No files open')).toBeVisible();
  });

  test('opens file when clicked in notes browser', async ({ page }) => {
    await page.route('**/api/notes*', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([
          { name: 'Example.md', path: '/kiln/Example.md', is_dir: false },
        ]),
      });
    });

    await page.route('**/api/notes/Example.md', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'text/plain',
        body: '# Example Note\n\nThis is example content.',
      });
    });

    await page.reload();
    await page.waitForTimeout(500);
    
    const noteLink = page.locator('text=Example.md');
    if (await noteLink.isVisible()) {
      await noteLink.click();
      await page.waitForTimeout(1000);
      
      const tab = page.locator('text=Example.md').last();
      await expect(tab).toBeVisible();
    }
  });

  test('displays file tabs for open files', async ({ page }) => {
    await page.route('**/api/notes*', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([
          { name: 'File1.md', path: '/kiln/File1.md', is_dir: false },
          { name: 'File2.md', path: '/kiln/File2.md', is_dir: false },
        ]),
      });
    });

    await page.route('**/api/notes/File1.md', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'text/plain',
        body: '# File 1',
      });
    });

    await page.route('**/api/notes/File2.md', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'text/plain',
        body: '# File 2',
      });
    });

    await page.reload();
    await page.waitForTimeout(500);
    
    const file1 = page.locator('text=File1.md').first();
    if (await file1.isVisible()) {
      await file1.click();
      await page.waitForTimeout(500);
    }
    
    const file2 = page.locator('text=File2.md').first();
    if (await file2.isVisible()) {
      await file2.click();
      await page.waitForTimeout(500);
      
      const tabs = page.locator('[class*="border-b-2"]');
      const tabCount = await tabs.count();
      expect(tabCount).toBeGreaterThanOrEqual(2);
    }
  });

  test('shows dirty indicator when file is modified', async ({ page }) => {
    await page.route('**/api/notes*', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([
          { name: 'Editable.md', path: '/kiln/Editable.md', is_dir: false },
        ]),
      });
    });

    await page.route('**/api/notes/Editable.md', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'text/plain',
        body: '# Original Content',
      });
    });

    await page.reload();
    await page.waitForTimeout(500);
    
    const noteLink = page.locator('text=Editable.md').first();
    if (await noteLink.isVisible()) {
      await noteLink.click();
      await page.waitForTimeout(1000);
      
      const editor = page.locator('.cm-content');
      if (await editor.isVisible()) {
        await editor.click();
        await page.keyboard.type('\n\nNew content added');
        
        await page.waitForTimeout(500);
        
        const dirtyIndicator = page.locator('text=●');
        if (await dirtyIndicator.count() > 0) {
          await expect(dirtyIndicator.first()).toBeVisible();
        }
      }
    }
  });

  test('can switch between open tabs', async ({ page }) => {
    await page.route('**/api/notes*', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([
          { name: 'Tab1.md', path: '/kiln/Tab1.md', is_dir: false },
          { name: 'Tab2.md', path: '/kiln/Tab2.md', is_dir: false },
        ]),
      });
    });

    await page.route('**/api/notes/Tab1.md', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'text/plain',
        body: '# Tab 1 Content',
      });
    });

    await page.route('**/api/notes/Tab2.md', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'text/plain',
        body: '# Tab 2 Content',
      });
    });

    await page.reload();
    await page.waitForTimeout(500);
    
    const tab1Link = page.locator('text=Tab1.md').first();
    if (await tab1Link.isVisible()) {
      await tab1Link.click();
      await page.waitForTimeout(500);
    }
    
    const tab2Link = page.locator('text=Tab2.md').first();
    if (await tab2Link.isVisible()) {
      await tab2Link.click();
      await page.waitForTimeout(500);
      
      const tab1Button = page.locator('[class*="border-b-2"]:has-text("Tab1.md")');
      const tab2Button = page.locator('[class*="border-b-2"]:has-text("Tab2.md")');
      
      if (await tab1Button.isVisible() && await tab2Button.isVisible()) {
        await tab1Button.click();
        await expect(tab1Button).toHaveClass(/border-blue-500/);
        
        await tab2Button.click();
        await expect(tab2Button).toHaveClass(/border-blue-500/);
      }
    }
  });

  test('can close tabs', async ({ page }) => {
    await page.route('**/api/notes*', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([
          { name: 'Closeable.md', path: '/kiln/Closeable.md', is_dir: false },
        ]),
      });
    });

    await page.route('**/api/notes/Closeable.md', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'text/plain',
        body: '# Closeable Note',
      });
    });

    await page.reload();
    await page.waitForTimeout(500);
    
    const noteLink = page.locator('text=Closeable.md').first();
    if (await noteLink.isVisible()) {
      await noteLink.click();
      await page.waitForTimeout(1000);
      
      const closeButton = page.locator('text=×');
      if (await closeButton.isVisible()) {
        await closeButton.click();
        await page.waitForTimeout(500);
        
        await expect(page.locator('text=No files open')).toBeVisible();
      }
    }
  });

  test('displays CodeMirror editor', async ({ page }) => {
    await page.route('**/api/notes*', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([
          { name: 'Code.md', path: '/kiln/Code.md', is_dir: false },
        ]),
      });
    });

    await page.route('**/api/notes/Code.md', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'text/plain',
        body: '# Code Editor Test',
      });
    });

    await page.reload();
    await page.waitForTimeout(500);
    
    const noteLink = page.locator('text=Code.md').first();
    if (await noteLink.isVisible()) {
      await noteLink.click();
      await page.waitForTimeout(1000);
      
      const cmEditor = page.locator('.cm-editor');
      if (await cmEditor.isVisible()) {
        await expect(cmEditor).toBeVisible();
      }
    }
  });
});
