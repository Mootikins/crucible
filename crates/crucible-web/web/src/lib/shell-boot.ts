import { loadLayoutOnStartup, setupLayoutAutoSave } from '@/lib/layout-persistence';

/**
 * Start layout persistence for the shell this page chose.
 *
 * Only the desktop shell has a layout. It persists to the DAEMON through
 * `POST /api/layout`, not to this browser, so every desktop on that daemon
 * shares one layout. The compact shell must neither load it (it has no panes to
 * restore) nor save it (it would overwrite every desktop's layout with nothing).
 */
export function startLayoutPersistence(opts: { compact: boolean }): void {
  if (opts.compact) return;
  void loadLayoutOnStartup();
  setupLayoutAutoSave();
}

/**
 * Tell the stylesheet which shell is drawing.
 *
 * The compact rules used to key on `@media (max-width: 767px)`, which is LIVE
 * while `isCompact()` is decided once at load. Narrowing a desktop window past
 * that width applied the phone's settings layout to the two-column grid dialog
 * still rendering. One decision, one source of truth: the shell stamps the
 * document, and the CSS reads the stamp.
 */
export function markShell(compact: boolean, root: HTMLElement = document.documentElement): void {
  root.toggleAttribute('data-compact-shell', compact);
}
