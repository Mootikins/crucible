import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import type { QueryKey } from '@tanstack/solid-query';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { onlyEventSource, installFakeEventSource } from '@/test-utils/sse';
import type { SurfaceChangedEvent } from '@/lib/api';
import type { Surface } from '@/lib/types';
import { keys } from '../../keys';
import { surfaceEvents } from '../../sse';
import { installSurfaceEventRoute } from '../surfaces';

let env: TestQueryEnv;
let invalidated: QueryKey[];
let stop: (() => void) | null = null;

/**
 * Opens the surface stream and answers the source the route reads.
 *
 * The route runs inside the stream, not beside it, so every case drives it the
 * way the daemon does: one frame on the wire.
 */
function openStream() {
  stop = surfaceEvents().subscribe(() => {});
  return onlyEventSource();
}

beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv();
  installSurfaceEventRoute();
  invalidated = [];
  vi.spyOn(env.client, 'invalidateQueries').mockImplementation((filters) => {
    invalidated.push((filters?.queryKey ?? []) as QueryKey);
    return Promise.resolve();
  });
});

afterEach(() => {
  stop?.();
  stop = null;
  vi.restoreAllMocks();
  env.restore();
});

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

describe('the surface event route', () => {
  it('invalidates the surface list when a surface changes', () => {
    const source = openStream();

    source.emit('surface_changed', changed());

    expect(invalidated).toEqual([keys.surfaces()]);
  });

  // The event carries a version and no rows, so a change can only be answered
  // by asking. A withdrawal is the exception: there is nothing left to ask for.
  it('drops a withdrawn surface from the cached list without asking again', () => {
    env.client.setQueryData(keys.surfaces(), [surface(), surface({ name: 'reviews' })]);
    const source = openStream();

    source.emit('surface_changed', changed({ withdrawn: true }));

    const held = env.client.getQueryData<Surface[]>(keys.surfaces());
    expect(held?.map((s) => s.name)).toEqual(['reviews']);
    expect(invalidated).toEqual([]);
  });

  // **The negative.** Two plugins may declare one name. A withdrawal names one
  // of them, and the other one's panel must survive it.
  it('leaves another plugin surface of the same name alone', () => {
    env.client.setQueryData(keys.surfaces(), [surface(), surface({ plugin: 'q' })]);
    const source = openStream();

    source.emit('surface_changed', changed({ withdrawn: true }));

    const held = env.client.getQueryData<Surface[]>(keys.surfaces());
    expect(held?.map((s) => s.plugin)).toEqual(['q']);
  });

  it('mints no list when nothing read the surfaces yet', () => {
    const source = openStream();

    source.emit('surface_changed', changed({ withdrawn: true }));

    expect(env.client.getQueryData(keys.surfaces())).toBeUndefined();
    expect(invalidated).toEqual([]);
  });
});
