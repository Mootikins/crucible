import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady, openSession } from './helpers/nav';
import { MOCK_PROJECT, MOCK_SESSION } from './helpers/fixtures';

/**
 * The change bar of each file lines up with the row it marks.
 *
 * The bar is the 3 px gutter of `@codemirror/merge`. CodeMirror places a
 * gutter element from its height map, and lays out the rows in normal flow.
 * The two agree only while the height map knows the height of every block.
 * Two faults break that:
 *
 * - `getBoundingClientRect` leaves a margin out, so a block widget with a
 *   vertical margin makes every bar below it ride high by that margin — see
 *   `.cm-diff-comment` in `components/diff-comments.tsx`.
 * - CodeMirror measures the rows of a view when the window shows it, and
 *   keeps an estimate of 14 px a row until then. A file that mounts under
 *   the fold is therefore measured when the user scrolls to it, as T3 Code
 *   measures a diff file only near the viewport.
 *
 * This spec measures every file of the mock diffset: `src/lib.rs` and
 * `src/server.rs` at the top of the pane, and `README.md`, which mounts under
 * the fold. The spec scrolls to `README.md` before it measures that file.
 *
 * jsdom has no layout, so a vitest unit cannot see this. A browser can.
 */

const ROOT = MOCK_PROJECT.path;
const FILE = `diff-file-${ROOT}:src/server.rs`;
/** The file that the mock diffset leaves under the fold. */
const UNDER_FOLD = `${ROOT}:README.md`;
/** The files this spec measures, by their section key. */
const MEASURED = [`${ROOT}:src/lib.rs`, `${ROOT}:src/server.rs`, UNDER_FOLD];

/** One row of the diff and the bar that marks it, in page coordinates. */
interface Pair {
  file: string;
  row: { top: number; bottom: number };
  bar: { top: number; bottom: number };
}

/**
 * Each marked row of each open file, with its bar.
 *
 * The bars and the rows come from one range set, so the nth bar of a file
 * marks its nth changed or removed row.
 */
async function pairs(page: Page): Promise<Pair[]> {
  return page.evaluate((keys: string[]) => {
    const out: Pair[] = [];
    for (const key of keys) {
      const section = document.querySelector<HTMLElement>(`section[data-file-key="${key}"]`);
      if (!section) continue;
      const box = (el: Element) => {
        const r = el.getBoundingClientRect();
        return { top: r.top, bottom: r.bottom };
      };
      for (const editor of section.querySelectorAll<HTMLElement>('.cm-editor')) {
        const bars = [
          ...editor.querySelectorAll('.cm-changedLineGutter, .cm-deletedLineGutter'),
        ].map(box);
        const rows = [...editor.querySelectorAll('.cm-content > .cm-changedLine, .cm-deletedChunk')]
          .filter((el) => el.closest('.cm-content'))
          .map(box);
        const file = section.dataset.fileKey ?? '';
        for (let i = 0; i < Math.min(bars.length, rows.length); i++) {
          out.push({ file, row: rows[i], bar: bars[i] });
        }
      }
    }
    return out;
  }, MEASURED) as Promise<Pair[]>;
}

/**
 * The pairs, once they stop moving.
 *
 * A web font swaps, and a section mounts when it comes near the viewport.
 * Each of those makes CodeMirror measure again, so a reading taken too early
 * shows a layout that no one sees. Two equal readings mean the layout
 * settled. The misalignment this file guards is static, so it survives the
 * wait.
 */
async function settled(page: Page): Promise<Pair[]> {
  await page.evaluate(() => document.fonts.ready);
  let last = '';
  await expect
    .poll(async () => {
      const now = JSON.stringify(await pairs(page));
      const same = now === last && now !== '[]';
      last = now;
      return same;
    })
    .toBe(true);
  return pairs(page);
}

/** Every bar covers its row: the same top and the same bottom, to the pixel. */
function expectAligned(found: Pair[], atLeast: number): void {
  expect(found.length).toBeGreaterThanOrEqual(atLeast);
  const off = found
    .filter((p) => Math.abs(p.bar.top - p.row.top) > 1 || Math.abs(p.bar.bottom - p.row.bottom) > 1)
    .map((p) => `${p.file}: bar ${p.bar.top}–${p.bar.bottom} row ${p.row.top}–${p.row.bottom}`);
  expect(off).toEqual([]);
}

/**
 * True while the window shows no part of the section of this file.
 *
 * The estimated heights belong to a view that the window does not show. The
 * case below is worthless if the pane grew tall enough to show this file.
 */
async function underTheFold(page: Page, key: string): Promise<boolean> {
  return page.evaluate((k: string) => {
    const section = document.querySelector<HTMLElement>(`section[data-file-key="${k}"]`);
    if (!section) throw new Error(`no section ${k}`);
    return section.getBoundingClientRect().top >= window.innerHeight;
  }, key);
}

/**
 * Hold the text of one file back until the files above it filled the pane.
 *
 * The daemon answers one request for each file, and the answers can arrive in
 * any order. An editor that mounts while the pane is still short mounts in
 * the window, and CodeMirror measures it there. The case below needs the
 * other order: the pane is already taller than the window when this editor
 * mounts, so the window never shows it.
 */
