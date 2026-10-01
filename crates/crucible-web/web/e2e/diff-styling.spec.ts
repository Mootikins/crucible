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
 * - Stored comments and the comment box have no surrounding frame.
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
  await file.getByTestId(`diff-line-${line}`).click();
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

test('a comment and the comment box have no surrounding frame', async ({ page }) => {
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
    expect(edges).toEqual(['0px', '0px', '0px', '0px']);
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


test('the file Comment action keeps the source, path and full-file anchor', async ({ page }) => {
  const file = await openDiff(page);
  await file.getByTestId('diff-file-comment').click();
  await file.getByTestId('diff-comment-input').fill('Review this file as a whole.');
  const sent = page.waitForRequest((request) => request.url().endsWith('/api/rpc/diff.comment'));
  await file.getByTestId('diff-comment-submit').click();
  expect((await sent).postDataJSON()).toMatchObject({
    source: { kind: 'branch', root: ROOT },
    path: 'src/server.rs', side: 'current', line_start: 1,
    body: 'Review this file as a whole.',
  });
  await expect(file.getByTestId('diff-comment-input')).toHaveCount(0);
  await expect(file.getByText('Review this file as a whole.', { exact: true })).toBeVisible();
});

test('changed review rows retain explicit plus and minus markers', async ({ page }) => {
  const file = await openDiff(page);
  const marker = (selector: string) => file.locator(selector).first().evaluate(
    (el) => getComputedStyle(el, '::before').content,
  );
  await expect.poll(() => marker('.cm-changedLine')).toBe('"+"');
  await expect.poll(() => marker('.cm-deletedChunk > .cm-deletedLine')).toBe('"−"');
});


test('file titles open the editor while caret folding stays in the review', async ({ page }) => {
  const file = await openDiff(page);
  const toggle = file.getByTestId('diff-file-toggle');
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await file.getByRole('button', { name: 'Open src/server.rs', exact: true }).click();
  const tab = page.locator(`[data-tab-id="tab-file-${ROOT}/src/server.rs"]`);
  await expect(tab).toBeVisible();
  await expect(tab).toHaveAttribute('data-active', '');
  await openBranchDiff(page);
  await expect(file.getByTestId('diff-file-editor')).toBeVisible();
});
