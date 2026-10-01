import { openSession } from './helpers/nav';
import { MOCK_SESSION } from './helpers/fixtures';
import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';

test.beforeEach(async ({ page }) => {
  await setupBasicMocks(page);
  await page.route('**/api/rpc/fs.list_dir', route => {
    const { root, rel_path } = route.request().postDataJSON();
    const entries = rel_path === 'folder' ? [{ name: 'target.md', rel_path: 'folder/target.md', is_dir: false, size: 12, modified: null, status: null }]
      : [{ name: 'folder', rel_path: 'folder', is_dir: true, size: 0, modified: null, status: null }];
    return route.fulfill({ json: { root, entries, truncated: false } });
  });
  await page.route('**/api/kiln/file?*', route => route.fulfill({ json: { content: '# Test file', content_hash: 'fixture' } }));
  await page.goto('/');
  await openSession(page, MOCK_SESSION.session_id);
  await expect(page.getByTestId('layout-menu')).toBeVisible();
});

test('file tab menu reveals an inactive file through lazy folders', async ({ page }) => {
  await page.evaluate(async () => {
    const { windowActions, windowStore } = await import('/src/stores/windowStore.ts');
    const group = windowStore.layout.tabGroupId;
    windowActions.addTab(group, { id: 'reveal-target', title: 'target.md', contentType: 'file', metadata: { filePath: '/home/user/project/folder/target.md' } });
    windowActions.addTab(group, { id: 'other', title: 'Other', contentType: 'search' });
    windowActions.setActiveTab(group, 'other');
    windowActions.setEdgePanelCollapsed('left', true);
  });
  await page.locator('[data-tab-id="reveal-target"]').click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Show in file tree', exact: true }).click();
  const row = page.getByRole('treeitem', { name: 'target', exact: true });
  await expect(row).toBeVisible();
  await expect(row).toBeFocused();
});


test('tree clicks keep the current file and open another editor tab', async ({ page }) => {
  await page.evaluate(async () => {
    const { openFileInEditor } = await import('/src/lib/file-actions.ts');
    openFileInEditor('/home/user/project/original.md');
  });
  await page.getByRole('treeitem').filter({ hasText: 'folder' }).first().locator('[data-part="branch-control"]').click();
  await page.getByRole('treeitem', { name: 'target', exact: true }).click();
  await expect(page.locator('[data-tab-id]').filter({ hasText: 'original.md' })).toBeVisible();
  await expect(page.locator('[data-tab-id]').filter({ hasText: 'target.md' })).toBeVisible();
});

test('tree guides remain distinct from the navigation surface with tint and contrast', async ({ page }) => {
  await page.getByRole('treeitem').filter({ hasText: 'folder' }).first().locator('[data-part="branch-control"]').click();
  const guide = page.getByRole('treeitem', { name: 'target', exact: true }).locator('span.block.h-full').first();
  for (const theme of ['dark', 'light']) {
    for (const contrast of [0, 16]) {
      await page.evaluate(({ theme, contrast }) => {
        document.documentElement.dataset.theme = theme;
        document.documentElement.style.setProperty('--mk-contrast', String(contrast));
        document.documentElement.style.setProperty('--mk-nav-tint', '35');
      }, { theme, contrast });
      const colors = await guide.evaluate(el => {
        const actual = getComputedStyle(el).backgroundColor;
        const probe = document.createElement('span');
        probe.style.backgroundColor = 'var(--mk-nav-solid)';
        el.append(probe);
        const surface = getComputedStyle(probe).backgroundColor;
        probe.remove();
        return { actual, surface };
      });
      expect(colors.actual).not.toBe(colors.surface);
    }
  }
});
