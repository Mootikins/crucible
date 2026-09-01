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
  //
  // NOT `force`. The comment that used to justify it blamed the trigger's own
  // chevron, which never animated — it computes to `animation: none`. The
  // motion is the PANEL: the right edge opens with a 200ms rAF tween
  // (EdgePanel.tsx) that `disableAnimations` cannot reach, because that helper
  // only zeroes CSS durations and the tween is deliberately JS.
  //
  // `toBeVisible` above goes true on the tween's first frame, and for roughly
  // seven more frames a click at the trigger's own centre lands on the centre
  // pane instead. Playwright's stability and hit-target checks wait exactly
  // that out — and `force` is the flag that turns both of them off. It did not
  // force past the animation; it stepped into it, which is why this spec failed
  // about one run in ten even at `--workers=1`.
  await page.click('[data-testid="root-dropdown"]');
  await page.click('[role="listbox"] :text-is("other-repo")');

  // The tree must show the picked project's contents. Named WITHOUT the
  // extension: the panel's `.ext` toggle defaults to on and hides `.md`, so
  // the row for `README.md` reads "README". Asserting the on-disk name here
  // tested the display preference rather than the pick, and broke when that
  // preference gained its default.
  await expect(page.locator('[role="treeitem"]:has-text("README")')).toBeVisible({
    timeout: 10_000,
  });
  await expect(page.locator('[role="treeitem"]:has-text("src")')).toBeVisible();

  // The dropdown IS the current-root display, so its own trigger names the
  // browsed root. Nothing else in the panel shows a root: the strip of tabs
  // that used to sit beside it is gone.
  await expect(page.locator('[data-testid="root-dropdown"]')).toContainText('other-repo');
  await expect(page.locator('[data-testid^="root-tab-"]')).toHaveCount(0);

  // Reopening lists it as browse-only — this session works in a different
  // project, so browsing it never let the agent read it.
  await page.click('[data-testid="root-dropdown"]');
  await expect(
    page.locator('[role="option"]:has-text("other-repo")'),
  ).toContainText('browse only');
});
