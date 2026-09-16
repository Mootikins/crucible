import { expect, type Page } from '@playwright/test';

/**
 * Shell navigation helpers.
 *
 * Two shell changes broke the old inline selectors across the suite, so the
 * knowledge lives here instead of in 20 specs:
 *
 *  - Sessions and the file tree are separate panels on opposite rails. The
 *    left panel opens on Sessions, so `session-item-*` rows exist from the
 *    start — no scope to switch, unlike the Navigator this replaced.
 *  - An empty pane holds only its own affordance. The session composer is no
 *    longer an empty-center splash; it is the content of a New Session tab,
 *    opened from the ribbon.
 */

const READY_TIMEOUT = 15000;

/**
 * Resolves once the app shell has painted (ribbon is up).
 *
 * Gated on the rail's own TOGGLE, which is the one element every ribbon
 * renders unconditionally and at every position. It used to wait on the
 * ribbon's new-session button — an incidental choice that made every spec in
 * the suite depend on one optional command button, and broke all of them the
 * day that button was retired.
 */
export async function appReady(page: Page): Promise<void> {
  await expect(page.getByTestId('ribbon-toggle-left')).toBeVisible({
    timeout: READY_TIMEOUT,
  });
}

/**
 * Wait for the sessions rail so `session-item-*` rows exist.
 *
 * The left panel opens on Sessions, so this only waits. It stays a helper
 * because a spec that reorders or closes tabs can leave another one active,
 * and because the callers read better for saying what they need.
 */
export async function openSessionsList(page: Page): Promise<void> {
  const tab = page.getByTestId('edge-tab-left-sessions-tab');
  await expect(tab).toBeVisible({ timeout: READY_TIMEOUT });
  // Probe the tree itself, not a button inside it: New Session moved onto the
  // project rows, so no single control marks the rail as open any more.
  //
  // ATTACHED on BOTH sides, not visible. With no projects and no sessions the
  // tree renders no rows, so it has no box and Playwright calls it hidden — a
  // visibility predicate would then click the tab on an ALREADY-OPEN rail and
  // close the thing this helper was asked to open.
  const tree = page.getByTestId('session-list');
  if (!(await tree.isVisible()) && !(await tree.count())) {
    await tab.click();
  }
  await expect(tree).toBeAttached({ timeout: READY_TIMEOUT });
}

/**
 * Start a session from the sessions rail.
 *
 * New Session lives on the PROJECT row — a session belongs to exactly one
 * project, so the row that names the project is what starts work in it. Pass
 * `projectPath` to pick one; the default takes the first group.
 *
 * The PATH, not the name: the tree folds worktrees into one group by basename,
 * so two checkouts of one repo share a display name and only the path is
 * unique.
 *
 * Hovers before clicking: the button is `opacity-0` at rest, and Playwright
 * counts a transparent element as visible, so a bare click would pass while
 * the affordance was unreachable by a human.
 */
export async function startNewSession(page: Page, projectPath?: string): Promise<void> {
  await openSessionsList(page);
  const button = projectPath
    ? page.getByTestId(`session-group-new-${projectPath}`)
    : page.locator('[data-testid^="session-group-new-"]').first();
  await expect(button).toBeAttached({ timeout: READY_TIMEOUT });
  await button.hover();
  await button.click();
}

/** Click a session row in the sessions rail. */
export async function openSession(page: Page, sessionId: string): Promise<void> {
  await openSessionsList(page);
  const row = page.getByTestId(`session-item-${sessionId}`);
  await expect(row).toBeVisible({ timeout: READY_TIMEOUT });
  await row.click();
}

/**
 * Open a New Session tab and wait for its composer.
 *
 * Ctrl+Shift+N, which is the project-agnostic doorway the app actually ships:
 * the palette lists it, and the ribbon's plus that used to be a third route to
 * the same event is gone.
 */
export async function openNewSessionTab(page: Page): Promise<void> {
  await appReady(page);
  await page.keyboard.press('Control+Shift+N');
  await expect(page.getByTestId('composer-input')).toBeVisible({
    timeout: READY_TIMEOUT,
  });
}
