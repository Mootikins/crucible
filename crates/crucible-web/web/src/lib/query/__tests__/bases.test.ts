import { QueryObserver } from '@tanstack/solid-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource, onlyEventSource } from '@/test-utils/sse';
import { keys } from '../keys';
import { fsEvents } from '../sse';
import { installFsEventRoute } from '../routes/fs';
import type { BaseRequest } from '../bases';

let env: TestQueryEnv;
beforeEach(() => {
  vi.useFakeTimers();
  installFakeEventSource();
  env = createTestQueryEnv();
  env.client.setDefaultOptions({ queries: { gcTime: Infinity } });
  installFsEventRoute();
});
afterEach(() => {
  env.restore();
  vi.useRealTimers();
});

/** An open base of the kiln `kiln`, and the number of its reads. */
function openBase(kiln: string) {
  const request: BaseRequest = { kiln, source: { path: 'Tasks.base' } };
  let reads = 0;
  const observer = new QueryObserver(env.client, {
    queryKey: keys.baseQuery(request),
    queryFn: () => { reads++; return Promise.resolve({ reads }); },
  });
  const stop = observer.subscribe(() => {});
  return { reads: () => reads, stop };
}

describe('base queries and file events', () => {
  it('refresh only the bases of the changed kiln, once per burst', async () => {
    env.client.setQueryData(keys.kilns(), [
      { name: 'Work', path: '/work' },
      { name: 'Home', path: '/home' },
    ]);
    const work = openBase('Work');
    const home = openBase('Home');
    await vi.advanceTimersByTimeAsync(0);
    expect(work.reads()).toBe(1);
    expect(home.reads()).toBe(1);

    fsEvents().subscribe(() => {});
    const source = onlyEventSource();
    for (const name of ['a', 'b', 'c']) source.emit('fs_changed', { topic: 'system', type: 'changed', kind: 'modify', path: `/work/tickets/${name}.md` });
    source.emit('fs_changed', { topic: 'system', type: 'changed', kind: 'modify', path: '/elsewhere/x.md' });
    await vi.advanceTimersByTimeAsync(500);

    expect(work.reads()).toBe(2);
    expect(home.reads()).toBe(1);
    work.stop();
    home.stop();
  });

  it('refresh every base when the kiln roster is not known yet', async () => {
    const work = openBase('Work');
    await vi.advanceTimersByTimeAsync(0);
    fsEvents().subscribe(() => {});
    onlyEventSource().emit('fs_deleted', { topic: 'system', type: 'deleted', path: '/somewhere/a.md' });
    await vi.advanceTimersByTimeAsync(500);
    expect(work.reads()).toBe(2);
    work.stop();
  });
});
