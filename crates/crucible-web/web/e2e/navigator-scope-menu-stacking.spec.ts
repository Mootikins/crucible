import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady, openNewSessionTab } from './helpers/nav';

/**
 * E2E: the Navigator's scope swapper (the file-tree root/directory dropdown)
 * opens OVER the center pane instead of behind it.
 *
 * The reported symptom was "drop down for file tree directory/root hides behind
 * the editor/view area to the right". The Navigator renders inside `EdgePanel`,
 * whose slide frame is `overflow-hidden` and whose inner wrapper always carries
 * a `translate` — that is a stacking context AND a containing block, so an
 * in-flow `absolute` menu was clipped at the panel's right edge and painted
 * beneath the center pane no matter what z-index it asked for.
 *
 * Two things are checked, because either alone can pass while the bug is live:
 *  - the menu's box extends past the Navigator, into the center area (not
 *    clipped); and
 *  - a hit test at a point inside that overhang lands on the menu (not on the
 *    center pane painted over it).
 */

// Narrower than the menu (256px), so the overhang is large and deterministic
// rather than depending on the default panel width.
const LEFT_PANEL_WIDTH = 200;

test.describe('Navigator scope menu stacking', () => {
  test('opens over the center pane instead of being clipped behind it', async ({ page }) => {
    await setupBasicMocks(page);
    await page.goto('/');
    await appReady(page);

    // A real center pane to be covered: the New Session composer.
    await openNewSessionTab(page);

    await page.evaluate((width) => {
      const actions = (window as unknown as Record<string, any>).__windowActions;
      actions.setEdgePanelSize('left', width);
    }, LEFT_PANEL_WIDTH);

    const swapper = page.getByTestId('navigator-swapper');
    await expect(swapper).toBeVisible();
    await swapper.click();

    const menu = page.getByTestId('navigator-scope-menu');
    await expect(menu).toBeVisible();

    // The Navigator's rightmost chrome — anything beyond it is the panel edge
    // and then the center area.
    const searchToggle = await page.getByTestId('navigator-search-toggle').boundingBox();
    const menuBox = await menu.boundingBox();
    expect(searchToggle).toBeTruthy();
    expect(menuBox).toBeTruthy();
    if (!searchToggle || !menuBox) return;

    const panelContentRight = searchToggle.x + searchToggle.width;
    const menuRight = menuBox.x + menuBox.width;
    // Not clipped: the menu spills past the panel into the center area.
    expect(menuRight).toBeGreaterThan(panelContentRight);

    // Not painted under: the topmost element at a point inside the overhang
    // belongs to the menu.
    const probe = { x: menuRight - 4, y: menuBox.y + menuBox.height / 2 };
    const hit = await page.evaluate((p) => {
      const el = document.elementFromPoint(p.x, p.y);
      const menuEl = document.querySelector('[data-testid="navigator-scope-menu"]');
      return {
        insideMenu: !!el && !!menuEl && menuEl.contains(el),
        // Named for a readable failure: before the fix this is the center pane.
        topmost: el ? `${el.tagName.toLowerCase()}.${el.className}`.slice(0, 120) : 'none',
      };
    }, probe);
    expect(hit.insideMenu, `topmost element at the overhang was ${hit.topmost}`).toBe(true);

    // Structural guarantee behind both assertions: the menu is portaled out of
    // every clipping/transformed ancestor.
    const portaled = await menu.evaluate(
      (el) => !el.closest('[data-nav-swapper]') && el.parentElement?.parentElement === document.body,
    );
    expect(portaled).toBe(true);
  });
});
