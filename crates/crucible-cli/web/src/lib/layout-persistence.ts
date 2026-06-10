import { createEffect, onCleanup } from 'solid-js';
import { windowStore, windowActions } from '@/stores/windowStore';
import { saveLayout, loadLayout } from './api';

let saveTimeout: ReturnType<typeof setTimeout> | null = null;

export function setupLayoutAutoSave(): void {
  createEffect(() => {
    void windowStore.layout;
    void windowStore.tabGroups;
    void windowStore.edgePanels;
    void windowStore.floatingWindows;

    if (saveTimeout) clearTimeout(saveTimeout);
    saveTimeout = setTimeout(() => {
      const serialized = windowActions.exportLayout();
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
