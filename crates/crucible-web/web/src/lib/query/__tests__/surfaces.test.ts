import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource, onlyEventSource } from '@/test-utils/sse';
import type { SurfaceChangedEvent } from '@/lib/api';
import type { Surface } from '@/lib/types';
import { surfaceEvents } from '../sse';
import { installSurfaceEventRoute } from '../routes/surfaces';
import { useSurfaces } from '../surfaces';

/** One declared surface, as `GET /api/surfaces` answers it. */
function surface(over: Partial<Surface> = {}): Surface {
  return {
    plugin: 'p',
    name: 'sessions',
    title: 'Sessions',
    shape: 'list',
    session: null,
    version: 1,
    rows: [],
    ...over,
  };
}

/** One `surface_changed` frame, as the Rust route serialises it. */
function changed(over: Partial<SurfaceChangedEvent> = {}): SurfaceChangedEvent {
  return { plugin: 'p', name: 'sessions', version: 2, ...over };
}

let env: TestQueryEnv;
let dispose: (() => void) | null = null;
let stop: (() => void) | null = null;

beforeEach(() => {
  installFakeEventSource();
});

afterEach(() => {
  stop?.();
  stop = null;
  dispose?.();
  dispose = null;
  env?.restore();
});

/** Runs the body under one Solid owner, which the test disposes afterwards. */
function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

describe('useSurfaces', () => {
  it('fetches once for two panels under one root', async () => {
    env = createTestQueryEnv({ 'GET /api/surfaces': () => ({ surfaces: [surface()] }) });

    const both = inRoot(() => ({ first: useSurfaces(), second: useSurfaces() }));

    await vi.waitFor(() => expect(both.first.data).toEqual([surface()]));
    expect(both.second.data).toEqual([surface()]);
    expect(env.fetch.calls('GET /api/surfaces')).toBe(1);
  });

  it('surfaces a refusal as an error rather than as an empty roster', async () => {
    env = createTestQueryEnv({
      'GET /api/surfaces': apiError(500, 'the plugin host is down'),
    });

    const query = inRoot(() => useSurfaces());

    await vi.waitFor(() => expect(query.isError).toBe(true));
    expect(query.error?.message).toContain('Failed to list plugin surfaces');
    expect(query.data).toBeUndefined();
  });
});

// The panel no longer refetches for itself. The stream's route owns the cache
// write, and these cases prove a reader mounted on the key sees it.
describe('a mounted reader and the surface stream', () => {
  it('asks again when a surface changes, because the frame withholds the rows', async () => {
    env = createTestQueryEnv({
      'GET /api/surfaces': () => ({ surfaces: [surface({ version: 2 })] }),
    });
    installSurfaceEventRoute();

    const query = inRoot(() => useSurfaces());
    await vi.waitFor(() => expect(query.data).toHaveLength(1));
    stop = surfaceEvents().subscribe(() => {});

    onlyEventSource().emit('surface_changed', changed());

    await vi.waitFor(() => expect(env.fetch.calls('GET /api/surfaces')).toBe(2));
  });

  it('drops a withdrawn surface from the mounted reader without a second GET', async () => {
    env = createTestQueryEnv({
      'GET /api/surfaces': () => ({ surfaces: [surface(), surface({ name: 'reviews' })] }),
    });
    installSurfaceEventRoute();

    const query = inRoot(() => useSurfaces());
    await vi.waitFor(() => expect(query.data).toHaveLength(2));
    stop = surfaceEvents().subscribe(() => {});

    onlyEventSource().emit('surface_changed', changed({ withdrawn: true }));

    await vi.waitFor(() => expect(query.data?.map((s) => s.name)).toEqual(['reviews']));
    // The frame said the surface is gone, so asking would spend a round trip
    // to be told what the event already said.
    expect(env.fetch.calls('GET /api/surfaces')).toBe(1);
  });

  // **The negative.** Two plugins may declare one name, and a withdrawal names
  // one of them. The other plugin's panel must survive it.
  it('keeps another plugin surface of the same name on screen', async () => {
    env = createTestQueryEnv({
      'GET /api/surfaces': () => ({ surfaces: [surface(), surface({ plugin: 'q' })] }),
    });
    installSurfaceEventRoute();

    const query = inRoot(() => useSurfaces());
    await vi.waitFor(() => expect(query.data).toHaveLength(2));
    stop = surfaceEvents().subscribe(() => {});

    onlyEventSource().emit('surface_changed', changed({ withdrawn: true }));

    await vi.waitFor(() => expect(query.data?.map((s) => s.plugin)).toEqual(['q']));
  });
});
