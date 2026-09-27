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

/** Lets a batched refresh run. It waits on each fetch in flight, a few microtasks. */
const settle = () => new Promise(resolve => setTimeout(resolve, 0));

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
  it('refetches the proposal, its diffset and the list on proposal_changed', async () => {
    const source = openStream();

    source.emit('proposal_changed', { id: 'p-1' });
    await settle();

    expect(invalidated).toEqual([
      keys.proposals(),
      keys.proposal('p-1'),
      keys.diffset('proposal-p-1'),
    ]);
  });

  // One system route owns publications and proposals.
  it('invalidates the named publication', () => {
    const source = openStream();

    source.emit('publication_changed', { plugin: 'board', key: 'rows' });

    expect(invalidated).toEqual([keys.pluginPublications('board', 'rows')]);
  });
  it('coalesces a burst without dropping changes after reconciliation starts', async () => {
    const source = openStream();
    source.emit('proposal_changed', { id: 'p-1' });
    source.emit('proposal_changed', { id: 'p-1' });
    source.emit('proposal_changed', { id: 'p-2' });
    await settle();
    expect(invalidated.filter(key => key[0] === 'proposals')).toHaveLength(1);
    expect(invalidated.filter(key => key[0] === 'proposal')).toEqual([
      keys.proposal('p-1'), keys.proposal('p-2'),
    ]);

    invalidated.length = 0;
    source.emit('proposal_changed', { id: 'p-1' });
    await settle();
    expect(invalidated).toContainEqual(keys.proposal('p-1'));
  });

});
