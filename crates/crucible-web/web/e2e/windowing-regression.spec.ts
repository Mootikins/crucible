import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';

/**
 * E2E: Windowing System Regression Guard
 *
 * Verifies that the core windowing system (WindowManager, layout, edge panels)
 * remains unbroken. This test protects against accidental modifications to:
 * - windowStore.ts
 * - WindowManager.tsx
 * - SplitPane.tsx
 * - CenterTiling.tsx
 * - layout-serializer.ts
 * - layout-persistence.ts
 * - windowTypes.ts
 *
 * Mocked API throughout: the vite dev server proxies /api to whatever runs
 * on :3000, so an unmocked run against a live daemon imports a real saved
 * layout mid-test and races the interactions below.
 */

test.beforeEach(async ({ page }) => {
  await setupBasicMocks(page);
  // The pop-out/dock tests open a real file tab; serve its bytes.
  await page.route('**/api/kiln/file**', (route) => {
    const m = route.request().method();
    if (m === 'GET') return route.fulfill({ json: { content: '# note\n' } });
    if (m === 'PUT') return route.fulfill({ status: 200, body: '' });
    return route.continue();
  });
});

test('the shell boots without errors and preserves its regions when toggled', async ({ page }) => {
  const errors: string[] = [];
  page.on('console', (msg) => {
    if (msg.type() === 'error' && !/Failed to load resource|ECONNREFUSED|http proxy error/.test(msg.text())) {
      errors.push(msg.text());
    }
  });
  await page.goto('/');
  const root = page.locator('div.flex.flex-col.h-screen.bg-shell-bg');
  const center = page.locator('div.flex-1.flex.flex-col.overflow-hidden').first();
  const toggle = page.getByTestId('ribbon-toggle-left');
  await expect(root).toBeVisible();
  await expect(center).toBeVisible();
  expect(await page.locator('div.flex-1').count()).toBeGreaterThan(0);
  // Left ribbon's bottom cluster: the three toggles that act on the whole
  // shell. The palette bolt and the new-session plus are deliberately gone —
  // each was a third doorway to an action with a shorter one (Ctrl+P,
  // Ctrl+Shift+N), spending the rail's most reachable pixels.
  await expect(page.getByTestId('ribbon-cmd-swap-sides')).toBeVisible();
  await expect(page.getByTestId('ribbon-cmd-theme')).toBeVisible();
  await expect(page.getByTestId('ribbon-cmd-settings')).toBeVisible();
  await expect(page.getByTestId('ribbon-cmd-palette')).toHaveCount(0);
  await expect(page.getByTestId('ribbon-cmd-new-session')).toHaveCount(0);

  // Both edges expose their own toggle. There is no third: the bottom dock is
  // gone, and the terminal it held is a pane under the file tree.
  await expect(page.getByTestId('ribbon-toggle-right')).toBeVisible();
  await expect(page.getByTestId('ribbon-toggle-bottom')).toHaveCount(0);

  // The header bar is gone: its Inbox pill and Ctrl+P kbd hint (the ribbon's
  // palette button shares the palette title, so Inbox is the discriminator)
  // no longer exist anywhere.
  await expect(page.locator('button[title="Inbox"]')).toHaveCount(0);
  // One surface still prints that chord: the empty-pane affordance, which
  // names the keys that fill the pane it sits in. Counting the two locators
  // against each other says every Ctrl+P chip on screen belongs to that
  // affordance — no header bar reintroduced one.
  const paletteHints = page.locator('kbd:has-text("Ctrl+P")');
  const affordanceHints = page.locator('[data-testid="empty-pane"] kbd:has-text("Ctrl+P")');
  expect(await paletteHints.count()).toBe(await affordanceHints.count());

  await expect(toggle).toHaveAttribute('title', 'Collapse panel');
  for (const title of ['Expand panel', 'Collapse panel']) {
    await toggle.click();
    await expect(toggle).toHaveAttribute('title', title);
    await expect(root).toBeVisible();
    await expect(center).toBeVisible();
    await expect(toggle).toBeVisible();
  }
  expect(errors).toEqual([]);
});

test('pop-out MOVES the tabs to a floating window (no mirrored group)', async ({ page }) => {
  // The DnD registry must stay coherent while the group moves between tab
  // bars — a "Cannot remove nonexistent draggable/droppable" warning means a
  // cleanup stole the new container's registration (tab silently undraggable).
  const dndWarnings: string[] = [];
  page.on('console', (msg) => {
    if (msg.text().includes('nonexistent')) dndWarnings.push(msg.text());
  });
  await page.goto('/');

  // Put a file tab in the center pane via the product open-file path.
  await page.evaluate(() => {
    window.dispatchEvent(
      new CustomEvent('crucible:open-file', {
        detail: { path: '/kiln/popout-note.md', name: 'popout-note.md' },
      }),
    );
  });
  await expect(page.locator('[data-tab-id^="tab-file-"]')).toHaveCount(1);

  // Pop the pane out.
  await page.locator('button[title="Pop out to floating window"]').first().click();

  // A floating window appears, titled after the tab...
  const floating = page.locator('div.absolute.flex.flex-col').filter({ hasText: 'popout-note.md' });
  await expect(floating.first()).toBeVisible();

  // ...and the tab exists exactly ONCE across all tab strips. The old
  // pop-out shared the group between pane and window: two tab strips, two
  // solid-dnd registrations under one id (drag silently broke).
  await expect(page.locator('[data-tab-id^="tab-file-"]')).toHaveCount(1);

  // Closing the floating window closes its tabs with it — nothing orphaned.
  await page.locator('button[title="Close (closes its tabs)"]').click();
  await expect(page.locator('[data-tab-id^="tab-file-"]')).toHaveCount(0);
  expect(dndWarnings).toEqual([]);
});

test('dock button moves a floating window back into the layout', async ({ page }) => {
  await page.goto('/');

  await page.evaluate(() => {
    window.dispatchEvent(
      new CustomEvent('crucible:open-file', {
        detail: { path: '/kiln/dock-note.md', name: 'dock-note.md' },
      }),
    );
  });
  await expect(page.locator('[data-tab-id^="tab-file-"]')).toHaveCount(1);

  await page.locator('button[title="Pop out to floating window"]').first().click();
  await expect(page.locator('button[title="Dock back into the layout"]')).toBeVisible();

  await page.locator('button[title="Dock back into the layout"]').click();

  // The floating window is gone and the tab is back in a pane, still unique.
  await expect(page.locator('button[title="Dock back into the layout"]')).toHaveCount(0);
  await expect(page.locator('[data-tab-id^="tab-file-"]')).toHaveCount(1);
});
