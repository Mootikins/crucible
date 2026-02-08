import { test, expect } from '@playwright/test';
import {
  findPath,
  findTabButton,
  findAllTabSets,
  drag,
  dragSplitter,
  Location,
} from './helpers';

const baseURL = '/flexlayout-test.html';
const evidencePath = '../../../.sisyphus/evidence';

// ─── 2.1 Close Button (closeType 0/1/2) ──────────────────────────────

test.describe('Tab Feature 2.1: Close Button Types', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=tab_close_types');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('all close types render tabs with close buttons visible', async ({ page }) => {
    for (let i = 0; i < 5; i++) {
      const tab = findTabButton(page, '/ts0', i);
      await expect(tab).toBeVisible();
      const closeBtn = findPath(page, `/ts0/tb${i}/button/close`);
      await expect(closeBtn).toBeAttached();
    }
    await page.screenshot({ path: `${evidencePath}/tab-close-types.png` });
  });

  test('clicking close button removes the tab', async ({ page }) => {
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Close Type 0');
    await findPath(page, '/ts0/tb0/button/close').click();
    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Close Type 1');
    await page.screenshot({ path: `${evidencePath}/tab-close-type0-removed.png` });
  });
});

// ─── 2.2 Drag to Move ────────────────────────────────────────────────

test.describe('Tab Feature 2.2: Drag to Move', () => {
  test('tab with enableDrag: false resists reorder via drag', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_drag_disabled');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Draggable');
    await expect(findTabButton(page, '/ts0', 1).locator('.flexlayout__tab_button_content')).toContainText('Locked');
    await expect(findTabButton(page, '/ts0', 2).locator('.flexlayout__tab_button_content')).toContainText('Also Draggable');

    const locked = findTabButton(page, '/ts0', 1);
    const target = findTabButton(page, '/ts0', 2);
    await drag(page, locked, target, Location.RIGHT);

    await expect(findTabButton(page, '/ts0', 1).locator('.flexlayout__tab_button_content')).toContainText('Locked');
    await page.screenshot({ path: `${evidencePath}/tab-drag-disabled.png` });
  });

  test('draggable tab can be reordered', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_drag_disabled');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const from = findTabButton(page, '/ts0', 2);
    const to = findTabButton(page, '/ts0', 0);
    await drag(page, from, to, Location.LEFT);

    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Also Draggable');
    await page.screenshot({ path: `${evidencePath}/tab-drag-reorder.png` });
  });
});

// ─── 2.3 Rename (Double-Click) ───────────────────────────────────────

test.describe('Tab Feature 2.3: Tab Rename', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_tab_rename');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('double-click opens inline rename, Enter commits', async ({ page }) => {
    await findPath(page, '/ts0/tb0').dblclick();

    const textbox = findPath(page, '/ts0/tb0/textbox');
    await expect(textbox).toBeVisible();
    await expect(textbox).toHaveValue('Rename Me');

    await textbox.fill('');
    await textbox.type('New Name');
    await textbox.press('Enter');

    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('New Name');
    await page.screenshot({ path: `${evidencePath}/tab-rename-commit.png` });
  });

  test('Escape cancels rename and preserves original name', async ({ page }) => {
    await findPath(page, '/ts0/tb0').dblclick();

    const textbox = findPath(page, '/ts0/tb0/textbox');
    await expect(textbox).toBeVisible();
    await textbox.type('Cancelled');
    await textbox.press('Escape');

    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Rename Me');
    await page.screenshot({ path: `${evidencePath}/tab-rename-cancel.png` });
  });
});

// ─── 2.4 Icons ───────────────────────────────────────────────────────

test.describe('Tab Feature 2.4: Tab Icons', () => {
  test('tabs with icon attribute have leading img element in header', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_icons');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    for (let i = 0; i < 5; i++) {
      const tab = findTabButton(page, '/ts0', i);
      await expect(tab).toBeVisible();
      await expect(tab.locator('.flexlayout__tab_button_leading img')).toBeAttached();
    }

    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Home');
    await expect(findTabButton(page, '/ts0', 1).locator('.flexlayout__tab_button_content')).toContainText('Settings');
    await expect(findTabButton(page, '/ts0', 2).locator('.flexlayout__tab_button_content')).toContainText('Search');
    await expect(findTabButton(page, '/ts0', 3).locator('.flexlayout__tab_button_content')).toContainText('Star');
    await expect(findTabButton(page, '/ts0', 4).locator('.flexlayout__tab_button_content')).toContainText('Warning');

    await page.screenshot({ path: `${evidencePath}/tab-icons.png` });
  });
});

