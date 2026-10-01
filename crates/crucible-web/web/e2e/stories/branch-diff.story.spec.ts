import { test, expect, type Request } from '@playwright/test';
import { createStory } from './_helpers/story';
import { setupBasicMocks } from '../helpers/mock-api';
import { appReady, openSession, openFilesPanel } from '../helpers/nav';
import { MOCK_PROJECT, MOCK_SESSION } from '../helpers/fixtures';

/**
 * Story: the user opens the branch diff of the browsed root.
 *
 * The session works in a project at the top level of a git repository. The
 * Files panel header therefore shows "Open branch diff". A click opens a diff
 * tab for the working tree against the default branch. The daemon names the
 * default branch, so the request sends no base and no head.
 */

const ROOT = MOCK_PROJECT.path;

test.describe('Branch diff from the Files panel', () => {
  test('opens the branch diff from the Files panel header', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await setupBasicMocks(page, {
      projects: [{ ...MOCK_PROJECT, repository: { root: ROOT, is_worktree: false } }],
    });

    const asked: Request[] = [];
    page.on('request', (request: Request) => {
      const url = new URL(request.url());
      if (url.pathname === '/api/rpc/diff.get') asked.push(request);
    });

    await page.goto('/');
    await appReady(page);
    // The tree follows the active session, and the session works in the project.
    await openSession(page, MOCK_SESSION.session_id);

    // Open the Files tab in the right edge panel through the store. The
    // pointer path races the JS tween of the panel (see root-dropdown-pick).
    await openFilesPanel(page);

    const open = page.getByTestId('open-branch-diff');
    await expect(open).toBeVisible();
    await story.step(page, 'the git root shows Open branch diff');

    await open.click();

    await expect(page.getByTestId('diff-source')).toContainText('Working tree');
    await expect(page.getByTestId(`diff-file-${ROOT}:src/lib.rs`)).toBeVisible();
    await expect(page.getByTestId(`diff-file-${ROOT}:README.md`)).toBeVisible();
    await expect(page.getByTestId(`diff-file-${ROOT}:src/server.rs`)).toBeVisible();
    await story.step(page, 'the branch diff lists the changed files');

    // One request, for the browsed root, with the default base and the working tree.
    expect(asked.length).toBeGreaterThan(0);
    const { source } = asked[0].postDataJSON() as { source: { kind: string; root: string; base: string; head: string | null } };
    expect(source.kind).toBe('branch');
    expect(source.root).toBe(ROOT);
    expect(source.base).toBe('');
    expect(source.head).toBeNull();
  });
});
