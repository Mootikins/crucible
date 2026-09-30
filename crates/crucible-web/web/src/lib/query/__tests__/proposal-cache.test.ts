import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { QueryObserver, type QueryKey } from '@tanstack/solid-query';
import { createRoot } from 'solid-js';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource, onlyEventSource } from '@/test-utils/sse';
import { invalidateProposal } from '../proposal-cache';
import { useProposalDecision } from '../proposals';
import { systemEvents } from '../sse';
import { installSystemEventRoute } from '../routes/system';
import { keys } from '../keys';

let env: TestQueryEnv;
let dispose: (() => void) | undefined;
const stops: (() => void)[] = [];

beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv({
    'POST /api/rpc/proposal.accept': () => ({ id: 'split' }),
  });
  env.client.setDefaultOptions({ queries: { gcTime: Infinity, retry: false } });
  installSystemEventRoute();
});
afterEach(() => {
  for (const stop of stops.splice(0)) stop();
  dispose?.();
  dispose = undefined;
  env.restore();
});

/** A deferred answer: the test decides when the fetch settles. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(r => { resolve = r; });
  return { promise, resolve };
}

/**
 * A reader of `queryKey` whose fetches the test answers one by one. The
 * fetch number `n` waits on `answers[n - 1]`.
 */
function reader(queryKey: QueryKey, answers: { promise: Promise<string> }[]) {
  let reads = 0;
  const observer = new QueryObserver(env.client, {
    queryKey,
    queryFn: () => answers[reads++]?.promise ?? Promise.resolve(`read ${reads}`),
  });
  stops.push(observer.subscribe(() => {}));
  return { observer, reads: () => reads, data: () => observer.getCurrentResult().data };
}

describe('proposal reconciliation', () => {
  it.each([
    ['the Inbox list', keys.proposals()],
    ['the proposal', keys.proposal('p')],
    ['the proposal diffset', keys.diffset('proposal-p')],
  ] as const)('a change during the first load of %s reads again after that load', async (_name, key) => {
    // A plain invalidation of a query with no data joins the fetch in flight,
    // and the change that arrived after that fetch began is lost.
    const first = deferred<string>();
    const second = deferred<string>();
    const read = reader(key, [first, second]);
    expect(read.reads()).toBe(1);

    const done = invalidateProposal(env.client, 'p');
    first.resolve('before the change');
    await vi.waitFor(() => expect(read.reads()).toBe(2));
    second.resolve('after the change');
    await done;
    expect(read.data()).toBe('after the change');
  });

  it.each(['before', 'after', 'absent'])('refreshes both split ids with the event %s the reply', async order => {
    const mutation = createRoot(stop => {
      dispose = stop;
      return useProposalDecision(() => 'original');
    });
    systemEvents().subscribe(() => {});
    const targets = ['original', 'split'].flatMap(id => [keys.proposal(id), keys.diffset('proposal-' + id)]);
    for (const key of [...targets, keys.proposals()]) env.client.setQueryData(key, 'held');
    const emit = () => onlyEventSource().emit('proposal_changed', { id: 'split' });
    if (order === 'before') emit();
    await mutation.mutateAsync({ kind: 'accept', files: [{ root: '/kiln', path: 'note.md' }] });
    if (order === 'after') emit();

    await vi.waitFor(() => {
      for (const key of [...targets, keys.proposals()]) {
        expect(env.client.getQueryState(key)?.isInvalidated, key.join('/')).toBe(true);
      }
    });
    const request = await env.fetch.sent(0);
    expect(request.path).toBe('/api/rpc/proposal.accept');
    expect(request.body).toEqual({
      id: 'original',
      files: [{ root: '/kiln', path: 'note.md' }],
    });
  });

  it('shares one refresh among the changes of one microtask, and waits for its refetch', async () => {
    const answers = [deferred<string>(), deferred<string>()];
    const read = reader(keys.proposal('p'), answers);
    answers[0]!.resolve('first');
    await vi.waitFor(() => expect(read.data()).toBe('first'));

    systemEvents().subscribe(() => {});
    onlyEventSource().emit('proposal_changed', { id: 'p' });
    let done = false;
    const settled = invalidateProposal(env.client, 'p').then(() => { done = true; });
    await vi.waitFor(() => expect(read.reads()).toBe(2));
    await Promise.resolve();
    expect(done).toBe(false);

    answers[1]!.resolve('second');
    await settled;
    expect(done).toBe(true);
    expect(read.reads()).toBe(2);
    expect(read.data()).toBe('second');
  });

  it('does not fold a new change into a refresh that already runs', async () => {
    const answers = [deferred<string>(), deferred<string>(), deferred<string>()];
    const read = reader(keys.proposal('p'), answers);
    answers[0]!.resolve('first');
    await vi.waitFor(() => expect(read.data()).toBe('first'));

    const first = invalidateProposal(env.client, 'p');
    await vi.waitFor(() => expect(read.reads()).toBe(2));
    const second = invalidateProposal(env.client, 'p');
    answers[1]!.resolve('older');
    await vi.waitFor(() => expect(read.reads()).toBe(3));
    answers[2]!.resolve('newest');
    await Promise.all([first, second]);
    expect(read.data()).toBe('newest');
  });
});
