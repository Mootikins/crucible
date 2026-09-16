import { test, expect } from '@playwright/test';
import { setupBasicMocks } from './helpers/mock-api';

/**
 * The right edge panel opens with a 200ms tween written in JavaScript, not CSS
 * — deliberately, because a CSS width transition runs on the main thread while
 * the inner `translate` runs on the compositor, and under load the two desync
 * and the panel tears against its own clip edge.
 *
 * That choice had a cost nobody had written down: `index.css` zeroes every
 * animation under `prefers-reduced-motion`, and this one animation, being JS,
 * silently opted out of that promise. A viewer who asked for no motion still
 * got the largest moving thing in the shell.
 *
 * It also made the panel a moving target for the whole E2E tier. The suite's
 * own `disableAnimations()` helper only zeroes CSS durations, so it advertised
 * a stillness it could not deliver, and `root-dropdown-pick` failed about one
 * run in ten because a click landed in the tween's tail.
 *
 * This gate asserts the MECHANISM rather than counting repeats: it samples the
 * control's position on twenty consecutive frames and requires it never to
 * move. A repeat count can only ever show a race is rarer; this shows it is
 * absent.
 */
test.describe('the edge panel under a reduced-motion preference', () => {
  test.use({ reducedMotion: 'reduce' });

  test('opens without moving its controls across frames', async ({ page }) => {
    await setupBasicMocks(page, { sseEvents: [] });
    await page.goto('/');
    await page.getByTestId('ribbon-toggle-left').waitFor({ state: 'visible' });

    // Sample from INSIDE the animating frame. The rail toggle sits outside it
    // and never moves, so measuring that would pass whether the tween ran or
    // not — the first version of this gate did exactly that and passed with
    // the fix removed, which is the failure mode a gate exists to avoid.
    const lefts = await page.evaluate(async () => {
      const store = (window as unknown as Record<string, any>).__windowStore;
      const actions = (window as unknown as Record<string, any>).__windowActions;
      const firstGroup = (node: any): string | null => {
        if (!node || typeof node !== 'object') return null;
        if (node.type === 'pane') return node.tabGroupId ?? null;
        return firstGroup(node.first) ?? firstGroup(node.second);
      };
      const groupId = firstGroup(store.edgePanels.right.layout);
      if (!groupId) throw new Error('right edge panel has no tab group');
      actions.setActiveTab(groupId, 'files-tab');

      // Toggle in the same turn we start sampling in: the tween's first frame
      // is precisely what a click would otherwise race.
      if (store.edgePanels?.right?.mode === 'strip') actions.toggleEdgePanel('right');

      const probe = () => {
        const el = document.querySelector('[data-testid="root-dropdown"]');
        return el ? Math.round(el.getBoundingClientRect().left) : -1;
      };
      const seen: number[] = [];
      for (let i = 0; i < 20; i++) {
        seen.push(probe());
        await new Promise((r) => requestAnimationFrame(() => r(null)));
      }
      return seen;
    });

    const distinct = [...new Set(lefts.filter((n) => n >= 0))];
    expect(
      distinct.length,
      `the control moved across frames (positions seen: ${distinct.join(', ')}) — ` +
        'the panel is still tweening despite a reduced-motion preference',
    ).toBe(1);
  });
});
