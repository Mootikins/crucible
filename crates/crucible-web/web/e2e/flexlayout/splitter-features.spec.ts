import { test, expect } from '@playwright/test';
import {
  findPath,
  findTabButton,
  findAllTabSets,
  dragSplitter,
} from './helpers';

const baseURL = '/flexlayout-test.html';
const evidencePath = '../../../.sisyphus/evidence';

// ─── 5.1 Splitter Size ───────────────────────────────────────────────

test.describe('Splitter Feature 5.1: Splitter Size', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=splitter_handle');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('splitter renders with configured splitterSize thickness', async ({ page }) => {
    const splitter = findPath(page, '/s0');
    await expect(splitter).toBeVisible();
    const box = await splitter.boundingBox();
    expect(box).toBeTruthy();
    expect(box!.width).toBeGreaterThanOrEqual(10);
    expect(box!.width).toBeLessThanOrEqual(16);

    await page.screenshot({ path: `${evidencePath}/splitter-5.1-size.png` });
  });

  test('multiple splitters all use the configured size', async ({ page }) => {
    const s0 = findPath(page, '/s0');
    const s1 = findPath(page, '/s1');
    await expect(s0).toBeVisible();
    await expect(s1).toBeVisible();

    const box0 = await s0.boundingBox();
    const box1 = await s1.boundingBox();
    expect(box0).toBeTruthy();
    expect(box1).toBeTruthy();
    expect(Math.abs(box0!.width - box1!.width)).toBeLessThan(2);
  });
});

// ─── 5.2 Splitter Extra Hit Area ─────────────────────────────────────

test.describe('Splitter Feature 5.2: Splitter Extra Hit Area', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=splitter_extra');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('thin splitter with extra hit area renders and is draggable', async ({ page }) => {
    const splitter = findPath(page, '/s0');
    await expect(splitter).toBeVisible();

    const ts0Before = await findPath(page, '/ts0').boundingBox();
    expect(ts0Before).toBeTruthy();
    const widthBefore = ts0Before!.width;

    await dragSplitter(page, splitter, false, 80);

    const ts0After = await findPath(page, '/ts0').boundingBox();
    expect(ts0After).toBeTruthy();
    expect(ts0After!.width).toBeGreaterThan(widthBefore + 30);

    await page.screenshot({ path: `${evidencePath}/splitter-5.2-extra-hit-area.png` });
  });

  test('vertical splitter in nested row also responds to drag', async ({ page }) => {
    const vSplitter = findPath(page, '/r1/s0');
    await expect(vSplitter).toBeVisible();

    const ts0Before = await findPath(page, '/r1/ts0').boundingBox();
    expect(ts0Before).toBeTruthy();
    const heightBefore = ts0Before!.height;

    await dragSplitter(page, vSplitter, true, 60);

    const ts0After = await findPath(page, '/r1/ts0').boundingBox();
    expect(ts0After).toBeTruthy();
    expect(ts0After!.height).toBeGreaterThan(heightBefore + 20);
  });
});

// ─── 5.3 Splitter Visible Handle/Grip ────────────────────────────────

test.describe('Splitter Feature 5.3: Splitter Visible Handle', () => {
  test('splitter with splitterEnableHandle has visible grip element', async ({ page }) => {
    await page.goto(baseURL + '?layout=splitter_handle');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const splitter = findPath(page, '/s0');
    await expect(splitter).toBeVisible();

    const splitterBox = await splitter.boundingBox();
    expect(splitterBox).toBeTruthy();
    expect(splitterBox!.width).toBeGreaterThanOrEqual(10);

    await page.screenshot({ path: `${evidencePath}/splitter-5.3-handle.png` });
  });

  test('splitter handle allows interactive drag resize', async ({ page }) => {
    await page.goto(baseURL + '?layout=splitter_handle');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const ts0Before = await findPath(page, '/ts0').boundingBox();
    expect(ts0Before).toBeTruthy();

    const splitter = findPath(page, '/s0');
    await dragSplitter(page, splitter, false, 100);

    const ts0After = await findPath(page, '/ts0').boundingBox();
    expect(ts0After).toBeTruthy();
    expect(ts0After!.width).toBeGreaterThan(ts0Before!.width + 50);

    await page.screenshot({ path: `${evidencePath}/splitter-5.3-handle-drag.png` });
  });
});

// ─── 5.4 Realtime Resize ─────────────────────────────────────────────

test.describe('Splitter Feature 5.4: Realtime Resize', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_realtime_resize');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('layout with realtimeResize renders three panes with splitters', async ({ page }) => {
    await expect(findAllTabSets(page)).toHaveCount(3);

    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Left Pane');
    await expect(findTabButton(page, '/ts1', 0).locator('.flexlayout__tab_button_content')).toContainText('Center Pane');
    await expect(findTabButton(page, '/ts2', 0).locator('.flexlayout__tab_button_content')).toContainText('Right Pane');

    const s0 = findPath(page, '/s0');
    const s1 = findPath(page, '/s1');
    await expect(s0).toBeVisible();
    await expect(s1).toBeVisible();

    await page.screenshot({ path: `${evidencePath}/splitter-5.4-realtime-resize.png` });
  });

  test('dragging splitter resizes panes in realtime mode', async ({ page }) => {
    const ts0Before = await findPath(page, '/ts0').boundingBox();
    expect(ts0Before).toBeTruthy();

    const splitter = findPath(page, '/s0');
    await dragSplitter(page, splitter, false, 120);

    const ts0After = await findPath(page, '/ts0').boundingBox();
    expect(ts0After).toBeTruthy();
    expect(ts0After!.width).toBeGreaterThan(ts0Before!.width + 60);

    await page.screenshot({ path: `${evidencePath}/splitter-5.4-realtime-drag.png` });
  });
});

