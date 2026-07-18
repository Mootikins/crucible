import { describe, it, expect, vi } from 'vitest';
import { createRoot, createEffect } from 'solid-js';

const saveLayoutMock = vi.fn().mockResolvedValue(undefined);
const loadLayoutMock = vi.fn().mockResolvedValue(null);

vi.mock('../api', () => ({
  saveLayout: (...args: unknown[]) => saveLayoutMock(...args),
  loadLayout: (...args: unknown[]) => loadLayoutMock(...args),
}));

import { windowActions } from '@/stores/windowStore';

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
