import { describe, it, expect, vi } from 'vitest';
import { createRoot, createEffect } from 'solid-js';
import { createMockFetch, type MockFetch } from '@/test-utils/mock-fetch';
import { windowActions, windowStore } from '@/stores/windowStore';
import { isEdgeCollapsed } from '@/types/windowTypes';
import { setupLayoutAutoSave, loadLayoutOnStartup } from '../layout';

// No `vi.mock('@/lib/api')`. The boot functions call the real module, so the
// daemon's two layout routes answer, and a case counts what reached the wire
// rather than a module double — which is also the only way to see that the
// POST the debounce owed was actually sent.

const SAVE = 'POST /api/layout';
const LOAD = 'GET /api/layout';

/** The daemon's answer for a user who never stored a layout. */
const noLayout = (): Response =>
  new Response(JSON.stringify({ error: { code: 404, message: 'no stored layout' } }), {
    status: 404,
    headers: { 'Content-Type': 'application/json' },
  });

let fetchMock: MockFetch;
/** Releases the startup load, which starts held: it is the slow thing. */
let answerLoad!: (reply: Response) => void;
const realFetch = global.fetch;

beforeEach(() => {
  fetchMock = createMockFetch({
    [SAVE]: () => ({ ok: true }),
    [LOAD]: () => new Promise<Response>((resolve) => (answerLoad = resolve)),
  });
  global.fetch = fetchMock;
});

afterEach(() => {
  global.fetch = realFetch;
});

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

    let dispose!: () => void;
    createRoot((d) => {
      dispose = d;
      setupLayoutAutoSave();
    });
    const loading = loadLayoutOnStartup();

    // Debounce elapses while the load is still in flight.
    await vi.advanceTimersByTimeAsync(600);
    expect(fetchMock.calls(SAVE)).toBe(0);

    // Load resolves (no saved layout → defaults kept). Gate opens.
    answerLoad(noLayout());
    await loading;

    // A genuine post-load edit now persists on the next debounce. Toggle
    // relative to the current value so it always changes (the store singleton
    // may be left collapsed by an earlier test, and Solid won't notify on a
    // no-op set).
    windowActions.setEdgePanelCollapsed(
      'left',
      !isEdgeCollapsed(windowStore.edgePanels.left),
    );
    await vi.advanceTimersByTimeAsync(600);
    expect(fetchMock.calls(SAVE)).toBe(1);

    dispose();
    vi.useRealTimers();
  });
});
