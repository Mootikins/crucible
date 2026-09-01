import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';
import { waitForFonts } from './stories/_helpers/fonts';

/**
 * A plugin this file has never heard of, with four control kinds.
 *
 * The name is deliberate. Nothing in the settings modal, its section registry,
 * or the option renderer may contain a branch on a plugin's identity — the tree
 * is declared in Lua and projected, and a plugin shipped tomorrow must get its
 * own entry in the left list for free. A spec naming a real plugin could pass
 * while that property was quietly broken.
 */
const TREE = {
  options: {
    'never-heard-of-this-one': {
      type: 'group',
      name: 'Widget Factory',
      order: 100,
      args: [
        { key: 'shade', type: 'input', name: 'Shade', desc: 'Colour of the widget.', order: 1, writable: true },
        { key: 'loud', type: 'toggle', name: 'Loud', order: 2, writable: true },
        { key: 'runtime', type: 'select', name: 'Runtime', order: 3, writable: true,
          values: [{ value: 'podman', label: 'podman' }, { value: 'docker', label: 'docker' }] },
        { key: 'jobs', type: 'range', name: 'Jobs', order: 4, min: 1, max: 8, step: 1, writable: true },
      ],
    },
  },
};

test.use({ viewport: { width: 1440, height: 900 } });

test('a plugin gets its own settings pane', async ({ page }) => {
  const errs: string[] = [];
  page.on('pageerror', (e) => errs.push(String(e)));
  await setupBasicMocks(page, { sseEvents: [] });
  await page.route('**/api/plugins/options*', (r) => r.fulfill({ json: TREE }));
  await page.route('**/api/plugins/*/option', (r) => r.fulfill({ json: { value: null } }));
  await page.goto('/');
  await waitForFonts(page);
  await page.getByTestId('ribbon-cmd-settings').click();
  await page.getByTestId('settings-modal').waitFor({ state: 'visible' });

  // The left list grew a Plugins group with an entry this file never named.
  // No sleep: `toBeVisible` polls, and the entry cannot appear until the
  // plugin trees have arrived — which is the condition, stated directly.
  const nav = page.getByTestId('settings-nav-plugin:never-heard-of-this-one');
  await expect(nav).toBeVisible();
  await nav.click();
  await expect(page.getByText('Shade')).toBeVisible();
  await expect(page.getByText('Colour of the widget.')).toBeVisible();
  // Its controls render in the settings table idiom, not as a foreign panel.
  await expect(page.getByTestId('plugin-option-runtime').locator('select')).toBeVisible();
  await expect(page.getByTestId('plugin-option-loud').locator('input[type=checkbox]')).toBeVisible();

  // And the app's own sections are still reachable beside it.
  await page.getByTestId('settings-nav-appearance').click();
  await expect(page.getByTestId('settings-nav-plugin:never-heard-of-this-one')).toBeVisible();

  expect(errs, 'no page errors').toEqual([]);
});
