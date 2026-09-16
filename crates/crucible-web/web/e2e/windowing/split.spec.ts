import { test, expect } from '@playwright/test';
import { stableCenter } from '../helpers/geometry';
import { act, depthOf, fillCentrePanes, openHarness, panesOf, readStore } from './harness';

/**
 * Centre splits: the tree the split actions build, and the geometry that
 * the splitter gives the two sides.
 */

test.beforeEach(async ({ page }) => openHarness(page));

/** The id of the first centre pane. */
async function firstCentrePane(page: import('@playwright/test').Page): Promise<string> {
  return panesOf((await readStore(page)).layout)[0]!.id;
}

test('creates a vertical split with row splitter semantics', async ({ page }) => {
  await act(page, 'splitPane', await firstCentrePane(page), 'vertical');

  // Row semantics live on the splitter's cursor, which an inert splitter
  // drops. The new pane is born empty, so fill both sides first.
  await fillCentrePanes(page);

  await expect(page.locator('[data-split-id].cursor-row-resize').first()).toBeVisible();

  const { layout } = await readStore(page);
  expect(layout.type).toBe('split');
  expect(layout.direction).toBe('vertical');
});

test('supports nested splits by splitting a child pane after initial split', async ({ page }) => {
  const before = depthOf((await readStore(page)).layout);
  await act(page, 'splitPane', await firstCentrePane(page), 'vertical');
  await act(page, 'splitPane', await firstCentrePane(page), 'horizontal');

  const { layout } = await readStore(page);
  expect(depthOf(layout)).toBe(before + 2);
  expect(layout.direction).toBe('vertical');
  expect(layout.first!.type).toBe('split');
  expect(layout.first!.direction).toBe('horizontal');
});

test('a yielding pane leaves no gap — the split still fills its container', async ({ page }) => {
  // Regression: the growing half carried its 0.5 ratio as its flex factor.
  // Flexbox hands out free space in proportion to the grow factors and keeps
  // the remainder when they sum to under 1, so beside a fixed-basis pane the
  // centre ended 348px short of its own container. jsdom does not lay
  // anything out, so only a browser sees it.
  await act(page, 'splitPane', await firstCentrePane(page), 'horizontal');

  // The new pane is born empty, so exactly one side yields.
  await expect(page.getByTestId('empty-pane').first()).toBeVisible();

  const geometry = await page.evaluate(() => {
    const splitter = document.querySelector('[data-testid="resize-splitter"]') as HTMLElement;
    const container = splitter.parentElement as HTMLElement;
    const w = (n: Element) => n.getBoundingClientRect().width;
    return {
      container: w(container),
      first: w(splitter.previousElementSibling!),
      splitter: w(splitter),
      second: w(splitter.nextElementSibling!),
    };
  });

  const covered = geometry.first + geometry.splitter + geometry.second;
  // Sub-pixel rounding only. Before the fix this was short by ~348px.
  expect(Math.abs(covered - geometry.container)).toBeLessThan(2);
});

test('split ratio persists after dragging splitter away from default', async ({ page }) => {
  await act(page, 'splitPane', await firstCentrePane(page), 'horizontal');
  // An empty side yields its width and pins the splitter. This test is about
  // the ratio that a DRAG writes, so both sides need content.
  await fillCentrePanes(page);

  const splitter = page.locator('[data-split-id]').first();
  const { x: cx, y: cy } = await stableCenter(splitter);

  // The separator is 1px wide and an ::after pseudo widens it, so a raw move
  // to the box centre can round onto the neighbouring pane. hover() uses
  // Playwright's own hit point.
  await splitter.hover();
  await page.mouse.down();
  await page.mouse.move(cx + 110, cy, { steps: 8 });
  await page.mouse.up();

  const readRatio = async () => (await readStore(page)).layout.splitRatio ?? 0.5;
  await expect.poll(readRatio).toBeGreaterThan(0.5);
  expect(Math.abs((await readRatio()) - 0.5)).toBeGreaterThan(0.02);
});

test('center splitter resize updates pane width', async ({ page }) => {
  await act(page, 'splitPane', await firstCentrePane(page), 'horizontal');
  await fillCentrePanes(page);

  const splitter = page.locator('[data-split-id]').first();
  const firstPane = splitter.locator('..').locator('> div').first();

  // Settle before measuring: a width read mid-layout is compared against a
  // settled one, and the assertion then says nothing about the drag.
  const { x: cx, y: cy } = await stableCenter(splitter);
  const widthBefore = await firstPane.evaluate((el) => el.getBoundingClientRect().width);

  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(cx + 80, cy, { steps: 5 });
  await page.mouse.up();

  await expect
    .poll(() => firstPane.evaluate((el) => el.getBoundingClientRect().width))
    .toBeGreaterThan(widthBefore);
});
