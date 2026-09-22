import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import type { QueryKey } from '@tanstack/solid-query';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { onlyEventSource, installFakeEventSource } from '@/test-utils/sse';
import { keys } from '../../keys';
import { systemEvents } from '../../sse';
import { installSystemEventRoute } from '../system';

let env: TestQueryEnv;
let invalidated: QueryKey[];
let stop: (() => void) | null = null;

/** Opens the system stream and answers the source the route reads. */
function openStream() {
  stop = systemEvents().subscribe(() => {});
  return onlyEventSource();
}

beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv();
  installSystemEventRoute();
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

describe('the system event route', () => {
  // The frame names the proposal and carries no value. The proposal, its
  // diffset and the Inbox list each read it again.
  it('refetches the proposal, its diffset and the list on proposal_changed', () => {
    const source = openStream();

    source.emit('proposal_changed', { id: 'p-1' });

    expect(invalidated).toEqual([
      keys.proposal('p-1'),
      keys.proposals(),
      keys.diffset('proposal-p-1'),
    ]);
  });

  // The plugin stream and its route own the publication write.
  it('writes nothing for a publication', () => {
    const source = openStream();

    source.emit('publication_changed', { plugin: 'board', key: 'rows' });

    expect(invalidated).toEqual([]);
  });
});
