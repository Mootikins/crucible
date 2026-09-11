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
