import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady } from './helpers/nav';

test('Files toolbar creates folders, sorts, refreshes and opens management settings', async ({ page }) => {
  await setupBasicMocks(page);
  let made = false;
  let reads = 0;
  await page.route('**/api/rpc/fs.list_dir', route => {
    reads++;
    return route.fulfill({ json: { entries: [
      { name: 'a.md', rel_path: 'a.md', is_dir: false, modified: 1, size: 10, status: null },
      { name: 'z.md', rel_path: 'z.md', is_dir: false, modified: 9, size: 10, status: null },
      ...(made ? [{ name: 'New chapter', rel_path: 'New chapter', is_dir: true, modified: 10, size: 0, status: null }] : []),
    ], truncated: false } });
  });
  await page.route('**/api/rpc/fs.mkdir', route => {
    expect(route.request().postDataJSON().rel_path).toBe('New chapter');
    made = true;
    return route.fulfill({ json: { created: true } });
  });
  await page.goto('/');
  await appReady(page);
  await page.getByTestId('root-dropdown').click();
  await page.getByTestId('root-dropdown-popout').getByRole('option').filter({ hasText: 'my-kiln' }).first().click();
  const tree = page.getByRole('tree', { name: 'File tree' });
  await expect(tree.locator('[data-part="item"]')).toHaveCount(2);
  await page.getByRole('button', { name: 'Sort', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Modified time' }).click();
  await expect(tree.locator('[data-part="item"]').first()).toContainText('z');
  page.once('dialog', dialog => dialog.accept('New chapter'));
  await page.getByRole('button', { name: 'New folder', exact: true }).click();
  await expect(tree.getByText('New chapter', { exact: true })).toBeVisible();
  const before = reads;
  await page.getByRole('button', { name: 'More file actions' }).click();
  await page.getByRole('menuitem', { name: 'Refresh', exact: true }).click();
  await expect.poll(() => reads).toBeGreaterThan(before);
  await page.getByRole('button', { name: 'More file actions' }).click();
  await page.getByRole('menuitem', { name: 'Manage projects and kilns…' }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
});
