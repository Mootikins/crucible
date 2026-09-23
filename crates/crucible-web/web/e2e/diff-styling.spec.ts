import { test, expect, type Locator, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady, openBranchDiff, openSession } from './helpers/nav';
import { MOCK_PROJECT, MOCK_SESSION } from './helpers/fixtures';

/**
 * The chrome of the diff pane stays quiet and compact.
 *
 * - The fold of unchanged lines is a label between two hairlines, on the
 *   background of the file header, as T3 Code draws its separator. It is not
 *   a grey band.
 * - A hunk header draws no rule of its own. The fold above it, or the file
 *   header, already separates it.
 * - A stored comment and the comment box carry a bar on the left edge, in
 *   the hue of the selection, and no full frame.
 * - The comment box puts the range label in the row of its buttons, so that
 *   the box takes one row less.
 *
 * jsdom computes no style from the theme of the editor, so a browser checks it.
 */

const ROOT = MOCK_PROJECT.path;
const FILE = `diff-file-${ROOT}:src/server.rs`;

async function openDiff(page: Page): Promise<Locator> {
  await setupBasicMocks(page, {
    projects: [{ ...MOCK_PROJECT, repository: { root: ROOT, is_worktree: false } }],
  });
  await page.goto('/');
  await appReady(page);
  await openSession(page, MOCK_SESSION.session_id);
  await openBranchDiff(page);
  const file = page.getByTestId(FILE);
  await expect(file.locator('.cm-collapsedLines').first()).toBeVisible();
  return file;
}

/** A press and a release on one line number: the comment box opens. */
async function commentBox(page: Page, file: Locator, line: number): Promise<Locator> {
  const at = await file.locator(`[data-testid="diff-line-${line}"]`).boundingBox();
  if (!at) throw new Error(`no line number ${line}`);
  await page.mouse.move(at.x + at.width / 2, at.y + at.height / 2);
  await page.mouse.down();
  await page.mouse.up();
  const box = file.getByTestId('diff-comment-box');
  await expect(box).toBeVisible();
  return box;
}

test('the fold of unchanged lines is a label between two hairlines', async ({ page }) => {
  const file = await openDiff(page);
  const fold = await file
    .locator('.cm-collapsedLines')
    .first()
    .evaluate((el) => {
      const own = getComputedStyle(el);
      const before = getComputedStyle(el, '::before');
      const after = getComputedStyle(el, '::after');
      return {
        background: own.backgroundColor,
        rules: [before, after].map((s) => ({ content: s.content, height: s.height })),
      };
    });
  expect(fold.background).toBe('rgba(0, 0, 0, 0)');
  expect(fold.rules).toEqual([
    { content: '""', height: '1px' },
    { content: '""', height: '1px' },
  ]);
});

test('a hunk header draws no rule of its own', async ({ page }) => {
  const file = await openDiff(page);
  const border = await file
    .getByTestId('diff-hunk-toggle')
    .first()
    .evaluate((el) => getComputedStyle(el).borderTopWidth);
  expect(border).toBe('0px');
});

test('a comment and the comment box carry a bar on the left, and no frame', async ({ page }) => {
  const file = await openDiff(page);
  const box = await commentBox(page, file, 17);
  await box.getByTestId('diff-comment-input').fill('Why ten seconds?');
  await box.getByTestId('diff-comment-submit').click();
  const stored = file.getByTestId('diff-comment');
  await expect(stored).toBeVisible();
  await commentBox(page, file, 31);

  for (const card of [stored, file.locator('.cm-diff-comment-box')]) {
    const edges = await card.evaluate((el) => {
      const s = getComputedStyle(el);
      return [s.borderLeftWidth, s.borderTopWidth, s.borderRightWidth, s.borderBottomWidth];
    });
    expect(edges).toEqual(['2px', '0px', '0px', '0px']);
  }
});

test('the comment box puts its range label in the row of its buttons', async ({ page }) => {
  const file = await openDiff(page);
  const box = await commentBox(page, file, 17);
  const label = await box.getByText('Line 17', { exact: true }).boundingBox();
  const submit = await box.getByTestId('diff-comment-submit').boundingBox();
  const input = await box.getByTestId('diff-comment-input').boundingBox();
  if (!label || !submit || !input) throw new Error('the box has no layout');
  // The label is under the text field, level with the buttons.
  expect(label.y).toBeGreaterThan(input.y + input.height);
  expect(Math.abs(label.y + label.height / 2 - (submit.y + submit.height / 2))).toBeLessThan(2);
});
