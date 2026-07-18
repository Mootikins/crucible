import { describe, it, expect, vi } from 'vitest';
import { createRoot, createEffect } from 'solid-js';

const saveLayoutMock = vi.fn().mockResolvedValue(undefined);
const loadLayoutMock = vi.fn().mockResolvedValue(null);

vi.mock('../api', () => ({
  saveLayout: (...args: unknown[]) => saveLayoutMock(...args),
  loadLayout: (...args: unknown[]) => loadLayoutMock(...args),
}));

import { windowActions, windowStore } from '@/stores/windowStore';
import { setupLayoutAutoSave, loadLayoutOnStartup } from '../layout-persistence';

describe('layout auto-save tracking', () => {
  // Regression: the auto-save effect serializes via exportLayout() INSIDE its
  // tracking scope. SolidJS fine-grained stores don't re-run an effect that
  // reads only top-level keys on a NESTED mutation — so collapsing an edge
  // panel (nested) must still re-run the serializing effect, or the save is
  // silently dropped. This asserts the read pattern the fix relies on.
  it('re-runs an exportLayout() effect on a nested edge-panel mutation', async () => {
    let runs = 0;
    let dispose!: () => void;
    createRoot((d) => {
      dispose = d;
      createEffect(() => {
        // Same deep read setupLayoutAutoSave performs.
        windowActions.exportLayout();
        runs++;
      });
    });

    // Let Solid flush the effect's initial (deferred) run.
    await Promise.resolve();
    const before = runs;
    expect(before).toBeGreaterThan(0);

    // A purely nested mutation must re-run the serializing effect.
    windowActions.setEdgePanelCollapsed('left', true);
    await Promise.resolve();
    expect(runs).toBeGreaterThan(before);

    dispose();
  });
});

describe('layout auto-save startup gating', () => {
  // Regression: setupLayoutAutoSave runs concurrently with the startup load. If
  // the load is slower than the 500ms debounce, the DEFAULT layout must NOT be
  // POSTed first — that would overwrite the user's saved layout before it is
  // imported. Saves are gated until loadLayoutOnStartup resolves.
  it('does not persist the default layout before a slow startup load finishes', async () => {
    vi.useFakeTimers();
    saveLayoutMock.mockClear();

    let resolveLoad!: (v: unknown) => void;
    loadLayoutMock.mockReturnValueOnce(
      new Promise((r) => {
        resolveLoad = r;
      })
    );

    let dispose!: () => void;
    createRoot((d) => {
      dispose = d;
      setupLayoutAutoSave();
    });
    const loading = loadLayoutOnStartup();

    // Debounce elapses while the load is still in flight.
    await vi.advanceTimersByTimeAsync(600);
    expect(saveLayoutMock).not.toHaveBeenCalled();

    // Load resolves (no saved layout → defaults kept). Gate opens.
    resolveLoad(null);
    await loading;

    // A genuine post-load edit now persists on the next debounce. Toggle
    // relative to the current value so it always changes (the store singleton
    // may be left collapsed by an earlier test, and Solid won't notify on a
    // no-op set).
    windowActions.setEdgePanelCollapsed('left', !windowStore.edgePanels.left.isCollapsed);
    await vi.advanceTimersByTimeAsync(600);
    expect(saveLayoutMock).toHaveBeenCalledTimes(1);

    dispose();
    vi.useRealTimers();
  });
});
