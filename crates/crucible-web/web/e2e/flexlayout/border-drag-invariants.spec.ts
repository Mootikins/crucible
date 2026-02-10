import { test, expect, Page } from '@playwright/test';
import { findPath, findTabButton, drag, Location } from './helpers';

const baseURL = '/flexlayout-test.html';
const evidencePath = '../../../.sisyphus/evidence';

function borderTabButtons(page: Page, border: string) {
  return page.locator(`.flexlayout__border_button[data-layout-path^="/border/${border}/tb"]`).filter({ visible: true });
}

function borderTabButtonContent(page: Page, border: string, index: number) {
  return findTabButton(page, `/border/${border}`, index).locator('.flexlayout__border_button_content').first();
}

// ── Test Group F: Drag-out visual behavior ──────────────────────────────────

test.describe('Drag-out visual behavior', () => {
  test('F1: drag active tab out of border shows next tab content', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_dnd_dock');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(borderTabButtonContent(page, 'left', 0)).toContainText('Explorer');
    await expect(borderTabButtonContent(page, 'left', 1)).toContainText('Search');

    const explorerBtn = findTabButton(page, '/border/left', 0);
    const centerTabset = page.locator('.flexlayout__tabset').first();
    await drag(page, explorerBtn, centerTabset, Location.CENTER);

    await expect(borderTabButtons(page, 'left')).toHaveCount(1);
    await expect(borderTabButtonContent(page, 'left', 0)).toContainText('Search');

    const centerTabs = page.locator('.flexlayout__tabset .flexlayout__tab_button');
    const centerTabTexts: string[] = [];
    const count = await centerTabs.count();
    for (let i = 0; i < count; i++) {
      const text = await centerTabs.nth(i).textContent();
      centerTabTexts.push(text || '');
    }
    expect(centerTabTexts.some(t => t.includes('Explorer'))).toBe(true);

    await page.screenshot({ path: `${evidencePath}/drag-inv-F1-active-tab-out.png` });
  });

  test('F2: drag last tab out of border auto-hides it', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_dnd_dock');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const centerTabset = page.locator('.flexlayout__tabset').first();

    const terminalBtn = findTabButton(page, '/border/bottom', 0);
    await drag(page, terminalBtn, centerTabset, Location.CENTER);

    await expect(borderTabButtons(page, 'bottom')).toHaveCount(1);

    const outputBtn = findTabButton(page, '/border/bottom', 0);
    await drag(page, outputBtn, centerTabset, Location.CENTER);

    const borderBottom = page.locator('.flexlayout__border_bottom');
    await expect(borderBottom.first()).toHaveClass(/flexlayout__border--hidden/);

    await expect(borderTabButtons(page, 'bottom')).toHaveCount(0);

    const centerTabs = page.locator('.flexlayout__tabset .flexlayout__tab_button');
    await expect(centerTabs).toHaveCount(4);

    await page.screenshot({ path: `${evidencePath}/drag-inv-F2-last-tab-auto-hide.png` });
  });

  test('F3: drag tab from border to center increases center tab count', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_dnd_dock');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const centerTabs = page.locator('.flexlayout__tabset .flexlayout__tab_button');
    await expect(centerTabs).toHaveCount(2);

    const explorerBtn = findTabButton(page, '/border/left', 0);
    const centerTabset = page.locator('.flexlayout__tabset').first();
    await drag(page, explorerBtn, centerTabset, Location.CENTER);

    await expect(page.locator('.flexlayout__tabset .flexlayout__tab_button')).toHaveCount(3);

    await page.screenshot({ path: `${evidencePath}/drag-inv-F3-center-gains-tab.png` });
  });

  test('F4: drag tab between borders updates both source and target', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_dnd_dock');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(borderTabButtons(page, 'left')).toHaveCount(2);

    const explorerBtn = findTabButton(page, '/border/left', 0);
    const rightBorder = findPath(page, '/border/right');
    await drag(page, explorerBtn, rightBorder, Location.CENTER);

    await expect(borderTabButtons(page, 'left')).toHaveCount(1);
    await expect(borderTabButtonContent(page, 'left', 0)).toContainText('Search');

    await expect(borderTabButtons(page, 'right')).toHaveCount(1);
    await expect(borderTabButtonContent(page, 'right', 0)).toContainText('Explorer');

    await page.screenshot({ path: `${evidencePath}/drag-inv-F4-between-borders.png` });
  });
});

// ── Test Group G: Collapsed label visibility ────────────────────────────────