// ─── 2.5 Help Text (Tooltip) ─────────────────────────────────────────

test.describe('Tab Feature 2.5: Help Text (Tooltip)', () => {
  test('tab with helpText renders title attribute for tooltip', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_help_text');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const tab0 = findTabButton(page, '/ts0', 0);
    await expect(tab0).toBeVisible();
    await expect(tab0.locator('.flexlayout__tab_button_content')).toContainText('Overview');
    await expect(tab0).toHaveAttribute('title', /project overview/i);

    await page.screenshot({ path: `${evidencePath}/tab-help-text.png` });
  });
});

// ─── 2.6 Alt Name (Overflow Menu) ────────────────────────────────────

test.describe('Tab Feature 2.6: Alt Name', () => {
  test('tabs with altName render with full names when space is available', async ({ page }) => {
    await page.goto(baseURL + '?layout=tab_alt_name');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Very Long Tab Name');
    await expect(findTabButton(page, '/ts0', 2).locator('.flexlayout__tab_button_content')).toContainText('Normal Name');

    for (let i = 0; i < 3; i++) {
      await expect(findTabButton(page, '/ts0', i)).toBeVisible();
    }

    await page.screenshot({ path: `${evidencePath}/tab-alt-name.png` });
  });
});

// ─── 2.7 Component Type (Factory Mapping) ────────────────────────────

test.describe('Tab Feature 2.7: Component Type', () => {
  test('info component renders description text from config', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_help_text');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const panel = page.locator('[data-testid="panel-Overview"]');
    await expect(panel).toBeVisible();
    await expect(panel).toContainText('Hover over this tab');
  });

  test('counter component renders interactive increment button', async ({ page }) => {
    await page.goto(baseURL + '?layout=tab_set_component');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await page.locator('[data-id="action-set-component"]').click();

    const tabPanel = findPath(page, '/ts0/t0');
    await expect(tabPanel).toBeVisible();
    await expect(tabPanel.locator('button')).toContainText('Increment');
    await expect(tabPanel).toContainText('Count:');

    await page.screenshot({ path: `${evidencePath}/tab-component-counter.png` });
  });
});

// ─── 2.8 Custom Config (Arbitrary JSON) ──────────────────────────────

test.describe('Tab Feature 2.8: Custom Config', () => {
  test('info component displays config.description from tab config', async ({ page }) => {
    await page.goto(baseURL + '?layout=tab_close_types');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await findTabButton(page, '/ts0', 0).click();
    const panel = findPath(page, '/ts0/t0');
    await expect(panel).toBeVisible();
    await expect(panel).toContainText('tabCloseType: 0');

    await findTabButton(page, '/ts0', 1).click();
    const panel1 = findPath(page, '/ts0/t1');
    await expect(panel1).toBeVisible();
    await expect(panel1).toContainText('tabCloseType: 1');

    await page.screenshot({ path: `${evidencePath}/tab-custom-config.png` });
  });
});

// ─── 2.9 Tab CSS Class ───────────────────────────────────────────────

test.describe('Tab Feature 2.9: Tab CSS Class', () => {
  test('tab with className has custom class on tab button DOM element', async ({ page }) => {
    await page.goto(baseURL + '?layout=tab_css_class');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const styledTab = findTabButton(page, '/ts0', 0);
    await expect(styledTab).toBeVisible();
    await expect(styledTab).toHaveClass(/custom-tab/);

    const altTab = findTabButton(page, '/ts0', 1);
    await expect(altTab).toHaveClass(/custom-tab-alt/);

    const defaultTab = findTabButton(page, '/ts0', 2);
    await expect(defaultTab).not.toHaveClass(/custom-tab/);

    await page.screenshot({ path: `${evidencePath}/tab-css-class.png` });
  });
});

// ─── 2.10 Content CSS Class ──────────────────────────────────────────

test.describe('Tab Feature 2.10: Tab Content CSS Class', () => {
  test('tabs render with correct names and contentClassName is stored in layout config', async ({ page }) => {
    await page.goto(baseURL + '?layout=tab_content_class');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Custom Content');
    await expect(findTabButton(page, '/ts0', 1).locator('.flexlayout__tab_button_content')).toContainText('Alt Content');
    await expect(findTabButton(page, '/ts0', 2).locator('.flexlayout__tab_button_content')).toContainText('Default Content');

    await findTabButton(page, '/ts0', 0).click();
    await expect(findPath(page, '/ts0/t0')).toBeVisible();
    await expect(findPath(page, '/ts0/t0')).toContainText('contentClassName');

    await page.screenshot({ path: `${evidencePath}/tab-content-class.png` });
  });
});

