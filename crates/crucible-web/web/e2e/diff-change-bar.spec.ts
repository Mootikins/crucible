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
 * `getBoundingClientRect` leaves a margin out, so a block widget with a
 * vertical margin makes every bar below it ride high by that margin — see
 * `.cm-diff-comment` in `components/diff-comments.tsx`.
 *
 * jsdom has no layout, so a vitest unit cannot see this. A browser can.
 */

const ROOT = MOCK_PROJECT.path;
const FILE = `diff-file-${ROOT}:src/server.rs`;
/**
 * The files this spec measures, by their section key.
 *
 * Both are at the top of the pane, so their editors are on screen and
 * measured. `README.md` is below the fold: its editor keeps the estimated
 * heights of a view that nobody has seen, which is a different subject.
 */
const MEASURED = [`${ROOT}:src/lib.rs`, `${ROOT}:src/server.rs`];

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

  // A wrapped row is taller than one line, so its bar must be taller too. A
  // removed row is a block widget of its own, and it is in the count.
  // A row taller than one line has wrapped. The case is worthless without it.
  const wrapped = await settled(page);
  expect(wrapped.some((p) => p.row.bottom - p.row.top > 30)).toBe(true);
  expectAligned(wrapped, 6);

  // A comment is a block widget between two rows. Every bar below it must
  // still cover its row.
  await commentOnLine(page, 17, 'The timeout is now 10 seconds. Why?');
  expectAligned(await settled(page), 6);
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
