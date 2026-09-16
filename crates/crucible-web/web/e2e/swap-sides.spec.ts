import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';

/**
 * The side swap in the app: the app's chord table reaches the layout action,
 * and the fixed rails (Sessions, Files) change sides.
 *
 * The mechanics (the ribbon button, the swap back, Ctrl+\ still splitting)
 * are core specs in e2e/windowing/rails.spec.ts, which drive the harness.
 */

async function boot(page: Page) {
  await setupBasicMocks(page);
  await page.route('**/api/layout', async (route) => {
    if (route.request().method() === 'GET') {
      await route.fulfill({ status: 404, contentType: 'application/json', body: '{}' });
      return;
    }
    await route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
  });
  await page.goto('/');
  await expect(page.getByTestId('ribbon-toggle-left')).toBeVisible({ timeout: 15000 });
}

/** Content types on one side, in strip order. */
async function sideContents(page: Page, side: 'left' | 'right'): Promise<string[]> {
  return page.evaluate((pos) => {
    const store = (window as unknown as Record<string, any>).__windowStore;
    const groups: string[] = [];
    const walk = (n: any): void => {
      if (!n) return;
      if (n.type === 'pane') {
        if (n.tabGroupId) groups.push(n.tabGroupId);
        return;
      }
      walk(n.first);
      walk(n.second);
    };
    walk(store.edgePanels[pos].layout);
    return groups.flatMap((g) =>
      (store.tabGroups[g]?.tabs ?? []).map((t: { contentType: string }) => t.contentType),
    );
  }, side);
}

test.describe('swap side panels', () => {
  test.beforeEach(async ({ page }) => boot(page));

  test('Ctrl+Shift+\\ mirrors the two rails', async ({ page }) => {
    expect(await sideContents(page, 'left')).toContain('sessions');
    expect(await sideContents(page, 'right')).toContain('files');

    await page.keyboard.press('Control+Shift+\\');

    await expect.poll(() => sideContents(page, 'left')).toContain('files');
    expect(await sideContents(page, 'right')).toContain('sessions');
  });
});
