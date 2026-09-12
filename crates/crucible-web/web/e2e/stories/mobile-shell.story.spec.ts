import { test, expect, devices } from '@playwright/test';
import { setupBasicMocks } from '../helpers/mock-api';
import { createStory } from './_helpers/story';
import { waitForFonts } from './_helpers/fonts';

/**
 * Story: WS-317 — Crucible on a phone.
 *
 * The compact shell only exists below 768 px, so this is the one spec that
 * runs at a phone's viewport. It walks what a user actually does: open the
 * left drawer, move between its two tabs, open the note's context on the
 * right, reach the rest from the overflow menu, and close a drawer with the
 * hardware back button without leaving the page.
 */
test.use({ ...devices['Pixel 7'] });

test.describe('WS-317 the compact shell', () => {
  test.beforeEach(async ({ page }) => {
    await setupBasicMocks(page);
    await page.goto('/');
    await expect(page.getByTestId('mobile-shell')).toBeVisible();
    await waitForFonts(page);
  });

  test('draws the phone shell, not the window manager', async ({ page }) => {
    // The desktop shell's rails and tab strips must be absent.
    await expect(page.getByTestId('mobile-shell')).toBeVisible();
    await expect(page.locator('[data-testid^="edge-collapsed-drop-"]')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Sessions and files' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Backlinks' })).toBeVisible();
  });

  test('walks the drawers, the tabs and the menu', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await story.step(page, 'shell at rest');

    await page.getByRole('button', { name: 'Sessions and files' }).click();
    await expect(page.getByRole('dialog', { name: 'Sessions and files' })).toBeVisible();
    await story.step(page, 'sessions drawer');

    await page.getByRole('tab', { name: 'Files' }).click();
    await expect(page.getByRole('tab', { name: 'Files' })).toHaveAttribute('aria-selected', 'true');
    await story.step(page, 'files tab');

    // Back closes the drawer and stays on the page — a phone's only Escape.
    await page.goBack();
    await expect(page.getByTestId('drawer-left')).toHaveAttribute('inert', '');
    expect(new URL(page.url()).pathname).toBe('/');

    await page.getByRole('button', { name: 'Backlinks' }).click();
    await expect(page.getByRole('dialog', { name: 'Backlinks' })).toBeVisible();
    await story.step(page, 'backlinks drawer');
    // The scrim covers the viewport, but its centre lies under the panel.
    // Tapping "outside" means the strip the panel does not cover.
    await page.getByTestId('drawer-scrim-right').click({ position: { x: 5, y: 300 } });
    await expect(page.getByTestId('drawer-right')).toHaveAttribute('inert', '');

    await page.getByRole('button', { name: 'More' }).click();
    await expect(page.getByRole('dialog', { name: 'More' })).toBeVisible();
    await story.step(page, 'overflow menu');
    await page.getByRole('button', { name: 'Search', exact: true }).click();
    await expect(page.getByRole('button', { name: /^Tabs/ })).toBeVisible();
    await story.step(page, 'search as a content tab');
  });

  test('starts a session in three steps', async ({ page }, testInfo) => {
    const story = createStory(testInfo);
    await page.evaluate(() => window.dispatchEvent(new CustomEvent('crucible:new-session')));

    await expect(page.getByRole('heading', { name: 'Agent' })).toBeVisible();
    await story.step(page, 'pick an agent');

    await page.getByRole('button', { name: 'Next' }).click();
    for (const row of ['Project', 'Workspace', 'Kiln', 'Model', 'Runtime']) {
      await expect(page.getByRole('button', { name: new RegExp(`^${row}`) })).toBeVisible();
    }
    await story.step(page, 'the five context rows');

    await page.getByRole('button', { name: 'Next' }).click();
    await expect(page.getByRole('button', { name: 'Send' })).toBeDisabled();
    await page.getByRole('textbox', { name: 'Message' }).fill('summarise my notes');
    await expect(page.getByRole('button', { name: 'Send' })).toBeEnabled();
    await story.step(page, 'the first message');
  });
});
