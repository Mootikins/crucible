import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady } from './helpers/nav';

/**
 * E2E: the Files root dropdown must re-root the tree on pick.
 *
 * Two regressions lived here. Picking a registered PROJECT that was not the
 * session's workspace pinned a key `resolveSessionRoot` could never resolve
 * (projects were absent from the browsable root set), so the click silently
 * did nothing. And with NO session active, `selectRoot` dropped the pick
 * entirely. Both must browse: the roster offers every project, a pick that
 * does nothing reads as broken.
 */

test('picking a non-workspace project from the root dropdown browses it', async ({ page }) => {
  await setupBasicMocks(page, {
    // Two projects; the mock session's workspace is neither.
    projects: [
      { path: '/home/user/workspace-repo', name: 'workspace-repo', kilns: [], last_accessed: '2026-01-01T00:00:00Z' },
      { path: '/home/user/other-repo', name: 'other-repo', kilns: [], last_accessed: '2026-01-01T00:00:00Z' },
    ],
  });

  // The tree fetches directories from /api/fs/list.
  await page.route('**/api/fs/list**', (route) => {
    const url = new URL(route.request().url());
    const root = url.searchParams.get('root') ?? '';
    if (root === '/home/user/other-repo' && !url.searchParams.get('rel_path')) {
      return route.fulfill({
        json: {
          entries: [
            { name: 'src', rel_path: 'src', is_dir: true, size: 0, modified: 0, status: null },
            { name: 'README.md', rel_path: 'README.md', is_dir: false, size: 10, modified: 0, status: null },
          ],
          truncated: false,
        },
      });
    }
    return route.fulfill({ json: { entries: [], truncated: false } });
  });

  await page.goto('/');
  await appReady(page);

  // Open the Files edge tab through the store — the same actions the tab's own
  // click handler and the ribbon chevron call (collapsed/expanded chevrons
  // animate, which makes pointer-driven opening flaky in a 30s budget).
  // The Files tab ships in the RIGHT edge panel.
  await page.evaluate(() => {
    const store = (window as unknown as Record<string, any>).__windowStore;
    const actions = (window as unknown as Record<string, any>).__windowActions;
    if (store.edgePanels?.right?.isCollapsed) actions.toggleEdgePanel('right');
    const firstGroup = (node: any): string | null => {
      if (!node || typeof node !== 'object') return null;
      if (node.type === 'pane') return node.tabGroupId ?? null;
      return firstGroup(node.first) ?? firstGroup(node.second);
    };
    const groupId = firstGroup(store.edgePanels.right.layout);
    if (!groupId) throw new Error('right edge panel has no tab group');
    actions.setActiveTab(groupId, 'files-tab');
  });
  await expect(page.locator('[data-testid="root-dropdown"]')).toBeVisible();

  // Open the Files panel's root dropdown and pick the non-workspace project.
  // The trigger chevron animates on mount; force past the stability check —
  // the assertion is about the pick's effect, not the click's precision.
  await page.click('[data-testid="root-dropdown"]', { force: true });
  await page.click('[role="listbox"] :text-is("other-repo")');

  // The tree must show the picked project's contents.
  await expect(page.locator('[role="treeitem"]:has-text("README.md")')).toBeVisible({
    timeout: 10_000,
  });

  // And the strip names the browsed root, dimmed as browse-only.
  const stripTab = page.locator('[data-testid^="root-tab-"]:has-text("other-repo")');
  await expect(stripTab).toBeVisible();
  await expect(stripTab).toHaveAttribute('data-origin', 'other-project');
});
