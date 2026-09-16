import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { waitForFonts } from './stories/_helpers/fonts';

/**
 * Installing a plugin from the settings dialog, end to end in the browser.
 *
 * The fixture repository is LOCAL — a bare repo served from this machine — so
 * the spec never reaches the network, and the URL is one the daemon's own
 * allowlist accepts (`http://`, not a bare path, which it refuses). The clone
 * itself belongs to the daemon and is tested in Rust; what is proved here is
 * the browser half: the URL the user typed reaches `POST /api/plugins`
 * unchanged, one confirmation names it before anything is cloned, and the
 * plugin's DECLARED options render afterwards with no reload.
 */
const FIXTURE_REPO = 'http://127.0.0.1:7391/fixture-plugin.git';

/** What the daemon answers with once the fixture plugin is installed. */
const INSTALLED_TREE = {
  options: {
    'fixture-plugin': {
      type: 'group',
      name: 'Fixture Plugin',
      order: 100,
      args: [
        {
          key: 'greeting',
          type: 'input',
          name: 'Greeting',
          desc: 'What the fixture plugin says.',
          order: 1,
          writable: true,
        },
      ],
    },
  },
};

test.use({ viewport: { width: 1440, height: 900 } });

test('installs a plugin from a pasted URL and shows the options it declares', async ({ page }) => {
  const errs: string[] = [];
  page.on('pageerror', (e) => errs.push(String(e)));
  await setupBasicMocks(page, { sseEvents: [] });

  let installed = false;
  const posted: string[] = [];

  await page.route('**/api/plugins', async (route) => {
    if (route.request().method() === 'POST') {
      const body = JSON.parse(route.request().postData() ?? '{}');
      posted.push(body.url);
      installed = true;
      await route.fulfill({
        json: {
          name: 'fixture-plugin',
          outcome: { kind: 'cloned', dest: '/plugins/fixture-plugin' },
          manifest: '/config/plugins.installed.json',
          installed: true,
          loaded: true,
          tools: 0,
          commands: 0,
          services: 0,
          error: null,
        },
      });
      return;
    }
    await route.fulfill({
      json: {
        plugins: installed
          ? [
              {
                name: 'fixture-plugin',
                version: '0.1.0',
                source: 'User',
                state: 'Active',
                dir: '/plugins/fixture-plugin',
                tools: 0,
                commands: 0,
                handlers: 0,
                services: 0,
              },
            ]
          : [],
      },
    });
  });

  // Nothing declares options until the plugin is installed.
  await page.route('**/api/plugins/options*', (route) =>
    route.fulfill({ json: installed ? INSTALLED_TREE : { options: {} } }),
  );
  await page.route('**/api/plugins/*/option', (route) => route.fulfill({ json: { value: null } }));

  await page.goto('/');
  await waitForFonts(page);
  await page.getByTestId('ribbon-cmd-settings').click();
  await page.getByTestId('settings-modal').waitFor({ state: 'visible' });
  await page.getByTestId('settings-nav-plugins').click();

  await page.getByTestId('plugin-install-url').fill(FIXTURE_REPO);
  await page.getByTestId('plugin-install-submit').click();

  // ONE confirmation, and it names the URL it will clone from.
  const confirm = page.getByTestId('plugin-install-confirm');
  await expect(confirm).toBeVisible();
  await expect(confirm).toContainText(FIXTURE_REPO);
  // Nothing is cloned until the user agrees.
  expect(posted, 'no install before the confirmation').toEqual([]);

  await page.getByTestId('plugin-install-confirm-submit').click();

  // The URL travels to the daemon exactly as it was typed.
  await expect.poll(() => posted).toEqual([FIXTURE_REPO]);

  // And the options the plugin declares are reachable at once — no restart,
  // and no second visit to the dialog.
  const nav = page.getByTestId('settings-nav-plugin:fixture-plugin');
  await expect(nav).toBeVisible();
  await nav.click();
  await expect(page.getByText('What the fixture plugin says.')).toBeVisible();

  expect(errs, 'no page errors').toEqual([]);
});
