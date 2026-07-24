import { test, expect, type Page } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady } from './helpers/nav';

/**
 * E2E: live-preview block widgets.
 *
 * Two behaviours that only show up in the editor (the reading view gets both
 * for free from markdown-it):
 *   1. A `$$` block inside a fenced code block is SOURCE, not math.
 *   2. Cursor motion must be able to STOP inside a rendered block — a block
 *      replace range has no visible positions, so arrow keys (and vim j/k,
 *      which route through the same motion) used to leap over a diagram and
 *      it could only be opened by clicking.
 */

const DOC = [
  'before one',
  'before two',
  '',
  '```mermaid',
  'flowchart LR',
  '  A --> B',
  '```',
  '',
  'A real formula:',
  '',
  '$$',
  'E = mc^2',
  '$$',
  '',
  'And the same thing quoted in a code fence:',
  '',
  '````markdown',
  '$$',
  '\\int_0^1 x^2 dx',
  '$$',
  '````',
  '',
  'after one',
  '',
].join('\n');

async function openLive(page: Page) {
  await setupBasicMocks(page);
  await page.route('**/api/kiln/file**', (route) => route.fulfill({ json: { content: DOC } }));
  await page.goto('/');
  await appReady(page);

  await page.evaluate(() => {
    const store = (window as unknown as Record<string, any>).__windowStore;
    const actions = (window as unknown as Record<string, any>).__windowActions;
    const firstGroup = (n: any): string | null =>
      !n ? null : n.type === 'pane' ? n.tabGroupId : firstGroup(n.first) ?? firstGroup(n.second);
    actions.addTab(firstGroup(store.layout), {
      id: 'tab-lp-blocks',
      title: 'blocks.md',
      contentType: 'file',
      metadata: { filePath: '/kiln/blocks.md', initialMode: 'live' },
    });
  });
  await expect(page.locator('.cm-content')).toBeVisible({ timeout: 20000 });
  await expect(page.getByTestId('lp-mermaid')).toBeVisible({ timeout: 20000 });
}

test.describe('live preview blocks', () => {
  test('renders display math but leaves a $$ block inside a code fence as source', async ({
    page,
  }) => {
    await openLive(page);

    // The real `$$` block became a KaTeX widget…
    await expect(page.locator('.cm-lp-math-display')).toHaveCount(1);
    // …and the one quoted inside the fence is still its own two `$$` lines,
    // with the fence intact rather than split around a rendered formula.
    await expect(page.locator('.cm-line', { hasText: '\\int_0^1 x^2 dx' })).toHaveCount(1);
    await expect(page.locator('.cm-content')).toContainText('````markdown');
  });

  test('arrow motion stops inside a rendered block instead of leaping over it', async ({
    page,
  }) => {
    await openLive(page);

    await page.locator('.cm-line').first().click();

    // Line 1 → 2 → blank → the diagram block.
    const fenceSource = page.locator('.cm-line', { hasText: 'flowchart LR' });
    await expect(fenceSource).toHaveCount(0);
    for (let i = 0; i < 3; i++) await page.keyboard.press('ArrowDown');

    // The block revealed its source instead of being skipped.
    await expect(fenceSource).toHaveCount(1);
    await expect(page.getByTestId('lp-mermaid')).toHaveCount(0);

    // Continuing down walks out of the block, which renders again.
    for (let i = 0; i < 4; i++) await page.keyboard.press('ArrowDown');
    await expect(page.getByTestId('lp-mermaid')).toHaveCount(1);
  });
});
