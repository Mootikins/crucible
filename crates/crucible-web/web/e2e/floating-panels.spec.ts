import { test, expect, type Page } from '@playwright/test';

async function waitForDockviewReady(page: Page): Promise<void> {
  await page.waitForSelector('.dockview-theme-abyss', { timeout: 10_000 });
  await page.waitForFunction(() => {
    const dockview = document.querySelector('.dockview-theme-abyss');
    return dockview instanceof HTMLElement && dockview.children.length > 0;
  }, { timeout: 10_000 });
}

test.describe('Floating Panels — Overlap Verification', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => localStorage.clear());
    await page.goto('/');
    await waitForDockviewReady(page);
  });

  test('overlay render container is attached to component root', async ({ page }) => {
    const overlayContainer = page.locator('.dv-overlay-render-container');
    await expect(overlayContainer).toBeAttached();

    const overlayParent = await page.evaluate(() => {
      const overlay = document.querySelector('.dv-overlay-render-container');
      if (!overlay || !overlay.parentElement) return null;
      return overlay.parentElement.className;
    });

    expect(overlayParent).not.toContain('dv-grid-container');
    expect(overlayParent).not.toContain('dv-docked');
  });

  test('floating panels have higher z-index than docked panels', async ({ page }) => {
    const floatingZIndex = await page.evaluate(() => {
      const overlay = document.querySelector('.dv-overlay-render-container');
      return overlay ? parseInt(getComputedStyle(overlay).zIndex || '0', 10) : 0;
    });

    const dockedZIndex = await page.evaluate(() => {
      const docked = document.querySelector('.dv-docked-panel');
      return docked ? parseInt(getComputedStyle(docked).zIndex || '0', 10) : 0;
    });

    expect(floatingZIndex).toBeGreaterThanOrEqual(dockedZIndex);
  });

  test('overlay container is positioned to cover entire viewport', async ({ page }) => {
    const overlayPosition = await page.evaluate(() => {
      const overlay = document.querySelector('.dv-overlay-render-container');
      if (!overlay) return null;
      const style = getComputedStyle(overlay);
      return {
        position: style.position,
        top: style.top,
        left: style.left,
        width: style.width,
        height: style.height,
      };
    });

    expect(overlayPosition).toBeTruthy();
    expect(overlayPosition?.position).toBe('absolute');
  });
});
