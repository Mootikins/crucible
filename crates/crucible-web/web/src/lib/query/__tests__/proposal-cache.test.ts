import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource, onlyEventSource } from '@/test-utils/sse';
import { invalidateProposal } from '../proposal-cache';
import { useProposalDecision } from '../proposals';
import { systemEvents } from '../sse';
import { installSystemEventRoute } from '../routes/system';
import { keys } from '../keys';

let env: TestQueryEnv;
let dispose: () => void;

beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv({
    'POST /api/proposals/original/accept': () => ({ id: 'split' }),
  });
  installSystemEventRoute();
});
afterEach(() => {
  dispose?.();
  vi.restoreAllMocks();
  env.restore();
});

describe('proposal reconciliation', () => {
  it.each(['before', 'after', 'absent'])('refreshes both split ids with the event %s the reply', async order => {
    const mutation = createRoot(stop => {
      dispose = stop;
      return useProposalDecision(() => 'original');
    });
    systemEvents().subscribe(() => {});
    const invalidate = vi.spyOn(env.client, 'invalidateQueries').mockResolvedValue();
    const emit = () => onlyEventSource().emit('proposal_changed', { id: 'split' });
    if (order === 'before') emit();
    await mutation.mutateAsync({ kind: 'accept', paths: ['note.md'] });
    if (order === 'after') emit();
    await Promise.resolve();

    for (const id of ['original', 'split']) {
      expect(invalidate).toHaveBeenCalledWith({ queryKey: keys.proposal(id) });
      expect(invalidate).toHaveBeenCalledWith({ queryKey: keys.diffset('proposal-' + id) });
    }
    expect(invalidate).toHaveBeenCalledWith({ queryKey: keys.proposals() });
  });

  it('shares a queued event refresh and waits for its reconciliation', async () => {
    let finish!: () => void;
    const fetching = new Promise<void>(resolve => { finish = resolve; });
    const invalidate = vi.spyOn(env.client, 'invalidateQueries').mockReturnValue(fetching);
    systemEvents().subscribe(() => {});
    onlyEventSource().emit('proposal_changed', { id: 'p' });
    let done = false;
    const settled = invalidateProposal(env.client, 'p').then(() => { done = true; });
    await Promise.resolve();
    expect(invalidate).toHaveBeenCalledTimes(3);
    expect(done).toBe(false);
    finish();
    await settled;
    expect(done).toBe(true);
  });

  it('does not coalesce a new change into an already running fetch', async () => {
    let finish!: () => void;
    const fetching = new Promise<void>(resolve => { finish = resolve; });
    const invalidate = vi.spyOn(env.client, 'invalidateQueries').mockReturnValue(fetching);
    const first = invalidateProposal(env.client, 'p');
    await Promise.resolve();
    const second = invalidateProposal(env.client, 'p');
    await Promise.resolve();
    expect(invalidate).toHaveBeenCalledTimes(6);
    finish();
    await Promise.all([first, second]);
  });
});