test.describe('Collapsed label visibility', () => {
  test('G1: collapsed left border shows full tab names', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/left/button/dock"]');
    await dockButton.click();

    const borderLeft = page.locator('.flexlayout__border_left');
    await expect(borderLeft.first()).toHaveClass(/flexlayout__border--collapsed/);

    const collapsedLabels = page.locator('.flexlayout__border_left .flexlayout__border_collapsed_label');
    const labelTexts: string[] = [];
    const count = await collapsedLabels.count();
    for (let i = 0; i < count; i++) {
      const text = await collapsedLabels.nth(i).textContent();
      labelTexts.push(text || '');
    }

    expect(labelTexts.some(t => t.includes('Explorer'))).toBe(true);
    expect(labelTexts.some(t => t.includes('Search'))).toBe(true);

    await page.screenshot({ path: `${evidencePath}/drag-inv-G1-collapsed-left.png` });
  });

  test('G2: collapsed right border shows full tab names', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/right/button/dock"]');
    await dockButton.click();

    const borderRight = page.locator('.flexlayout__border_right');
    await expect(borderRight.first()).toHaveClass(/flexlayout__border--collapsed/);

    const collapsedLabels = page.locator('.flexlayout__border_right .flexlayout__border_collapsed_label');
    const labelTexts: string[] = [];
    const count = await collapsedLabels.count();
    for (let i = 0; i < count; i++) {
      const text = await collapsedLabels.nth(i).textContent();
      labelTexts.push(text || '');
    }

    expect(labelTexts.some(t => t.includes('Properties'))).toBe(true);

    await page.screenshot({ path: `${evidencePath}/drag-inv-G2-collapsed-right.png` });
  });

  test('G3: collapsed bottom border shows full tab names', async ({ page }) => {
    await page.goto(baseURL + '?layout=docked_panes');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const dockButton = page.locator('[data-layout-path="/border/bottom/button/dock"]');
    await dockButton.click();

    const borderBottom = page.locator('.flexlayout__border_bottom');
    await expect(borderBottom.first()).toHaveClass(/flexlayout__border--collapsed/);

    const collapsedLabels = page.locator('.flexlayout__border_bottom .flexlayout__border_collapsed_label');
    const labelTexts: string[] = [];
    const count = await collapsedLabels.count();
    for (let i = 0; i < count; i++) {
      const text = await collapsedLabels.nth(i).textContent();
      labelTexts.push(text || '');
    }

    expect(labelTexts.some(t => t.includes('Terminal'))).toBe(true);
    expect(labelTexts.some(t => t.includes('Output'))).toBe(true);

    await page.screenshot({ path: `${evidencePath}/drag-inv-G3-collapsed-bottom.png` });
  });
});

// ── Test Group H: Drop-into-collapsed/hidden ────────────────────────────────

test.describe('Drop-into-collapsed/hidden', () => {
  test('H1: collapse border then expand via dock restores tabs', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_dnd_dock');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(borderTabButtons(page, 'left')).toHaveCount(2);

    const dockButton = page.locator('[data-layout-path="/border/left/button/dock"]');
    await dockButton.click();

    const borderLeft = page.locator('.flexlayout__border_left');
    await expect(borderLeft.first()).toHaveClass(/flexlayout__border--collapsed/);

    const collapsedLabels = page.locator('.flexlayout__border_left .flexlayout__border_collapsed_label');
    await expect(collapsedLabels).toHaveCount(2);

    await dockButton.click();
    await dockButton.click();

    await expect(borderLeft.first()).not.toHaveClass(/flexlayout__border--collapsed/);
    await expect(borderLeft.first()).not.toHaveClass(/flexlayout__border--hidden/);
    await expect(borderTabButtons(page, 'left')).toHaveCount(2);

    await page.screenshot({ path: `${evidencePath}/drag-inv-H1-collapse-expand-restore.png` });
  });

  test('H2: drop tab into empty border expands it', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_dnd_dock');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(borderTabButtons(page, 'right')).toHaveCount(0);

    const documentTab = page.locator('.flexlayout__tabset .flexlayout__tab_button').filter({ hasText: 'Document' });
    const rightBorder = findPath(page, '/border/right');
    await drag(page, documentTab, rightBorder, Location.CENTER);

    await expect(borderTabButtons(page, 'right')).toHaveCount(1);
    await expect(borderTabButtonContent(page, 'right', 0)).toContainText('Document');

    await page.screenshot({ path: `${evidencePath}/drag-inv-H2-drop-into-empty.png` });
  });

  test('H3: dock button click restores hidden border after drag-out', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_dnd_dock');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const centerTabset = page.locator('.flexlayout__tabset').first();

    const terminalBtn = findTabButton(page, '/border/bottom', 0);
    await drag(page, terminalBtn, centerTabset, Location.CENTER);

    await expect(borderTabButtons(page, 'bottom')).toHaveCount(1);

    const outputBtn = findTabButton(page, '/border/bottom', 0);
    await drag(page, outputBtn, centerTabset, Location.CENTER);

    const borderBottom = page.locator('.flexlayout__border_bottom');
    await expect(borderBottom.first()).toHaveClass(/flexlayout__border--hidden/);

    const dockButton = page.locator('[data-layout-path="/border/bottom/button/dock"]');
    await dockButton.click({ force: true });

    await expect(borderBottom.first()).not.toHaveClass(/flexlayout__border--hidden/);

    await page.screenshot({ path: `${evidencePath}/drag-inv-H3-dock-restore-hidden.png` });
  });
});

