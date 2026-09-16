import { test, expect } from '@playwright/test';
import { act, openHarness, readStore } from './harness';

/**
 * Edge modes. This branch types the four modes and marks the rail with its
 * mode. The `flyout` and `hidden` presentations come later; until then they
 * present as `strip`, so this spec asserts only the mark and the store.
 */

test.beforeEach(async ({ page }) => openHarness(page));

test('setEdgeMode hidden marks the left rail, and toggleEdgePanel docks it again', async ({ page }) => {
  const host = page.getByTestId('edge-host-left');
  await expect(host).toHaveAttribute('data-edge-mode', 'docked');

  await act(page, 'setEdgeMode', 'left', 'hidden');
  await expect(host).toHaveAttribute('data-edge-mode', 'hidden');
  expect((await readStore(page)).edgePanels.left.mode).toBe('hidden');
  // Only the named rail changes.
  await expect(page.getByTestId('edge-host-right')).toHaveAttribute('data-edge-mode', 'strip');

  await act(page, 'toggleEdgePanel', 'left');
  await expect(host).toHaveAttribute('data-edge-mode', 'docked');
  expect((await readStore(page)).edgePanels.left.mode).toBe('docked');
  await expect(page.getByTestId('edge-tabbar-left')).toBeVisible();
});
