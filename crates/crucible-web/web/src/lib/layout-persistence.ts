import { createEffect, onCleanup } from 'solid-js';
import { windowActions } from '@/stores/windowStore';
import { saveLayout, loadLayout } from './api';

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

// The Navigator replaced the separate files/sessions/search left tabs. Remap
// any of those in a persisted layout to 'navigator', and drop duplicates that
// would otherwise land in the same tab group.
const RETIRED_LEFT = new Set(['files', 'sessions', 'search']);
function migrateRetiredPanels(value: unknown): unknown {
  if (Array.isArray(value)) {
    let seenNavigator = false;
    const out: unknown[] = [];
    for (const item of value) {
      const migrated = migrateRetiredPanels(item);
      if (migrated && typeof migrated === 'object' && (migrated as { contentType?: string }).contentType === 'navigator') {
        if (seenNavigator) continue;
        seenNavigator = true;
      }
      out.push(migrated);
    }
    return out;
  }
  if (value && typeof value === 'object') {
    const obj: Record<string, unknown> = { ...(value as Record<string, unknown>) };
    if (typeof obj.contentType === 'string' && RETIRED_LEFT.has(obj.contentType)) {
      obj.contentType = 'navigator';
      obj.title = 'Navigator';
    }
    for (const key of Object.keys(obj)) obj[key] = migrateRetiredPanels(obj[key]);
    return obj;
  }
  return value;
}

export async function loadLayoutOnStartup(): Promise<void> {
  try {
    const saved = await loadLayout();
    if (saved) {
      windowActions.importLayout(migrateRetiredPanels(saved) as typeof saved);
    }
  } catch (err) {
    console.warn('Failed to restore layout, using defaults:', err);
  } finally {
    // Load done (restored or defaulted) — later mutations are genuine edits.
    startupLoaded = true;
  }
}
