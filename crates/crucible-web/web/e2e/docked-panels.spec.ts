import { test, expect, type Page } from '@playwright/test';

async function waitForDockviewReady(page: Page): Promise<void> {
  await page.waitForSelector('.dockview-theme-abyss', { timeout: 10_000 });
  await page.waitForFunction(() => {
    const dockview = document.querySelector('.dockview-theme-abyss');
    return dockview instanceof HTMLElement && dockview.children.length > 0;
  }, { timeout: 10_000 });
}

test.describe('Docked Panels — Native Dockview Integration', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => localStorage.clear());
    await page.goto('/');
    await waitForDockviewReady(page);
  });

  test('default layout loads with docked panels on all sides', async ({ page }) => {
    const dockedLeft = page.locator('.dv-docked-left-container');
    const dockedRight = page.locator('.dv-docked-right-container');
    const dockedBottom = page.locator('.dv-docked-bottom-container');
    const gridContainer = page.locator('.dv-grid-container');

    await expect(dockedLeft).toBeAttached();
    await expect(dockedRight).toBeAttached();
    await expect(dockedBottom).toBeAttached();
    await expect(gridContainer).toBeAttached();
  });

  test('docked panels have correct flex layout', async ({ page }) => {
    const leftBasis = await page.waitForFunction(() => {
      const el = document.querySelector('.dv-docked-left-container');
      if (!el) return null;
      return getComputedStyle(el).flexBasis;
    });
    const leftBasisValue = await leftBasis.jsonValue();
    expect(leftBasisValue).toMatch(/\d+px/);

    const centerFlex = await page.waitForFunction(() => {
      const el = document.querySelector('.dv-center-container');
      if (!el) return null;
      return getComputedStyle(el).flex;
    });
    const centerFlexValue = await centerFlex.jsonValue();
    expect(centerFlexValue).toContain('1');
  });

  test('keyboard shortcut toggles left docked pane', async ({ page }) => {
    const isMac = process.platform === 'darwin';
    const modifier = isMac ? 'Meta' : 'Control';

    const getLeftWidth = () => page.evaluate(() => {
      const el = document.querySelector('.dv-docked-left-container');
      return el ? getComputedStyle(el).flexBasis : null;
    });

    const initialWidth = await getLeftWidth();
    expect(initialWidth).toMatch(/\d+px/);

    await page.keyboard.press(`${modifier}+KeyB`);

    await page.waitForFunction(() => {
      const el = document.querySelector('.dv-docked-left-container');
      if (!el) return false;
      return getComputedStyle(el).flexBasis === '0px';
    }, { timeout: 5000 });

    const collapsedWidth = await getLeftWidth();
    expect(collapsedWidth).toBe('0px');

    await page.keyboard.press(`${modifier}+KeyB`);

    await page.waitForFunction(() => {
      const el = document.querySelector('.dv-docked-left-container');
      if (!el) return false;
      const basis = getComputedStyle(el).flexBasis;
      return basis !== '0px' && basis.match(/\d+px/);
    }, { timeout: 5000 });

    const expandedWidth = await getLeftWidth();
    expect(expandedWidth).toMatch(/\d+px/);
    expect(expandedWidth).not.toBe('0px');
  });

  test('keyboard shortcut toggles right docked pane', async ({ page }) => {
    const isMac = process.platform === 'darwin';
    const modifier = isMac ? 'Meta' : 'Control';

    await page.keyboard.press(`${modifier}+Shift+KeyB`);

    await page.waitForFunction(() => {
      const el = document.querySelector('.dv-docked-right-container');
      if (!el) return false;
      return getComputedStyle(el).flexBasis === '0px';
    }, { timeout: 5000 });

    const collapsedWidth = await page.evaluate(() => {
      const el = document.querySelector('.dv-docked-right-container');
      return el ? getComputedStyle(el).flexBasis : null;
    });
    expect(collapsedWidth).toBe('0px');
  });

  test('keyboard shortcut toggles bottom docked pane', async ({ page }) => {
    const isMac = process.platform === 'darwin';
    const modifier = isMac ? 'Meta' : 'Control';

    await page.keyboard.press(`${modifier}+KeyJ`);

    await page.waitForFunction(() => {
      const el = document.querySelector('.dv-docked-bottom-container');
      if (!el) return false;
      return getComputedStyle(el).flexBasis === '0px';
    }, { timeout: 5000 });

    const collapsedHeight = await page.evaluate(() => {
      const el = document.querySelector('.dv-docked-bottom-container');
      return el ? getComputedStyle(el).flexBasis : null;
    });
    expect(collapsedHeight).toBe('0px');
  });

  test('floating panels overlap docked areas', async ({ page }) => {
    const overlayContainer = page.locator('.dv-overlay-render-container');
    await expect(overlayContainer).toBeAttached();

    const overlayParent = await page.evaluate(() => {
      const overlay = document.querySelector('.dv-overlay-render-container');
      return overlay?.parentElement?.className || null;
    });

    expect(overlayParent).not.toContain('dv-grid-container');
    expect(overlayParent).not.toContain('dv-docked');
  });

  test('CSS transitions are applied to docked containers', async ({ page }) => {
    const leftTransition = await page.evaluate(() => {
      const el = document.querySelector('.dv-docked-left-container .dv-docked-panel');
      return el ? getComputedStyle(el).transition : null;
    });

    expect(leftTransition).toContain('flex-basis');
    expect(leftTransition).toContain('ease-out');
  });
});
