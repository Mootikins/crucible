import { createEffect, onCleanup } from 'solid-js';
import { useMutation, type UseMutationResult } from '@tanstack/solid-query';
import { loadLayout, resetLayout, saveLayout } from '@/lib/api';
import { windowActions } from '@/stores/windowStore';
import { getQueryClient } from './client';

/**
 * The layout's server access: the reset a component offers, and the load/save
 * plumbing the shell runs at boot.
 *
 * The layout is deliberately NOT a cache entry — it is a signal the window
 * manager owns, written to the daemon's disk, loaded once at start — so there
 * are no keys and no invalidation here. The hooks exist for the import rule:
 * a component reads the query layer, never the api module. The boot functions
 * are plain functions beside them because their caller is `lib/shell-boot.ts`,
 * which runs before any component exists (the same shape `recordRecentOnce`
 * takes in `recents.ts`).
 */

/**
 * Ask the daemon to forget the stored layout.
 *
 * The caller resets the local store afterwards; this only deletes the server
 * copy, because the store write triggers the layout auto-save below, so a
 * delete after it would race it and could leave the old layout on disk to
 * come back on the next load.
 */
export function useResetLayout(): UseMutationResult<void, Error, void, unknown> {
  return useMutation(
    () => ({ mutationFn: () => resetLayout() }),
    () => getQueryClient(),
  );
}

let saveTimeout: ReturnType<typeof setTimeout> | null = null;
// The startup load runs concurrently with auto-save setup. Until it finishes,
// the store still holds the DEFAULT layout — persisting it would race a slow
// load (HTTP slower than the 500ms debounce) and overwrite the user's saved
// layout before importLayout applies it. Gate saves until the load resolves.
let startupLoaded = false;

export function setupLayoutAutoSave(): void {
  createEffect(() => {
    // Serialize INSIDE the tracking scope. SolidJS stores are fine-grained:
    // reading only the top-level keys (layout/tabGroups/edgePanels/…) does not
    // re-run the effect on nested mutations, so collapses, active-tab switches,
    // renames, and same-group reorders silently never persisted. exportLayout()
    // walks every nested node, so any mutation now re-triggers the save.
    const serialized = windowActions.exportLayout();

    // Read above (subscribe now) but don't persist pre-load — see startupLoaded.
    if (!startupLoaded) return;

    if (saveTimeout) clearTimeout(saveTimeout);
    saveTimeout = setTimeout(() => {
      saveLayout(serialized).catch((err) => {
        console.warn('Auto-save layout failed:', err);
      });
    }, 500);

    onCleanup(() => {
      if (saveTimeout) clearTimeout(saveTimeout);
    });
  });
}

// The Navigator's collapse of files/sessions/search used to be remapped here,
// outside the version chain, so it re-ran on every load. Splitting the
// Navigator back apart is `migrateV5toV6` in stores/layoutMigrations instead:
// versioned, and therefore applied exactly once per stored layout.

export async function loadLayoutOnStartup(): Promise<void> {
  try {
    const saved = await loadLayout();
    if (saved) {
      windowActions.importLayout(saved);
    }
  } catch (err) {
    console.warn('Failed to restore layout, using defaults:', err);
  } finally {
    // Load done (restored or defaulted) — later mutations are genuine edits.
    startupLoaded = true;
  }
}
