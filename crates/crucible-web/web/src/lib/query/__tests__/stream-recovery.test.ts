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
