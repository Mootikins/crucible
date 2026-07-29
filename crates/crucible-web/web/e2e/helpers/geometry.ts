import { expect, type Page, type Locator } from '@playwright/test';

export interface Point {
  x: number;
  y: number;
}

/**
 * Make animations and transitions instant for the rest of the page's life.
 *
 * The suite's dominant historical failure was reading element geometry while
 * the layout was still animating: a panel that has *started* expanding is
 * already "visible", so a box read then is a position the element is still
 * moving away from. Raw `page.mouse` drags — required because the app uses
 * pointer-event DnD rather than HTML5 DnD — bypass Playwright's actionability
 * checks, including its stability wait, so nothing catches it.
 *
 * Removing the animation removes the window in which that can happen, rather
 * than teaching each call site to wait it out. Registered via `addInitScript`
 * so it applies before any app script runs, on every navigation.
 *
 * This does NOT remove animation classes (`cru-anim-*`), so assertions about
 * them still hold — it only collapses their duration to zero.
 */
export async function disableAnimations(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const apply = () => {
      const style = document.createElement('style');
      style.setAttribute('data-test-no-animations', '');
      style.textContent = `*, *::before, *::after {
        animation-duration: 0s !important;
        animation-delay: 0s !important;
        transition-duration: 0s !important;
        transition-delay: 0s !important;
        scroll-behavior: auto !important;
      }`;
      document.head.appendChild(style);
    };
    // addInitScript can run before <head> exists on the very first document.
    if (document.head) apply();
    else document.addEventListener('DOMContentLoaded', apply, { once: true });
  });
}

/**
 * Centre of `locator` once its box has stopped moving.
 *
 * Playwright's own actionability checks wait for an element to be "stable"
 * (same box across two consecutive frames) before clicking it. Raw
 * `page.mouse` drags — which the pointer-event DnD library requires — bypass
 * that entirely, so a box read while a panel is still expanding is a
 * mid-animation position. `mouse.down()` then lands beside the tab, no drag
 * ever starts, and the test fails on the drop assertion with no hint that the
 * grab was the problem.
 *
 * "Visible" is not enough: it goes true the instant the panel begins opening.
 */
export async function stableCenter(locator: Locator, timeout = 3000): Promise<Point> {
  await locator.waitFor({ state: 'visible', timeout });
  let previous: { x: number; y: number; width: number; height: number } | null = null;
  await expect
    .poll(
      async () => {
        const box = await locator.boundingBox();
        if (!box) return false;
        const settled =
          previous !== null &&
          previous.x === box.x &&
          previous.y === box.y &&
          previous.width === box.width &&
          previous.height === box.height;
        previous = box;
        return settled;
      },
      { timeout },
    )
    .toBe(true);
  return { x: previous!.x + previous!.width / 2, y: previous!.y + previous!.height / 2 };
}

/** {@link stableCenter} by selector. */
export function getCenter(page: Page, selector: string): Promise<Point> {
  return stableCenter(page.locator(selector));
}

/** {@link stableCenter} by locator. */
export function getCenterOf(_page: Page, locator: Locator): Promise<Point> {
  return stableCenter(locator);
}
