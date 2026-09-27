import { QueryObserver } from '@tanstack/solid-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource, onlyEventSource, FakeEventSource } from '@/test-utils/sse';
import { systemEvents, fsEvents, surfaceEvents } from '../sse';
import { keys } from '../keys';
import { installSystemEventRoute } from '../routes/system';
import { installFsEventRoute } from '../routes/fs';
import { installSurfaceEventRoute } from '../routes/surfaces';

let env: TestQueryEnv;
beforeEach(() => {
  vi.useFakeTimers();
  installFakeEventSource();
  env = createTestQueryEnv();
  env.client.setDefaultOptions({ queries: { gcTime: Infinity } });
  installSystemEventRoute();
  installFsEventRoute();
  installSurfaceEventRoute();
});
afterEach(() => {
  env.restore();
  vi.useRealTimers();
});

describe('stream recovery', () => {
  const cases = [
    { name: 'system', stream: systemEvents, keys: [
      keys.proposals(), keys.proposal('p'), keys.diffset('proposal-p'),
      keys.pluginPublications('board', 'rows'),
    ] },
    { name: 'filesystem', stream: fsEvents, keys: [
      keys.fsFile('/note.md'), keys.fsDir('/'), keys.notesList('kiln'),
    ] },
    { name: 'surface', stream: surfaceEvents, keys: [keys.surfaces()] },
  ];
  it.each(cases)('$name reconciles first open, gaps and reopen without a subsequent change', async row => {
    row.stream().subscribe(() => {});
    const seed = () => {
      for (const key of row.keys) env.client.setQueryData(key, []);
      env.client.setQueryData(keys.providers(), []);
    };
    const assertRefreshed = () => {
      for (const key of row.keys) expect(env.client.getQueryState(key)?.isInvalidated, key.join('/')).toBe(true);
      expect(env.client.getQueryState(keys.providers())?.isInvalidated).toBe(false);
    };
    seed();
    onlyEventSource().open();
    await vi.waitFor(assertRefreshed);

    seed();
    onlyEventSource().emit('stream_gap', { dropped: 4 });
    await vi.waitFor(assertRefreshed);

    seed();
    const source = onlyEventSource();
    source.onerror?.(new Event('error'));
    await vi.advanceTimersByTimeAsync(1000);
    FakeEventSource.instances.at(-1)!.open();
    await vi.waitFor(assertRefreshed);
  });
  it.each(cases)('$name replaces an initial snapshot that was in flight before a gap', async row => {
    let release!: (value: string[]) => void;
    const old = new Promise<string[]>(resolve => { release = resolve; });
    let reads = 0;
    const observer = new QueryObserver(env.client, {
      queryKey: row.keys[0],
      queryFn: () => ++reads === 1 ? old : Promise.resolve(['current']),
    });
    const stop = observer.subscribe(() => {});
    row.stream().subscribe(() => {});
    expect(reads).toBe(1);
    onlyEventSource().emit('stream_gap', { dropped: 1 });
    release(['before gap']);
    try {
      await vi.waitFor(() => expect(observer.getCurrentResult().data).toEqual(['current']));
      expect(reads).toBe(2);
    } finally {
      stop();
    }
  });

  it.each(cases)('$name makes a new reader wait through disconnection', async row => {
    row.stream().subscribe(() => {});
    const original = onlyEventSource();
    original.open();
    original.onerror?.(new Event('error'));
    const opened = vi.fn();
    row.stream().subscribe(() => {}, opened);
    expect(opened).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1000);
    FakeEventSource.instances.at(-1)!.open();
    expect(opened).toHaveBeenCalledTimes(1);
  });

  it.each(cases)('$name opens a new source after an error answer, with a growing backoff', async row => {
    // A 5xx or a wrong content type fires `error` and leaves the browser's
    // source CLOSED for good: only a new source brings the stream back.
    const opened = vi.fn();
    row.stream().subscribe(() => {}, opened);
    const refused = onlyEventSource();
    refused.onerror?.(new Event('error'));
    expect(refused.closed).toBe(true);
    await vi.advanceTimersByTimeAsync(999);
    expect(FakeEventSource.instances).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(FakeEventSource.instances).toHaveLength(2);

    FakeEventSource.instances[1]!.onerror?.(new Event('error'));
    await vi.advanceTimersByTimeAsync(1999);
    expect(FakeEventSource.instances).toHaveLength(2);
    await vi.advanceTimersByTimeAsync(1);
    expect(FakeEventSource.instances).toHaveLength(3);

    FakeEventSource.instances[2]!.open();
    expect(opened).toHaveBeenCalledTimes(1);
  });

  it.each(cases)('$name reconciliation leaves a first fetchQuery in flight to finish', async row => {
    // A cancel of a query with no data fails each `fetchQuery` that waits on
    // it; a folder then does not expand at app load.
    let release!: (value: string[]) => void;
    const first = new Promise<string[]>(resolve => { release = resolve; });
    let reads = 0;
    const fetched = env.client.fetchQuery({
      queryKey: row.keys[0],
      queryFn: () => ++reads === 1 ? first : Promise.resolve(['current']),
    });
    row.stream().subscribe(() => {});
    onlyEventSource().open();
    onlyEventSource().emit('stream_gap', { dropped: 1 });
    release(['first']);
    await expect(fetched).resolves.toEqual(['first']);
    await vi.waitFor(() => expect(env.client.getQueryState(row.keys[0])?.isInvalidated).toBe(true));
  });

  it('system reconciliation leaves branch and working-tree diffs alone', async () => {
    const branch = keys.diffset('branch:/repo:main..');
    const comments = keys.diffComments('session-s1');
    for (const key of [branch, comments, keys.diffset('proposal-p')]) env.client.setQueryData(key, []);
    systemEvents().subscribe(() => {});
    onlyEventSource().open();
    await vi.waitFor(() => expect(env.client.getQueryState(keys.diffset('proposal-p'))?.isInvalidated).toBe(true));
    expect(env.client.getQueryState(branch)?.isInvalidated).toBe(false);
    expect(env.client.getQueryState(comments)?.isInvalidated).toBe(false);
  });

  it.each(cases)('$name stops announcing open after a protocol refusal', row => {
    row.stream().subscribe(() => {});
    const source = onlyEventSource();
    source.open();
    source.emit('stream_version', { version: 999 });
    expect(source.closed).toBe(true);
    const opened = vi.fn();
    row.stream().subscribe(() => {}, opened);
    expect(opened).not.toHaveBeenCalled();
  });

});