// ─── 2.11-2.12 Min/Max Width/Height ──────────────────────────────────

test.describe('Tab Feature 2.11-2.12: Min/Max Width/Height', () => {
  test('tabset respects minimum width constraint during splitter drag', async ({ page }) => {
    await page.goto(baseURL + '?layout=tabset_min_max');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const splitter = findPath(page, '/s0');
    await dragSplitter(page, splitter, false, -1000);

    const ts0 = findPath(page, '/ts0');
    const afterBox = await ts0.boundingBox();
    expect(afterBox).toBeTruthy();
    expect(afterBox!.width).toBeGreaterThanOrEqual(145);

    await page.screenshot({ path: `${evidencePath}/tab-min-width.png` });
  });

  test('tabset respects maximum width constraint during splitter drag', async ({ page }) => {
    await page.goto(baseURL + '?layout=tabset_min_max');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const splitter = findPath(page, '/s0');
    await dragSplitter(page, splitter, false, 1000);

    const ts0 = findPath(page, '/ts0');
    const afterBox = await ts0.boundingBox();
    expect(afterBox).toBeTruthy();
    expect(afterBox!.width).toBeLessThanOrEqual(610);

    await page.screenshot({ path: `${evidencePath}/tab-max-width.png` });
  });
});

// ─── 2.13 Border Size Override ───────────────────────────────────────

test.describe('Tab Feature 2.13: Tab Border Size Override', () => {
  test('border tab with tabBorderWidth overrides default border size', async ({ page }) => {
    await page.goto(baseURL + '?layout=tab_border_size');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const borderTab = findTabButton(page, '/border/left', 0);
    await borderTab.click();

    await expect(findPath(page, '/border/left/t0')).toBeVisible();

    const borderPanel = findPath(page, '/border/left/t0');
    const box = await borderPanel.boundingBox();
    expect(box).toBeTruthy();
    expect(box!.width).toBeGreaterThanOrEqual(250);
    expect(box!.width).toBeLessThan(400);

    await page.screenshot({ path: `${evidencePath}/tab-border-size.png` });
  });

  test('border tab without override uses default size', async ({ page }) => {
    await page.goto(baseURL + '?layout=tab_border_size');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const borderTab = findTabButton(page, '/border/bottom', 1);
    await borderTab.click();

    await expect(findPath(page, '/border/bottom/t1')).toBeVisible();
    const borderPanel = findPath(page, '/border/bottom/t1');
    const box = await borderPanel.boundingBox();
    expect(box).toBeTruthy();
    expect(box!.height).toBeGreaterThan(100);
    expect(box!.height).toBeLessThan(300);

    await page.screenshot({ path: `${evidencePath}/tab-border-default-size.png` });
  });
});

// ─── 2.14 Render On Demand (Lazy) ────────────────────────────────────

test.describe('Tab Feature 2.14: Render On Demand', () => {
  test('unselected tab content is not visible, selecting it makes it visible', async ({ page }) => {
    await page.goto(baseURL + '?layout=tab_render_on_demand');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Lazy A');
    await expect(findPath(page, '/ts0/t0')).toBeVisible();
    await expect(findPath(page, '/ts0/t1')).not.toBeVisible();
    await expect(findPath(page, '/ts0/t2')).not.toBeVisible();

    await findTabButton(page, '/ts0', 1).dispatchEvent('click');

    await expect(findPath(page, '/ts0/t1')).toBeVisible();
    await expect(findPath(page, '/ts0/t0')).not.toBeVisible();

    await page.screenshot({ path: `${evidencePath}/tab-render-on-demand.png` });
  });
});

// ─── 2.19 Title Update (Programmatic) ────────────────────────────────

test.describe('Tab Feature 2.19: Title Update', () => {
  test('tab rename via double-click updates the tab title', async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_tab_rename');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    await findTabButton(page, '/ts0', 1).click();
    await findPath(page, '/ts0/tb1').dblclick();
    const textbox = findPath(page, '/ts0/tb1/textbox');
    await expect(textbox).toBeVisible();
    await textbox.fill('');
    await textbox.type('Updated Title');
    await textbox.press('Enter');

    await expect(findTabButton(page, '/ts0', 1).locator('.flexlayout__tab_button_content')).toContainText('Updated Title');
    await page.screenshot({ path: `${evidencePath}/tab-title-update.png` });
  });
});

