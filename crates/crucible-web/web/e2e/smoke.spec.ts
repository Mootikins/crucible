import { test, expect } from '@playwright/test';

test.describe('Smoke Tests - Critical User Flows', () => {
  test('complete user flow: project → session → note → edit', async ({ page }) => {
    await page.route('**/api/projects', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([
          {
            name: 'crucible',
            path: '/home/moot/crucible',
            kilns: ['/home/moot/crucible/docs'],
          },
        ]),
      });
    });

    await page.route('**/api/sessions*', async (route) => {
      if (route.request().method() === 'POST') {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            id: 'test-session-1',
            title: 'Test Session',
            state: 'active',
            agent_model: 'test-model',
          }),
        });
      } else {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify([
            {
              id: 'test-session-1',
              title: 'Test Session',
              state: 'active',
              agent_model: 'test-model',
            },
          ]),
        });
      }
    });

    await page.route('**/api/notes*', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([
          { name: 'Index.md', path: '/docs/Index.md', is_dir: false },
          { name: 'README.md', path: '/docs/README.md', is_dir: false },
        ]),
      });
    });

    await page.route('**/api/notes/Index.md', async (route) => {
      if (route.request().method() === 'PUT') {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({ success: true }),
        });
      } else {
        await route.fulfill({
          status: 200,
          contentType: 'text/plain',
          body: '# Index\n\nWelcome to Crucible.',
        });
      }
    });

    await page.goto('/');
    await page.waitForTimeout(500);

    await expect(page.locator('text=Projects')).toBeVisible();

    const projectButton = page.getByText('/home/moot/crucible').first();
    await expect(projectButton).toBeVisible();
    await projectButton.click();
    await page.waitForTimeout(500);

    await expect(page.locator('text=Sessions')).toBeVisible();

    const createSessionButton = page.locator('text=+ New Session');
    await expect(createSessionButton).toBeVisible();
    await createSessionButton.click();
    await page.waitForTimeout(1000);

    await expect(page.locator('text=Notes')).toBeVisible();

    const noteLink = page.locator('text=Index.md').first();
    await expect(noteLink).toBeVisible();
    await noteLink.click();
    await page.waitForTimeout(1000);

    const editor = page.locator('.cm-content');
    await expect(editor).toBeVisible();
    await editor.click();
    await page.keyboard.type('\n\nEdited content');
    await page.waitForTimeout(500);

    const dirtyIndicator = page.locator('text=●');
    await expect(dirtyIndicator.first()).toBeVisible();
  });

  test('app loads without errors', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (error) => {
      errors.push(error.message);
    });

    await page.goto('/');
    await page.waitForTimeout(1000);

    expect(errors).toHaveLength(0);
  });

  test('displays main UI components', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(500);

    await expect(page.locator('text=Projects')).toBeVisible();
    await expect(page.locator('text=Notes')).toBeVisible();
  });

  test('handles API errors gracefully', async ({ page }) => {
    await page.route('**/api/projects', async (route) => {
      await route.fulfill({
        status: 500,
        contentType: 'application/json',
        body: JSON.stringify({ error: 'Internal server error' }),
      });
    });

    await page.goto('/');
    await page.waitForTimeout(1000);

    await expect(page.locator('text=Projects')).toBeVisible();
  });

  test('responsive layout renders correctly', async ({ page }) => {
    await page.setViewportSize({ width: 1920, height: 1080 });
    await page.goto('/');
    await page.waitForTimeout(500);

    await expect(page.locator('text=Projects')).toBeVisible();
    await expect(page.locator('text=Notes')).toBeVisible();

    await page.setViewportSize({ width: 1280, height: 720 });
    await page.waitForTimeout(500);

    await expect(page.locator('text=Projects')).toBeVisible();
  });
});
