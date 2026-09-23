import { test, expect, type Locator, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady, openBranchDiff, openSession } from './helpers/nav';
import { MOCK_PROJECT, MOCK_SESSION } from './helpers/fixtures';

/**
 * A drag over the text of the diff opens the comment box, as a drag over the
 * line numbers does.
 *
 * CodeMirror selects the text, and the drag takes the whole lines of its two
 * ends. A removed row counts: at the top end of the drag it takes the line
 * above its chunk, at the bottom end the line under it, so the chunk is in the
 * range. A wrapped line is one line. During the drag the browser selection
 * hides, so only the tint shows the range. Cancel selects the whole lines
 * of the range, for a copy: a comment range never holds part of a line.
 *
 * In `src/server.rs`, current line 31 is `let listener = …`, the removed row
 * `axum::serve(listener, routes()).await` sits above current line 32, and
 * current line 33 is `axum::serve(listener, routes())`.
 *
 * jsdom has no layout, so CodeMirror cannot select text there. A browser can.
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
  await expect(file.locator('.cm-deletedChunk').first()).toBeVisible();
  return file;
}

/** The row of the current side that holds this text. */
const line = (file: Locator, text: string) => file.locator('.cm-line', { hasText: text });
/** The removed row that holds this text. */
const removed = (file: Locator, text: string) =>
  file.locator('.cm-deletedChunk .cm-deletedLine', { hasText: text });

/**
 * Waits until the row stops moving. CodeMirror measures the rows again after
 * a scroll, and a web font changes the wrap, so a box read too early is not
 * where the row ends up.
 */
async function settled(row: Locator): Promise<void> {
  let last = '';
  await expect
    .poll(async () => {
      const now = JSON.stringify(await row.boundingBox());
      const same = now === last;
      last = now;
      return same;
    })
    .toBe(true);
}

/** A point in the text of a row: `x` from its left edge, `y` from its top. */
async function point(row: Locator, x = 40, y = 8): Promise<{ x: number; y: number }> {
  await row.scrollIntoViewIfNeeded();
  await settled(row);
  const box = await row.boundingBox();
  if (!box) throw new Error('the row has no layout');
  return { x: box.x + x, y: box.y + y };
}

/** A press at `from`, and a move to `to` in steps, as a hand moves. The button stays down. */
async function dragTo(page: Page, from: Locator, to: Locator): Promise<void> {
  const a = await point(from);
  const b = await point(to, 60);
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  await page.mouse.move(b.x, b.y, { steps: 6 });
}

/** The background of the browser selection on a row. */
const selectionColor = (row: Locator) =>
  row.evaluate((el) => getComputedStyle(el, '::selection').backgroundColor);

/** True while the base number of this removed row is in the range. */
const baseChosen = (file: Locator, n: number) =>
  file
    .locator(`[data-testid="diff-base-line-${n}"]`)
    .evaluate(
      (el) => !!el.closest('.cm-gutterElement')?.classList.contains('cm-diff-selected-number'),
    );

test('a drag over the text tints whole lines, hides the selection, and opens the box', async ({
  page,
}) => {
  const file = await openDiff(page);
  const first = line(file, 'let listener');
  await dragTo(page, first, line(file, 'axum::serve(listener, routes())'));

  // During the drag: three whole lines and the removed row between them.
  await expect(file.locator('.cm-line.cm-diff-selected')).toHaveCount(3);
  expect(await baseChosen(file, 32)).toBe(true);
  expect(await selectionColor(first)).toBe('rgba(0, 0, 0, 0)');
  await expect(file.getByTestId('diff-comment-box')).toHaveCount(0);

  await page.mouse.up();
  await expect(file.getByTestId('diff-comment-box')).toContainText('Lines 31-33');
  await expect(file.getByTestId('diff-comment-input')).toBeFocused();
});

test('a click in the text opens no box', async ({ page }) => {
  const file = await openDiff(page);
  const at = await point(line(file, 'let listener'));
  await page.mouse.click(at.x, at.y);
  await expect(file.getByTestId('diff-comment-box')).toHaveCount(0);
  await expect(file.locator('.cm-line.cm-diff-selected')).toHaveCount(0);
});