// ─── 2.20 Close Disable Per-Tab ──────────────────────────────────────

test.describe('Tab Feature 2.20: Close Disable Per-Tab', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(baseURL + '?layout=basic_close_disabled');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
  });

  test('tab with enableClose: false has no close button', async ({ page }) => {
    const permanentTab = findTabButton(page, '/ts0', 1);
    await expect(permanentTab).toBeVisible();
    await expect(permanentTab.locator('.flexlayout__tab_button_content')).toContainText('Permanent');
    const closeBtn = findPath(page, '/ts0/tb1/button/close');
    await expect(closeBtn).not.toBeAttached();

    await page.screenshot({ path: `${evidencePath}/tab-close-disabled.png` });
  });

  test('tab with enableClose: true can be closed', async ({ page }) => {
    const closeBtn = findPath(page, '/ts0/tb0/button/close');
    await expect(closeBtn).toBeAttached();
    await closeBtn.click();

    await expect(findTabButton(page, '/ts0', 0).locator('.flexlayout__tab_button_content')).toContainText('Permanent');

    await page.screenshot({ path: `${evidencePath}/tab-close-enabled.png` });
  });
});

// ─── 2.21 Set Component (Programmatic) ───────────────────────────────

test.describe('Tab Feature 2.21: Programmatic Set Component', () => {
  test('action button changes tab component from info to counter', async ({ page }) => {
    await page.goto(baseURL + '?layout=tab_set_component');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const panel = page.locator('[data-testid="panel-Morphable Tab"]');
    await expect(panel).toBeVisible();
    await expect(panel).toContainText('Set Component');

    await page.locator('[data-id="action-set-component"]').click();

    const tabPanel = findPath(page, '/ts0/t0');
    await expect(tabPanel).toBeVisible();
    await expect(tabPanel.locator('button')).toContainText('Increment');

    await page.screenshot({ path: `${evidencePath}/tab-set-component.png` });
  });
});

// ─── 2.22 Set Config (Programmatic) ──────────────────────────────────

test.describe('Tab Feature 2.22: Programmatic Set Config', () => {
  test('action button updates tab config and re-renders content', async ({ page }) => {
    await page.goto(baseURL + '?layout=tab_set_config');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const panel = page.locator('[data-testid="panel-Config Tab"]');
    await expect(panel).toBeVisible();
    await expect(panel).toContainText('Update Config');

    await page.locator('[data-id="action-set-config"]').click();

    const tabPanel = findPath(page, '/ts0/t0');
    await expect(tabPanel).toBeVisible();
    await expect(tabPanel).toContainText('Config updated at');

    await page.screenshot({ path: `${evidencePath}/tab-set-config.png` });
  });
});

// ─── 2.23 Set Icon (Programmatic) ────────────────────────────────────

test.describe('Tab Feature 2.23: Programmatic Set Icon', () => {
  test('toggle icon button changes tab icon src attribute', async ({ page }) => {
    await page.goto(baseURL + '?layout=tab_programmatic_attrs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const tab = findTabButton(page, '/ts0', 0);
    await expect(tab).toBeVisible();
    const img = tab.locator('.flexlayout__tab_button_leading img');
    await expect(img).toBeAttached();
    const initialSrc = await img.getAttribute('src');

    await page.locator('[data-id="action-set-icon"]').click();

    const newSrc = await img.getAttribute('src');
    expect(newSrc).not.toEqual(initialSrc);

    await page.screenshot({ path: `${evidencePath}/tab-set-icon.png` });
  });
});

// ─── 2.24 Set Enable Close (Programmatic) ────────────────────────────

test.describe('Tab Feature 2.24: Programmatic Set Enable Close', () => {
  test('toggle close button disables and re-enables close on tab', async ({ page }) => {
    await page.goto(baseURL + '?layout=tab_programmatic_attrs');
    await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

    const closeBtn = findPath(page, '/ts0/tb0/button/close');
    await expect(closeBtn).toBeAttached();

    await page.locator('[data-id="action-set-close"]').click();

    await expect(closeBtn).not.toBeAttached();

    await page.locator('[data-id="action-set-close"]').click();

    await expect(findPath(page, '/ts0/tb0/button/close')).toBeAttached();

    await page.screenshot({ path: `${evidencePath}/tab-set-enable-close.png` });
  });
});
