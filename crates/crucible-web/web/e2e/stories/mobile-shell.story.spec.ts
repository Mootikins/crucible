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

/**
 * A config with a group inside a group, which the shared fixture has none of.
 * The third level is the point: it is what a strip of tabs could never show.
 */
const NESTED_CONFIG = {
  kiln_path: '/home/user/notes',
  config: { chat: { model: 'sonnet' } },
  origins: [],
  controls: {
    read_only: [],
    options: {
      type: 'group',
      args: [
        {
          type: 'group',
          key: 'chat',
          name: 'Chat',
          args: [
            { type: 'toggle', key: 'stream', path: 'chat.stream', name: 'Stream replies' },
            {
              type: 'group',
              key: 'context',
              name: 'Context',
              args: [{ type: 'toggle', key: 'trim', path: 'chat.context.trim', name: 'Trim old turns' }],
            },
          ],
        },
      ],
    },
  },
};

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

  // Settings on a phone is a drill-down, not the desktop's two columns and
  // not a strip of tabs across the top. Section 10a of the design note.
  test('walks settings by drilling in, and back out again', async ({ page }, testInfo) => {
    const story = createStory(testInfo);

    await page.evaluate(() => window.dispatchEvent(new CustomEvent('crucible:open-settings')));
    const dialog = page.getByTestId('settings-modal');
    await expect(dialog).toBeVisible();

    // The root is ONE list of every category. No form is open yet, and the
    // desktop's section list is not rendered at all.
    await expect(dialog.locator('nav')).toHaveCount(0);
    await expect(dialog.getByTestId('settings-nav-appearance')).toBeVisible();
    await expect(dialog.getByTestId('settings-nav-app-config')).toBeVisible();
    await expect(dialog.getByTestId('settings-back')).toHaveCount(0);
    await story.step(page, 'settings root');

    await dialog.getByTestId('settings-nav-appearance').click();
    await expect(dialog.getByRole('heading', { name: 'Appearance' })).toBeVisible();
    // The list it came from is gone, not scrolled past.
    await expect(dialog.getByTestId('settings-nav-app-config')).toHaveCount(0);
    await story.step(page, 'a category');

    await dialog.getByTestId('settings-back').click();
    await expect(dialog.getByTestId('settings-nav-app-config')).toBeVisible();
    await expect(dialog.getByTestId('settings-back')).toHaveCount(0);
  });

  test('drills three levels into the config tree, and the back button unwinds them', async ({
    page,
  }) => {
    await setupBasicMocks(page, { config: NESTED_CONFIG });
    await page.reload();
    await expect(page.getByTestId('mobile-shell')).toBeVisible();

    await page.evaluate(() => window.dispatchEvent(new CustomEvent('crucible:open-settings')));
    const dialog = page.getByTestId('settings-modal');
    const title = dialog.getByRole('heading').first();

    await dialog.getByTestId('settings-nav-app-config').click();
    await expect(title).toHaveText('Configuration');

    // A group is a ROW that opens a page, carrying what it holds.
    const chat = dialog.getByTestId('config-group-chat');
    await expect(chat).toContainText('2 settings');
    await chat.click();
    await expect(title).toHaveText('Chat');

    // A group inside a group drills again — the renderer does not cap depth.
    await dialog.getByTestId('config-group-context').click();
    await expect(title).toHaveText('Context');
    await expect(dialog.getByText('Trim old turns')).toBeVisible();

    // The PHONE's back button, three times, without leaving the page.
    await page.goBack();
    await expect(title).toHaveText('Chat');
    await page.goBack();
    await expect(title).toHaveText('Configuration');
    await page.goBack();
    await expect(title).toHaveText('Settings');
    await expect(dialog).toBeVisible();
  });

  // Closing from depth must give every history entry back, or the next back
  // press walks a dialog that is no longer on screen. Measured on the nav id
  // the shell stamps into history, which is what `NavStack` counts by.
  test('gives the history back when settings close from a deep page', async ({ page }) => {
    const navId = () =>
      page.evaluate(() => (history.state as { crucibleNav?: number } | null)?.crucibleNav ?? 0);
    const before = await navId();

    await page.evaluate(() => window.dispatchEvent(new CustomEvent('crucible:open-settings')));
    const dialog = page.getByTestId('settings-modal');
    await dialog.getByTestId('settings-nav-appearance').click();
    await expect(dialog.getByTestId('settings-back')).toBeVisible();
    expect(await navId()).toBeGreaterThan(before);

    await dialog.getByTestId('settings-modal-close').click();
    await expect(dialog).toHaveCount(0);

    // Back where it started: the drill-down took no entry with it.
    await expect.poll(navId).toBe(before);
    await expect(page.getByTestId('mobile-shell')).toBeVisible();
  });
});