test('a removed row at an end of the drag brings its chunk into the range', async ({ page }) => {
  const file = await openDiff(page);
  // Down onto the removed row: the range ends at the line under its chunk.
  await dragTo(page, line(file, '/// Serves the routes'), removed(file, 'axum::serve'));
  await page.mouse.up();
  const box = file.getByTestId('diff-comment-box');
  await expect(box).toContainText('Lines 29-32');
  expect(await baseChosen(file, 32)).toBe(true);
  await box.getByTestId('diff-comment-cancel').click();

  // Up onto the removed row: the range starts at the line above its chunk.
  await dragTo(page, line(file, 'axum::serve(listener, routes())'), removed(file, 'axum::serve'));
  await page.mouse.up();
  await expect(box).toContainText('Lines 31-33');
  expect(await baseChosen(file, 32)).toBe(true);
});

test('a drag across the rows of one wrapped line takes one line', async ({ page }) => {
  // A narrow window wraps the long line 32. A narrower one shows the phone shell.
  await page.setViewportSize({ width: 800, height: 800 });
  const file = await openDiff(page);
  const wrapped = line(file, 'tracing::info!');
  // In the middle of the window: near an edge, CodeMirror scrolls during the
  // drag, and the drag then ends on another line. CodeMirror measures the
  // rows again after a scroll, and the font changes the wrap, so the case
  // waits until the row stops moving.
  await page.evaluate(() => document.fonts.ready);
  await wrapped.evaluate((el) => el.scrollIntoView({ block: 'center' }));
  await settled(wrapped);
  const one = await line(file, '#[cfg(test)]').boundingBox();
  const box = await wrapped.boundingBox();
  if (!one || !box) throw new Error('the rows have no layout');
  // The case is worthless unless the line wraps.
  expect(box.height).toBeGreaterThan(one.height * 1.5);

  await page.mouse.move(box.x + 40, box.y + 6);
  await page.mouse.down();
  await page.mouse.move(box.x + 60, box.y + box.height - 6, { steps: 6 });
  await page.mouse.up();
  await expect(file.getByTestId('diff-comment-box')).toContainText('Line 32');
  await expect(file.getByTestId('diff-comment-box')).not.toContainText('Lines');
});

test('Cancel selects the whole lines of the range, for a copy', async ({ page }) => {
  const file = await openDiff(page);
  const first = line(file, 'let listener');
  await dragTo(page, first, line(file, 'axum::serve(listener, routes())'));
  await page.mouse.up();
  await expect(file.getByTestId('diff-comment-input')).toBeFocused();
  await page.keyboard.press('Escape');

  await expect(file.getByTestId('diff-comment-box')).toHaveCount(0);
  await expect(file.locator('.cm-line.cm-diff-selected')).toHaveCount(0);
  const text = await page.evaluate(() => window.getSelection()?.toString() ?? '');
  // The drag started inside line 31 and ended inside line 33. The selection
  // holds lines 31 to 33 from end to end, and the removed row between them.
  expect(text).toMatch(/^    let listener = TcpListener::bind/);
  expect(text).toContain('tracing::info!');
  expect(text).toMatch(/axum::serve\(listener, routes\(\)\)$/);
  // The browser draws the selection again.
  expect(await selectionColor(first)).not.toBe('rgba(0, 0, 0, 0)');
});

test('a drag over the text while a box is open only selects text', async ({ page }) => {
  const file = await openDiff(page);
  await dragTo(page, line(file, 'let listener'), line(file, 'tracing::info!'));
  await page.mouse.up();
  const box = file.getByTestId('diff-comment-box');
  await expect(box).toContainText('Lines 31-32');
  await box.getByTestId('diff-comment-input').fill('a draft that must stay');

  await dragTo(page, line(file, '/// Serves the routes'), line(file, 'pub async fn serve'));
  await page.mouse.up();
  await expect(box).toHaveCount(1);
  await expect(box).toContainText('Lines 31-32');
  await expect(box.getByTestId('diff-comment-input')).toHaveValue('a draft that must stay');
});

test('Cancel after a drag over the line numbers selects no text', async ({ page }) => {
  const file = await openDiff(page);
  const number = file.locator('[data-testid="diff-line-31"]');
  await number.scrollIntoViewIfNeeded();
  const at = await number.boundingBox();
  if (!at) throw new Error('no line number 31');
  await page.mouse.move(at.x + at.width / 2, at.y + at.height / 2);
  await page.mouse.down();
  await page.mouse.up();
  await expect(file.getByTestId('diff-comment-input')).toBeFocused();
  await page.keyboard.press('Escape');

  await expect(file.getByTestId('diff-comment-box')).toHaveCount(0);
  expect(await page.evaluate(() => window.getSelection()?.toString() ?? '')).toBe('');
});
