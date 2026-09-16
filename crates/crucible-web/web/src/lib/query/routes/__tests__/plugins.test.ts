import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import type { QueryKey } from '@tanstack/solid-query';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { onlyEventSource, installFakeEventSource } from '@/test-utils/sse';
import { keys } from '../../keys';
import { pluginEvents } from '../../sse';
import { installPluginEventRoute } from '../plugins';

let env: TestQueryEnv;
let invalidated: QueryKey[];
let stop: (() => void) | null = null;

/**
 * Opens the plugin stream and answers the source the route reads.
 *
 * The route runs inside the stream, not beside it, so every case drives it the
 * way the daemon does: one frame on the wire.
 */
function openStream() {
  stop = pluginEvents().subscribe(() => {});
  return onlyEventSource();
}

beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv();
  installPluginEventRoute();
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

describe('the plugin event route', () => {
  // The frame names the plugin and the key and carries no value, so the only
  // correct answer is to ask for that one publication again.
  it('invalidates the publication the event names', () => {
    const source = openStream();

    source.emit('publication_changed', { plugin: 'board', key: 'rows' });

    expect(invalidated).toEqual([keys.pluginPublications('board', 'rows')]);
  });

  // **The negative.** A block reading another key of the same plugin holds a
  // value the event said nothing about, and it must not be thrown away.
  it('names the key as well as the plugin, so a sibling key is left alone', () => {
    const source = openStream();

    source.emit('publication_changed', { plugin: 'board', key: 'rows' });

    expect(invalidated).toEqual([['plugins', 'publications', 'board', 'rows']]);
    expect(invalidated).not.toContainEqual(keys.pluginPublications('board'));
  });

  it('writes nothing for a frame that names no plugin or no key', () => {
    const source = openStream();

    source.emit('publication_changed', { plugin: 'board' });
    source.emit('publication_changed', { key: 'rows' });

    expect(invalidated).toEqual([]);
  });
});
