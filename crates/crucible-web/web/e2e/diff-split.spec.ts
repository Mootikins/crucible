import { test, expect, type Locator, type Page } from '@playwright/test';
import { setupBasicMocks, type MockOverrides } from './helpers/mock-api';
import { appReady, openBranchDiff, openSession } from './helpers/nav';
import { MOCK_PROJECT, MOCK_SESSION } from './helpers/fixtures';

/**
 * The split view of the diff pane: the base on the left, the current text on
 * the right, and each hunk header in both editors.
 *
 * The hunk headers count the lines of the other side. The editor of the
 * current side has no unified view, so it cannot read the base text from
 * one. A click on Split threw "Field is not present in this state" there,
 * and the pane stayed unified.
 */

const ROOT = MOCK_PROJECT.path;
const FILE = `diff-file-${ROOT}:src/server.rs`;

/** Mocks the API. The case reads the stored comments from the result. */
const mocks = (page: Page, over: MockOverrides = {}) =>
  setupBasicMocks(page, {
    projects: [{ ...MOCK_PROJECT, repository: { root: ROOT, is_worktree: false } }],
    ...over,
  });

/** A file that the change deletes: three base lines, no current text. */
const DELETED = {
  diffFiles: [
    {
      path: 'src/old.rs',
      status: { kind: 'deleted' },
      added: 0,
      removed: 3,
      binary: false,
      too_large: false,
    },
  ],
  diffTexts: { 'src/old.rs': { base_text: 'one\ntwo\nthree\n', current_text: null } },
};
const OLD = `diff-file-${ROOT}:src/old.rs`;

/**
 * Opens the branch diff, waits for the editor of the section `key`, then
 * clicks Split. Each page error goes into `errors`.
 */
async function openSplit(page: Page, errors: string[], key = FILE): Promise<Locator> {
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/');
  await appReady(page);
  await openSession(page, MOCK_SESSION.session_id);
  await openBranchDiff(page);
  const file = page.getByTestId(key);
  await expect(file.locator('.cm-editor')).toHaveCount(1);
  await page.getByTestId('diff-layout-split').click();
  return file;
}

test('a click on Split shows both sides, with the hunk headers in each', async ({ page }) => {
  const errors: string[] = [];
  await mocks(page);
  const file = await openSplit(page, errors);

  await expect(file.locator('.cm-merge-a')).toBeVisible();
  await expect(file.locator('.cm-merge-b')).toBeVisible();
  const labels = ['−14,7 +14,7', '−29,7 +29,10'];
  for (const side of ['.cm-merge-a', '.cm-merge-b']) {
    await expect(file.locator(side).getByTestId('diff-hunk-toggle')).toHaveText(labels);
  }
  expect(errors).toEqual([]);
});

test('a drag over the base text of the split view comments on the base side', async ({ page }) => {
  const errors: string[] = [];
  const api = await mocks(page);
  const file = await openSplit(page, errors);
  const base = file.locator('.cm-merge-a');
  await expect(base).toBeVisible();

  // Base lines 30 to 32. Line 32 is removed, and in the split view it is a
  // line of the base editor, not a removed row.
  const from = await base.locator('.cm-line', { hasText: 'pub async fn serve' }).boundingBox();
  const to = await base
    .locator('.cm-line', { hasText: 'axum::serve(listener, routes()).await' })
    .boundingBox();
  if (!from || !to) throw new Error('the base rows have no layout');
  await page.mouse.move(from.x + 60, from.y + 8);
  await page.mouse.down();
  await page.mouse.move(to.x + 90, to.y + 8, { steps: 6 });
  await expect(base.locator('.cm-line.cm-diff-selected')).toHaveCount(3);
  await page.mouse.up();

  const box = base.getByTestId('diff-comment-box');
  await expect(box).toContainText('Lines 30-32');
  await box.getByTestId('diff-comment-input').fill('Why does this call go?');
  await box.getByTestId('diff-comment-submit').click();
  await expect(base.getByTestId('diff-comment')).toContainText('Why does this call go?');
  expect(api.comments.map((c) => [c.side, c.line_range])).toEqual([
    ['base', { start: 30, end: 33 }],
  ]);
  expect(errors).toEqual([]);
});

/**
 * An empty side has no line. CodeMirror gives an empty text one empty line,
 * and that line took the number 1 and made the range of the side `1,1`.
 */
test.describe('an empty side shows no line and counts none', () => {
  test('an added file, split: no base number, and the range -0,0', async ({ page }) => {
    const errors: string[] = [];
    await mocks(page);
    await openSplit(page, errors);
    const added = page.getByTestId(`diff-file-${ROOT}:README.md`);
    await added.scrollIntoViewIfNeeded();
    await expect(added.locator('.cm-merge-a')).toBeVisible();
    for (const side of ['.cm-merge-a', '.cm-merge-b']) {
      await expect(added.locator(side).getByTestId('diff-hunk-toggle')).toHaveText([
        '−0,0 +1,3',
      ]);
    }
    await expect(added.locator('.cm-merge-a [data-testid^="diff-line-"]')).toHaveCount(0);
    await expect(added.locator('.cm-merge-b [data-testid^="diff-line-"]')).toHaveCount(3);
    expect(errors).toEqual([]);
  });

  test('a deleted file, unified: no current number, and the range +0,0', async ({ page }) => {
    await mocks(page, DELETED);
    await page.goto('/');
    await appReady(page);
    await openSession(page, MOCK_SESSION.session_id);
    await openBranchDiff(page);
    const old = page.getByTestId(OLD);
    await expect(old.getByTestId('diff-hunk-toggle')).toHaveText(['−1,3 +0,0']);
    await expect(old.locator('[data-testid^="diff-line-"]')).toHaveCount(0);
    await expect(old.locator('[data-testid^="diff-base-line-"]')).toHaveCount(3);
  });

  test('a deleted file, split: no current number, and the range +0,0', async ({ page }) => {
    const errors: string[] = [];
    await mocks(page, DELETED);
    await openSplit(page, errors, OLD);
    const old = page.getByTestId(OLD);
    await expect(old.locator('.cm-merge-b')).toBeVisible();
    for (const side of ['.cm-merge-a', '.cm-merge-b']) {
      await expect(old.locator(side).getByTestId('diff-hunk-toggle')).toHaveText([
        '−1,3 +0,0',
      ]);
    }
    await expect(old.locator('.cm-merge-a [data-testid^="diff-line-"]')).toHaveCount(3);
    await expect(old.locator('.cm-merge-b [data-testid^="diff-line-"]')).toHaveCount(0);
    expect(errors).toEqual([]);
  });
});
