import { test, expect } from '@playwright/test';
import {
  findPath,
  findTabButton,
  findAllTabSets,
} from './helpers';

const baseURL = '/flexlayout-test.html';
const evidencePath = '../../../.sisyphus/evidence';

// ─── Visual Fixes Test Suite ────────────────────────────────────────────────
// Comprehensive E2E tests for all 13 visual issues fixed in Tasks 1-6

test.describe('FlexLayout Visual Fixes', () => {
  // ─── Issue #1: Tab bar indistinguishable from content area ────────────────

  test.describe('Issue #1: Tab bar distinct from content area', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(baseURL + '?layout=test_two_tabs');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    });

    test('tab bar background differs from content background', async ({ page }) => {
      const tabBar = findPath(page, '/ts0/tabstrip');
      const content = findPath(page, '/ts0/t0');

      const tabBarBg = await tabBar.evaluate(el =>
        getComputedStyle(el).backgroundColor
      );
      const contentBg = await content.evaluate(el =>
        getComputedStyle(el).backgroundColor
      );

      expect(tabBarBg).not.toBe(contentBg);
    });

    test('tab bar has visible bottom border', async ({ page }) => {
      const tabBar = findPath(page, '/ts0/tabstrip');
      const borderWidth = await tabBar.evaluate(el =>
        getComputedStyle(el).borderBottomWidth
      );

      expect(borderWidth).not.toBe('0px');
    });
  });

  // ─── Issue #2: Splitter invisible ──────────────────────────────────────────

  test.describe('Issue #2: Splitter visible and distinguishable', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(baseURL + '?layout=test_two_tabs');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    });

    test('splitter background differs from panel background', async ({ page }) => {
      const splitter = page.locator('.flexlayout__splitter').first();
      const panel = page.locator('.flexlayout__row').first();

      const splitterBg = await splitter.evaluate(el =>
        getComputedStyle(el).backgroundColor
      );
      const panelBg = await panel.evaluate(el =>
        getComputedStyle(el).backgroundColor
      );

      expect(splitterBg).not.toBe(panelBg);
    });

    test('splitter shows hover state', async ({ page }) => {
      const splitter = page.locator('.flexlayout__splitter').first();

      const bgBefore = await splitter.evaluate(el =>
        getComputedStyle(el).backgroundColor
      );

      await splitter.hover();

      const bgAfter = await splitter.evaluate(el =>
        getComputedStyle(el).backgroundColor
      );

      expect(bgAfter).not.toBe(bgBefore);
    });

    test('border splitter is also visible', async ({ page }) => {
      await page.goto(baseURL + '?layout=test_with_borders');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

      const borderSplitter = page.locator('.flexlayout__splitter_border').first();
      const borderWidth = await borderSplitter.evaluate(el =>
        getComputedStyle(el).width
      );

      expect(borderWidth).not.toBe('0px');
    });
  });

  // ─── Issue #3: Maximize button invisible ───────────────────────────────────

  test.describe('Issue #3: Maximize button visible with hover state', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(baseURL + '?layout=test_two_tabs');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    });

    test('maximize button has sufficient opacity', async ({ page }) => {
      const maxButton = page.locator('.flexlayout__tab_toolbar_button').first();
      const opacity = await maxButton.evaluate(el =>
        getComputedStyle(el).opacity
      );

      const opacityValue = parseFloat(opacity);
      expect(opacityValue).toBeGreaterThanOrEqual(0.6);
    });

    test('maximize button font size is readable', async ({ page }) => {
      const maxButton = page.locator('.flexlayout__tab_toolbar_button').first();
      const fontSize = await maxButton.evaluate(el =>
        getComputedStyle(el).fontSize
      );

      const sizeValue = parseFloat(fontSize);
      expect(sizeValue).toBeGreaterThanOrEqual(12);
    });

    test('maximize button shows hover state', async ({ page }) => {
      const maxButton = page.locator('.flexlayout__tab_toolbar_button').first();

      const bgBefore = await maxButton.evaluate(el =>
        getComputedStyle(el).backgroundColor
      );

      await maxButton.hover();

      const bgAfter = await maxButton.evaluate(el =>
        getComputedStyle(el).backgroundColor
      );

      expect(bgAfter).not.toBe(bgBefore);
    });
  });

  // ─── Issue #4: Close button hover not visible ──────────────────────────────

  test.describe('Issue #4: Close button has visible hover state', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(baseURL + '?layout=test_two_tabs');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    });

    test('close button shows background on hover', async ({ page }) => {
      const closeButton = page.locator('.flexlayout__tab_button_trailing').first();

      const bgBefore = await closeButton.evaluate(el =>
        getComputedStyle(el).backgroundColor
      );

      await closeButton.hover();

      const bgAfter = await closeButton.evaluate(el =>
        getComputedStyle(el).backgroundColor
      );

      expect(bgAfter).not.toBe(bgBefore);
    });

    test('close button has minimum touch target size', async ({ page }) => {
      const closeButton = page.locator('.flexlayout__tab_button_trailing').first();
      const box = await closeButton.boundingBox();

      expect(box?.width).toBeGreaterThanOrEqual(16);
      expect(box?.height).toBeGreaterThanOrEqual(16);
    });
  });

  // ─── Issue #5: Active/inactive tab distinction ─────────────────────────────

  test.describe('Issue #5: Active tab visually distinct from inactive', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(baseURL + '?layout=test_two_tabs');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    });

    test('selected tab has different background than unselected', async ({ page }) => {
      const selectedTab = findTabButton(page, '/ts0', 0);
      const unselectedTab = findTabButton(page, '/ts0', 1);

      const selectedBg = await selectedTab.evaluate(el =>
        getComputedStyle(el).backgroundColor
      );
      const unselectedBg = await unselectedTab.evaluate(el =>
        getComputedStyle(el).backgroundColor
      );

      expect(selectedBg).not.toBe(unselectedBg);
    });

    test('selected tab has --selected class', async ({ page }) => {
      const selectedTab = findTabButton(page, '/ts0', 0);
      const classes = await selectedTab.getAttribute('class');

      expect(classes).toContain('flexlayout__tab_button--selected');
    });
  });

  // ─── Issue #6: Low contrast dark-on-dark ──────────────────────────────────

  test.describe('Issue #6: Text contrast meets WCAG AA', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(baseURL + '?layout=test_two_tabs');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    });

    test('muted text has sufficient contrast', async ({ page }) => {
      // Find an element with muted text color
      const element = page.locator('.flexlayout__tab_button_content').first();

      const color = await element.evaluate(el =>
        getComputedStyle(el).color
      );
      const bgColor = await element.evaluate(el => {
        const parent = el.parentElement;
        return parent ? getComputedStyle(parent).backgroundColor : '#0a0a0a';
      });

      // Parse RGB values and calculate contrast ratio
      const parseRgb = (rgb: string) => {
        const match = rgb.match(/\d+/g);
        return match ? match.map(Number) : [0, 0, 0];
      };

      const [r1, g1, b1] = parseRgb(color);
      const [r2, g2, b2] = parseRgb(bgColor);

      const getLuminance = (r: number, g: number, b: number) => {
        const [rs, gs, bs] = [r, g, b].map(x => {
          x = x / 255;
          return x <= 0.03928 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4);
        });
        return 0.2126 * rs + 0.7152 * gs + 0.0722 * bs;
      };

      const l1 = getLuminance(r1, g1, b1);
      const l2 = getLuminance(r2, g2, b2);
      const contrast = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);

      expect(contrast).toBeGreaterThanOrEqual(4.5);
    });

    test('border color is visible', async ({ page }) => {
      const tabBar = findPath(page, '/ts0/tabstrip');
      const borderColor = await tabBar.evaluate(el =>
        getComputedStyle(el).borderBottomColor
      );

      // Border should not be transparent or same as background
      expect(borderColor).not.toBe('rgba(0, 0, 0, 0)');
      expect(borderColor).not.toBe('transparent');
    });
  });

  // ─── Issue #7: Dock toggle buttons too small ───────────────────────────────

  test.describe('Issue #7: Dock toggle buttons visible and interactive', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(baseURL + '?layout=test_with_borders');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    });

    test('dock button font size is readable', async ({ page }) => {
      const dockButton = page.locator('.flexlayout__border_button').first();
      const fontSize = await dockButton.evaluate(el =>
        getComputedStyle(el).fontSize
      );

      const sizeValue = parseFloat(fontSize);
      expect(sizeValue).toBeGreaterThanOrEqual(14);
    });

    test('dock button has sufficient opacity', async ({ page }) => {
      const dockButton = page.locator('.flexlayout__border_button').first();
      const opacity = await dockButton.evaluate(el =>
        getComputedStyle(el).opacity
      );

      const opacityValue = parseFloat(opacity);
      expect(opacityValue).toBeGreaterThanOrEqual(0.85);
    });

    test('dock button shows hover state', async ({ page }) => {
      const dockButton = page.locator('.flexlayout__border_button').first();

      const bgBefore = await dockButton.evaluate(el =>
        getComputedStyle(el).backgroundColor
      );

      await dockButton.hover();

      const bgAfter = await dockButton.evaluate(el =>
        getComputedStyle(el).backgroundColor
      );

      expect(bgAfter).not.toBe(bgBefore);
    });
  });

  // ─── Issue #8: Rotated labels hard to read ─────────────────────────────────

  test.describe('Issue #8: Rotated labels readable', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(baseURL + '?layout=test_with_borders');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    });

    test('collapsed label font size is sufficient', async ({ page }) => {
      const collapsedLabel = page.locator('.flexlayout__border_button_content').first();
      const fontSize = await collapsedLabel.evaluate(el =>
        getComputedStyle(el).fontSize
      );

      const sizeValue = parseFloat(fontSize);
      expect(sizeValue).toBeGreaterThanOrEqual(13);
    });
  });

  // ─── Issue #9: Tab icons not rendering ─────────────────────────────────────

  test.describe('Issue #9: Tab icons render correctly', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(baseURL + '?layout=basic_icons');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    });

    test('emoji icons render as text, not broken img tags', async ({ page }) => {
      const iconContainer = page.locator('.flexlayout__tab_button_leading').first();
      const imgCount = await iconContainer.locator('img').count();

      expect(imgCount).toBe(0);
    });

    test('emoji icons are visible in tab headers', async ({ page }) => {
      const iconContainer = page.locator('.flexlayout__tab_button_leading').first();
      const text = await iconContainer.textContent();

      expect(text).toBeTruthy();
      expect(text?.length).toBeGreaterThan(0);
    });

    test('icon text is properly sized', async ({ page }) => {
      const iconSpan = page.locator('.flexlayout__tab_button_icon_text').first();
      const fontSize = await iconSpan.evaluate(el =>
        getComputedStyle(el).fontSize
      );

      const sizeValue = parseFloat(fontSize);
      expect(sizeValue).toBeGreaterThan(0);
    });

    test('URL icons still render as img tags', async ({ page }) => {
      await page.goto(baseURL + '?layout=test_three_tabs');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

      const imgCount = await page.locator('.flexlayout__tab_button_leading img').count();
      expect(imgCount).toBeGreaterThanOrEqual(1);
    });
  });

  // ─── Issue #10: Close type behavior not respected ──────────────────────────

  test.describe('Issue #10: Close type 0/1/2 behavior respected', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(baseURL + '?layout=tab_close_types');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    });

    test('close type 0 tabs always show close button', async ({ page }) => {
      // Find tab with "Close Type 0" text
      const tab = page.locator('.flexlayout__tab_button_content').filter({ hasText: /Close Type 0/ }).first();
      const tabButton = tab.locator('..').first();
      const closeButton = tabButton.locator('.flexlayout__tab_button_trailing');

      const visibility = await closeButton.evaluate(el =>
        getComputedStyle(el).visibility
      );

      expect(visibility).toBe('visible');
    });

    test('close type 1 tabs show close button only on hover', async ({ page }) => {
      // Find tab with "Close Type 1" text
      const tab = page.locator('.flexlayout__tab_button_content').filter({ hasText: /Close Type 1/ }).first();
      const tabButton = tab.locator('..').first();
      const closeButton = tabButton.locator('.flexlayout__tab_button_trailing');

      // Before hover: should be hidden
      const visibilityBefore = await closeButton.evaluate(el =>
        getComputedStyle(el).visibility
      );
      expect(visibilityBefore).toBe('hidden');

      // After hover: should be visible
      await tabButton.hover();
      const visibilityAfter = await closeButton.evaluate(el =>
        getComputedStyle(el).visibility
      );
      expect(visibilityAfter).toBe('visible');
    });

    test('close type 2 tabs never show close button', async ({ page }) => {
      // Find tab with "Close Type 2" text
      const tab = page.locator('.flexlayout__tab_button_content').filter({ hasText: /Close Type 2/ }).first();
      const tabButton = tab.locator('..').first();
      const closeButtonCount = await tabButton.locator('.flexlayout__tab_button_trailing').count();

      expect(closeButtonCount).toBe(0);
    });

    test('enableClose false overrides all close types', async ({ page }) => {
      // Find tab with "No Close" text (enableClose: false)
      const tab = page.locator('.flexlayout__tab_button_content').filter({ hasText: /No Close/ }).first();
      const tabButton = tab.locator('..').first();
      const closeButtonCount = await tabButton.locator('.flexlayout__tab_button_trailing').count();

      expect(closeButtonCount).toBe(0);
    });
  });

  // ─── Issue #11: Bottom tab strip position ──────────────────────────────────

  test.describe('Issue #11: Bottom tab strip renders below content', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(baseURL + '?layout=tabset_bottom_tabs');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    });

    test('bottom tab strip renders below content area', async ({ page }) => {
      const content = findPath(page, '/ts0/t0');
      const tabstrip = findPath(page, '/ts0/tabstrip');

      const contentBox = await content.boundingBox();
      const tabstripBox = await tabstrip.boundingBox();

      expect(contentBox).toBeTruthy();
      expect(tabstripBox).toBeTruthy();

      if (contentBox && tabstripBox) {
        expect(tabstripBox.y).toBeGreaterThan(contentBox.y);
      }
    });

    test('bottom tab strip has top border', async ({ page }) => {
      const tabstrip = findPath(page, '/ts0/tabstrip');
      const borderTopWidth = await tabstrip.evaluate(el =>
        getComputedStyle(el).borderTopWidth
      );

      expect(borderTopWidth).not.toBe('0px');
    });

    test('bottom tab strip CSS class is applied', async ({ page }) => {
      const tabstrip = findPath(page, '/ts0/tabstrip');
      const classes = await tabstrip.getAttribute('class');

      expect(classes).toContain('flexlayout__tabset_tabbar_outer_bottom');
    });
  });

  // ─── Issue #12: Tab container padding too tight ────────────────────────────

  test.describe('Issue #12: Tab container has adequate padding', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(baseURL + '?layout=test_two_tabs');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    });

    test('tab button has sufficient horizontal padding', async ({ page }) => {
      const tabButton = findTabButton(page, '/ts0', 0);
      const paddingLeft = await tabButton.evaluate(el =>
        getComputedStyle(el).paddingLeft
      );
      const paddingRight = await tabButton.evaluate(el =>
        getComputedStyle(el).paddingRight
      );

      const leftValue = parseFloat(paddingLeft);
      const rightValue = parseFloat(paddingRight);

      expect(leftValue).toBeGreaterThanOrEqual(8);
      expect(rightValue).toBeGreaterThanOrEqual(8);
    });
  });

  // ─── Issue #13: Overflow button missing ────────────────────────────────────

  test.describe('Issue #13: Overflow button visible when needed', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto(baseURL + '?layout=test_two_tabs');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
    });

    test('overflow button has hover state', async ({ page }) => {
      const overflowButton = page.locator('.flexlayout__tab_overflow_button').first();

      // Check if overflow button exists
      const count = await overflowButton.count();
      if (count > 0) {
        const bgBefore = await overflowButton.evaluate(el =>
          getComputedStyle(el).backgroundColor
        );

        await overflowButton.hover();

        const bgAfter = await overflowButton.evaluate(el =>
          getComputedStyle(el).backgroundColor
        );

        expect(bgAfter).not.toBe(bgBefore);
      }
    });
  });

  // ─── Regression: All 5 key layouts render without errors ────────────────────

  test.describe('Regression: All key layouts render correctly', () => {
    const layouts = [
      'test_two_tabs',
      'test_with_borders',
      'basic_icons',
      'tab_close_types',
      'tabset_bottom_tabs',
    ];

    for (const layout of layouts) {
      test(`${layout} renders without errors`, async ({ page }) => {
        await page.goto(baseURL + `?layout=${layout}`);
        await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });

        // Check for console errors
        const errors: string[] = [];
        page.on('console', msg => {
          if (msg.type() === 'error') {
            errors.push(msg.text());
          }
        });

        // Wait a bit for any async errors
        await page.waitForTimeout(500);

        expect(errors).toHaveLength(0);
      });
    }
  });

  // ─── Screenshot Evidence ──────────────────────────────────────────────────

  test.describe('Screenshot Evidence', () => {
    test('capture after-test-two-tabs.png', async ({ page }) => {
      await page.goto(baseURL + '?layout=test_two_tabs');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
      await page.screenshot({ path: `${evidencePath}/after-test-two-tabs.png` });
    });

    test('capture after-test-with-borders.png', async ({ page }) => {
      await page.goto(baseURL + '?layout=test_with_borders');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
      await page.screenshot({ path: `${evidencePath}/after-test-with-borders.png` });
    });

    test('capture after-basic-icons.png', async ({ page }) => {
      await page.goto(baseURL + '?layout=basic_icons');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
      await page.screenshot({ path: `${evidencePath}/after-basic-icons.png` });
    });

    test('capture after-tab-close-types.png', async ({ page }) => {
      await page.goto(baseURL + '?layout=tab_close_types');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
      await page.screenshot({ path: `${evidencePath}/after-tab-close-types.png` });
    });

    test('capture after-tabset-bottom-tabs.png', async ({ page }) => {
      await page.goto(baseURL + '?layout=tabset_bottom_tabs');
      await page.waitForSelector('[data-layout-path="/"]', { timeout: 10_000 });
      await page.screenshot({ path: `${evidencePath}/after-tabset-bottom-tabs.png` });
    });
  });
});