// ── Test Group I: Complex sequences ─────────────────────────────────────────

test.describe('Complex sequences', () => {
  test('I1: drag out then collapse then expand — state consistent', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_dnd_dock');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const explorerBtn = findTabButton(page, '/border/left', 0);
    const centerTabset = page.locator('.flexlayout__tabset').first();
    await drag(page, explorerBtn, centerTabset, Location.CENTER);

    await expect(borderTabButtons(page, 'left')).toHaveCount(1);
    await expect(borderTabButtonContent(page, 'left', 0)).toContainText('Search');

    const dockButton = page.locator('[data-layout-path="/border/left/button/dock"]');
    await dockButton.click();

    const borderLeft = page.locator('.flexlayout__border_left');
    await expect(borderLeft.first()).toHaveClass(/flexlayout__border--collapsed/);

    await dockButton.click();
    await dockButton.click();

    await expect(borderLeft.first()).not.toHaveClass(/flexlayout__border--collapsed/);
    await expect(borderLeft.first()).not.toHaveClass(/flexlayout__border--hidden/);
    await expect(borderTabButtons(page, 'left')).toHaveCount(1);
    await expect(borderTabButtonContent(page, 'left', 0)).toContainText('Search');

    await page.screenshot({ path: `${evidencePath}/drag-inv-I1-drag-collapse-expand.png` });
  });

  test('I2: drag out reduces border count and increases center count', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_dnd_dock');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(borderTabButtons(page, 'left')).toHaveCount(2);
    await expect(page.locator('.flexlayout__tabset .flexlayout__tab_button')).toHaveCount(2);

    const explorerBtn = findTabButton(page, '/border/left', 0);
    const centerTabset = page.locator('.flexlayout__tabset').first();
    await drag(page, explorerBtn, centerTabset, Location.CENTER);

    await expect(borderTabButtons(page, 'left')).toHaveCount(1);
    await expect(page.locator('.flexlayout__tabset .flexlayout__tab_button')).toHaveCount(3);

    const searchBtn = findTabButton(page, '/border/left', 0);
    await drag(page, searchBtn, centerTabset, Location.CENTER);

    await expect(borderTabButtons(page, 'left')).toHaveCount(0);
    await expect(page.locator('.flexlayout__tabset .flexlayout__tab_button')).toHaveCount(4);

    const borderLeft = page.locator('.flexlayout__border_left');
    await expect(borderLeft.first()).toHaveClass(/flexlayout__border--hidden/);

    await page.screenshot({ path: `${evidencePath}/drag-inv-I2-drain-border.png` });
  });

  test('I3: drag between borders then dock cycle — all states consistent', async ({ page }) => {
    await page.goto(baseURL + '?layout=border_dnd_dock');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(borderTabButtons(page, 'left')).toHaveCount(2);
    await expect(borderTabButtons(page, 'bottom')).toHaveCount(2);

    const explorerBtn = findTabButton(page, '/border/left', 0);
    const bottomBorder = findPath(page, '/border/bottom');
    await drag(page, explorerBtn, bottomBorder, Location.CENTER);

    await expect(borderTabButtons(page, 'left')).toHaveCount(1);
    await expect(borderTabButtons(page, 'bottom')).toHaveCount(3);

    const dockButton = page.locator('[data-layout-path="/border/bottom/button/dock"]');
    await dockButton.evaluate((el: HTMLElement) => el.click());

    const borderBottom = page.locator('.flexlayout__border_bottom');
    await expect(borderBottom.first()).toHaveClass(/flexlayout__border--collapsed/);

    await dockButton.evaluate((el: HTMLElement) => el.click());
    await expect(borderBottom.first()).toHaveClass(/flexlayout__border--hidden/);

    await dockButton.evaluate((el: HTMLElement) => el.click());

    await expect(borderBottom.first()).not.toHaveClass(/flexlayout__border--collapsed/);
    await expect(borderBottom.first()).not.toHaveClass(/flexlayout__border--hidden/);
    await expect(borderTabButtons(page, 'bottom')).toHaveCount(3);

    await page.screenshot({ path: `${evidencePath}/drag-inv-I3-between-borders-dock-cycle.png` });
  });
});
