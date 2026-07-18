import { createEffect, onCleanup } from 'solid-js';
import { windowActions } from '@/stores/windowStore';
import { saveLayout, loadLayout } from './api';

let saveTimeout: ReturnType<typeof setTimeout> | null = null;

export function setupLayoutAutoSave(): void {
  createEffect(() => {
    // Serialize INSIDE the tracking scope. SolidJS stores are fine-grained:
    // reading only the top-level keys (layout/tabGroups/edgePanels/…) does not
    // re-run the effect on nested mutations, so collapses, active-tab switches,
    // renames, and same-group reorders silently never persisted. exportLayout()
    // walks every nested node, so any mutation now re-triggers the save.
    const serialized = windowActions.exportLayout();

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

export async function loadLayoutOnStartup(): Promise<void> {
  try {
    const saved = await loadLayout();
    if (saved) {
      windowActions.importLayout(saved);
    }
  } catch (err) {
    console.warn('Failed to restore layout, using defaults:', err);
  }
}