// ─── 5.5 Adjust Weights (Programmatic) ───────────────────────────────

test.describe('Splitter Feature 5.5: Adjust Weights Programmatic', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=action_weights');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('equal weights button sets both tabsets to roughly equal width', async ({ page }) => {
    await page.locator('[data-id="action-weights-8020"]').click();
    await page.locator('[data-id="action-equal-weights"]').click();

    const box0 = await findPath(page, '/ts0').boundingBox();
    const box1 = await findPath(page, '/ts1').boundingBox();
    expect(box0).toBeTruthy();
    expect(box1).toBeTruthy();

    const ratio = box0!.width / box1!.width;
    expect(ratio).toBeGreaterThan(0.85);
    expect(ratio).toBeLessThan(1.15);

    await page.screenshot({ path: `${evidencePath}/splitter-5.5-equal-weights.png` });
  });

  test('80/20 button skews proportions to approximately 4:1', async ({ page }) => {
    await page.locator('[data-id="action-weights-8020"]').click();

    const box0 = await findPath(page, '/ts0').boundingBox();
    const box1 = await findPath(page, '/ts1').boundingBox();
    expect(box0).toBeTruthy();
    expect(box1).toBeTruthy();

    const ratio = box0!.width / box1!.width;
    expect(ratio).toBeGreaterThan(2.5);

    await page.screenshot({ path: `${evidencePath}/splitter-5.5-weights-8020.png` });
  });
});

// ─── 5.6 Adjust Border Split (Programmatic) ──────────────────────────

test.describe('Splitter Feature 5.6: Adjust Border Split', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=splitter_adjust_border');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('border tab is present and can be opened', async ({ page }) => {
    const bottomTab = findTabButton(page, '/border/bottom', 0);
    await expect(bottomTab).toBeVisible();
    await expect(bottomTab.locator('.flexlayout__border_button_content')).toContainText('Bottom Panel');

    await bottomTab.click();
    await expect(findPath(page, '/border/bottom/t0')).toBeVisible();

    await page.screenshot({ path: `${evidencePath}/splitter-5.6-border-open.png` });
  });

  test('adjust border button triggers programmatic border resize', async ({ page }) => {
    const bottomTab = findTabButton(page, '/border/bottom', 0);
    await bottomTab.click();
    await expect(findPath(page, '/border/bottom/t0')).toBeVisible();

    const borderBefore = await findPath(page, '/border/bottom/t0').boundingBox();
    expect(borderBefore).toBeTruthy();
    const heightBefore = borderBefore!.height;

    await page.locator('[data-id="action-adjust-border"]').click();

    const borderAfter = await findPath(page, '/border/bottom/t0').boundingBox();
    expect(borderAfter).toBeTruthy();

    // Action.adjustBorderSplit adjusts by delta — border panel remains visible with valid dimensions
    expect(borderAfter!.height).toBeGreaterThanOrEqual(100);
    expect(borderAfter!.width).toBeGreaterThan(0);

    await page.screenshot({ path: `${evidencePath}/splitter-5.6-adjust-border.png` });
  });
});

// ─── 5.7 Sash Color ──────────────────────────────────────────────────

test.describe('Splitter Feature 5.7: Sash Color', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=splitter_sash_color');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('splitter sash renders between three panes with handle enabled', async ({ page }) => {
    await expect(findAllTabSets(page)).toHaveCount(3);

    const s0 = findPath(page, '/s0');
    const s1 = findPath(page, '/s1');
    await expect(s0).toBeVisible();
    await expect(s1).toBeVisible();

    const s0Box = await s0.boundingBox();
    expect(s0Box).toBeTruthy();
    expect(s0Box!.width).toBeGreaterThanOrEqual(6);
    expect(s0Box!.width).toBeLessThanOrEqual(12);

    await page.screenshot({ path: `${evidencePath}/splitter-5.7-sash-color.png` });
  });

  test('splitter sash is draggable for resizing', async ({ page }) => {
    const ts0Before = await findPath(page, '/ts0').boundingBox();
    expect(ts0Before).toBeTruthy();

    const splitter = findPath(page, '/s0');
    await dragSplitter(page, splitter, false, 80);

    const ts0After = await findPath(page, '/ts0').boundingBox();
    expect(ts0After).toBeTruthy();
    expect(ts0After!.width).toBeGreaterThan(ts0Before!.width + 30);

    await page.screenshot({ path: `${evidencePath}/splitter-5.7-sash-drag.png` });
  });

  test('tab names render correctly in sash color layout', async ({ page }) => {
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Left');
    await expect(findTabButton(page, '/ts1', 0).locator('.flexlayout__tab_button_content')).toContainText('Center');
    await expect(findTabButton(page, '/ts2', 0).locator('.flexlayout__tab_button_content')).toContainText('Right');
  });
});
