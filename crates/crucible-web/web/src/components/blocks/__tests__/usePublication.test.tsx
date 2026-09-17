import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import { usePublication } from '../usePublication';
import { pluginEvents } from '@/lib/query/sse';
import { installPluginEventRoute } from '@/lib/query/routes/plugins';
import { FakeEventSource, installFakeEventSource } from '@/test-utils/sse';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { PLUGIN_CALLER_HEADER } from '@/lib/api';

/**
 * The plugin blocks, on the stream every other reader is on.
 *
 * `usePublication` held its own `EventSource` and its own refcount, so a
 * document with four blocks in it opened one stream and the rest of the app
 * opened a second one for the same URL. Here the blocks subscribe to
 * `pluginEvents()`, the shared root of `lib/query/sse.ts`, and this spec holds
 * that: one source for the blocks and every other reader together.
 *
 * The block no longer re-reads for itself either. Its value is a cache entry,
 * and the stream's route invalidates the pair the frame names, so "re-reads the
 * block the event names" is now a statement about that route reaching the
 * entry this block mounted.
 */

// No `vi.mock('@/lib/api')`: the cache entry's fetch runs the real
// `getPluginPublications` against the `GET /api/plugins/publications` route
// below, so the narrowed KEY and the plugin CALLER are read off the wire.
/** Every read the stream's cache issued, as it went out. */
const asked: { key: string; plugin: string }[] = [];
/** What the publications route answers for the pair it was asked for. */
let reply: (key: string, plugin: string) => Record<string, Record<string, unknown>> = () => ({});

let env: TestQueryEnv;

/** One block, drawing one plugin's value for one key. */
function Block(props: { plugin: string; publicationKey: string }) {
  const value = usePublication<string>(props.plugin, props.publicationKey);
  return <span data-testid={`${props.plugin}-${props.publicationKey}`}>{value() ?? ''}</span>;
}

/** What `GET /api/plugins/publications` answers for one plugin and key. */
function published(key: string, plugin: string, value: string) {
  return { [key]: { [plugin]: value } };
}

beforeEach(() => {
  installFakeEventSource();
  // In the app `src/index.tsx` names the route once, at start.
  installPluginEventRoute();
  asked.length = 0;
  reply = () => ({});
  // A fresh cache per case: the entries of one case would otherwise answer the
  // next one, and the counts below are about who asked the daemon.
  env = createTestQueryEnv({
    'GET /api/plugins/publications': (request) => {
      const key = new URL(request.url).searchParams.get('key') ?? '';
      const plugin = request.headers.get(PLUGIN_CALLER_HEADER) ?? '';
      asked.push({ key, plugin });
      return { publications: reply(key, plugin) };
    },
  });
});

afterEach(() => {
  env.restore();
  vi.clearAllMocks();
});

describe('usePublication on the shared plugin stream', () => {
  it('opens one EventSource for four blocks in one document', async () => {
    render(() => (
      <>
        <Block plugin="board" publicationKey="rows" />
        <Block plugin="board" publicationKey="columns" />
        <Block plugin="clock" publicationKey="now" />
        <Block plugin="clock" publicationKey="zone" />
      </>
    ));

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    expect(FakeEventSource.instances[0]!.url).toBe('/api/plugins/events');
  });

  // The fault this file exists to find: a block that goes around the root opens
  // a second source for a stream the app is already on.
  it('shares the source with every other reader of the stream', async () => {
    const other = vi.fn();
    const stop = pluginEvents().subscribe(other);
    render(() => <Block plugin="board" publicationKey="rows" />);

    await waitFor(() => expect(asked).toHaveLength(1));
    expect(FakeEventSource.instances).toHaveLength(1);

    FakeEventSource.instances[0]!.emit('publication_changed', { plugin: 'board', key: 'rows' });
    expect(other).toHaveBeenCalledWith('board', 'rows');
    stop();
  });

  it('re-reads the block the event names, and leaves the others alone', async () => {
    reply = (key, plugin) => published(key, plugin, 'first');
    const { getByTestId } = render(() => (
      <>
        <Block plugin="board" publicationKey="rows" />
        <Block plugin="board" publicationKey="columns" />
      </>
    ));
    await waitFor(() => expect(getByTestId('board-rows').textContent).toBe('first'));
    await waitFor(() => expect(getByTestId('board-columns').textContent).toBe('first'));
    reply = (key, plugin) => published(key, plugin, 'second');
    asked.length = 0;

    FakeEventSource.instances[0]!.emit('publication_changed', { plugin: 'board', key: 'rows' });

    await waitFor(() => expect(getByTestId('board-rows').textContent).toBe('second'));
    expect(getByTestId('board-columns').textContent).toBe('first');
    expect(asked).toHaveLength(1);
  });

  it('closes the source when the last block goes', async () => {
    const view = render(() => <Block plugin="board" publicationKey="rows" />);
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    const source = FakeEventSource.instances[0]!;

    view.unmount();

    await waitFor(() => expect(source.closed).toBe(true));
  });
});