async function textArrivesLast(page: Page, path: string): Promise<void> {
  await page.route('**/api/diff/file**', async (route) => {
    if (new URL(route.request().url()).searchParams.get('path') === path) {
      await new Promise((done) => setTimeout(done, 500));
    }
    await route.fallback();
  });
}

/** Open the Files tab of the right edge panel through the store. */
async function openFilesPanel(page: Page): Promise<void> {
  await page.evaluate(() => {
    const store = (window as unknown as Record<string, any>).__windowStore;
    const actions = (window as unknown as Record<string, any>).__windowActions;
    if (store.edgePanels?.right?.mode !== 'docked') actions.toggleEdgePanel('right');
    const firstGroup = (node: any): string | null => {
      if (!node || typeof node !== 'object') return null;
      if (node.type === 'pane') return node.tabGroupId ?? null;
      return firstGroup(node.first) ?? firstGroup(node.second);
    };
    const groupId = firstGroup(store.edgePanels.right.layout);
    if (!groupId) throw new Error('right edge panel has no tab group');
    actions.setActiveTab(groupId, 'files-tab');
  });
  await expect(page.getByTestId('root-dropdown')).toBeVisible();
}

/** Select one line of `src/server.rs` and store a comment on it. */
async function commentOnLine(page: Page, line: number, text: string): Promise<void> {
  const number = page.getByTestId(FILE).locator(`[data-testid="diff-line-${line}"]`);
  await number.scrollIntoViewIfNeeded();
  const at = await number.boundingBox();
  if (!at) throw new Error(`no line number ${line}`);
  await page.mouse.move(at.x + at.width / 2, at.y + at.height / 2);
  await page.mouse.down();
  await page.mouse.up();
  const box = page.getByTestId('diff-comment-box');
  await expect(box).toBeVisible();
  await box.getByTestId('diff-comment-input').fill(text);
  await box.getByTestId('diff-comment-submit').click();
  await expect(page.getByTestId(FILE).getByTestId('diff-comment')).toContainText(text);
}

/** The whole case: open the branch diff, wrap it, then comment in it. */
async function everyBarCoversItsRow(page: Page): Promise<void> {
  await setupBasicMocks(page, {
    projects: [{ ...MOCK_PROJECT, repository: { root: ROOT, is_worktree: false } }],
  });
  await textArrivesLast(page, 'README.md');

  // A narrow window makes the centre pane narrow, so the long lines of
  // `src/server.rs` wrap. The size is set before the first paint, so the
  // editor mounts at its final width and CodeMirror measures it once.
  await page.setViewportSize({ width: 900, height: 800 });
  await page.goto('/');
  await appReady(page);
  await openSession(page, MOCK_SESSION.session_id);
  await openFilesPanel(page);
  await page.getByTestId('open-branch-diff').click();
  await expect(page.getByTestId(FILE)).toBeVisible();
  await expect(page.getByTestId(FILE).locator('.cm-changedLineGutter').first()).toBeVisible();
  // The held-back editor. `toBeAttached` does not scroll, so the file stays
  // under the fold.
  await expect(page.getByTestId(`diff-file-${UNDER_FOLD}`).locator('.cm-editor')).toBeAttached();

  // A wrapped row is taller than one line, so its bar must be taller too. A
  // removed row is a block widget of its own, and it is in the count.
  // A row taller than one line has wrapped. The case is worthless without it.
  const wrapped = await settled(page);
  expect(wrapped.some((p) => p.row.bottom - p.row.top > 30)).toBe(true);
  // `README.md` is not measured yet: the window has not shown it.
  expectAligned(
    wrapped.filter((p) => p.file !== UNDER_FOLD),
    6,
  );

  // `README.md` mounted under the fold. A user scrolls to it, and CodeMirror
  // measures it when the window shows it.
  expect(await underTheFold(page, UNDER_FOLD)).toBe(true);
  await page.getByTestId(`diff-file-${UNDER_FOLD}`).scrollIntoViewIfNeeded();
  const scrolled = await settled(page);
  expect(scrolled.filter((p) => p.file === UNDER_FOLD).length).toBeGreaterThanOrEqual(3);
  expectAligned(scrolled, 9);

  // A comment is a block widget between two rows. Every bar below it must
  // still cover its row.
  await commentOnLine(page, 17, 'The timeout is now 10 seconds. Why?');
  expectAligned(await settled(page), 9);
}

/** The stored theme preference, which the shell reads before its first paint. */
function storedTheme(value: 'dark' | 'light') {
  const port = process.env.CRUCIBLE_WEB_PORT ?? '5273';
  return {
    cookies: [],
    origins: [
      {
        origin: new URL(`http://localhost:${port}`).origin,
        localStorage: [{ name: 'crucible:theme', value }],
      },
    ],
  };
}

test.describe('The change bar of the diff pane, dark', () => {
  test.use({ storageState: storedTheme('dark'), colorScheme: 'dark' });
  test('covers its row, also under a comment and beside a wrapped line', async ({ page }) => {
    await everyBarCoversItsRow(page);
  });
});

// The geometry must not depend on the palette: the light theme brings its own
// editor theme, so it gets its own run.
test.describe('The change bar of the diff pane, light', () => {
  test.use({ storageState: storedTheme('light'), colorScheme: 'light' });
  test('covers its row, also under a comment and beside a wrapped line', async ({ page }) => {
    await everyBarCoversItsRow(page);
  });
});
