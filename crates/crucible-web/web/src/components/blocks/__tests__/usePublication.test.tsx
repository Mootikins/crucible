import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';

/**
 * The plugin blocks, on the stream every other reader is on.
 *
 * `usePublication` held its own `EventSource` and its own refcount, so a
 * document with four blocks in it opened one stream and the rest of the app
 * opened a second one for the same URL. Here the blocks subscribe to
 * `pluginEvents()`, the shared root of `lib/query/sse.ts`, and this spec holds
 * that: one source for the blocks and every other reader together.
 */

const getPluginPublications = vi.fn(
  async (_key: string, _plugin: string) => ({}) as Record<string, Record<string, unknown>>,
);

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getPluginPublications: (key: string, plugin: string) => getPluginPublications(key, plugin),
}));

const { usePublication } = await import('../usePublication');
const { pluginEvents, resetSseForTests } = await import('@/lib/query/sse');
const { FakeEventSource, installFakeEventSource } = await import('@/test-utils/sse');

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
  getPluginPublications.mockResolvedValue({});
});

afterEach(() => {
  resetSseForTests();
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

    await waitFor(() => expect(getPluginPublications).toHaveBeenCalled());
    expect(FakeEventSource.instances).toHaveLength(1);

    FakeEventSource.instances[0]!.emit('publication_changed', { plugin: 'board', key: 'rows' });
    expect(other).toHaveBeenCalledWith('board', 'rows');
    stop();
  });

  it('re-reads the block the event names, and leaves the others alone', async () => {
    getPluginPublications.mockImplementation(async (key, plugin) =>
      published(key, plugin, 'first'),
    );
    const { getByTestId } = render(() => (
      <>
        <Block plugin="board" publicationKey="rows" />
        <Block plugin="board" publicationKey="columns" />
      </>
    ));
    await waitFor(() => expect(getByTestId('board-rows').textContent).toBe('first'));
    await waitFor(() => expect(getByTestId('board-columns').textContent).toBe('first'));
    getPluginPublications.mockImplementation(async (key, plugin) =>
      published(key, plugin, 'second'),
    );
    getPluginPublications.mockClear();

    FakeEventSource.instances[0]!.emit('publication_changed', { plugin: 'board', key: 'rows' });

    await waitFor(() => expect(getByTestId('board-rows').textContent).toBe('second'));
    expect(getByTestId('board-columns').textContent).toBe('first');
    expect(getPluginPublications).toHaveBeenCalledTimes(1);
  });

  it('closes the source when the last block goes', async () => {
    const view = render(() => <Block plugin="board" publicationKey="rows" />);
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    const source = FakeEventSource.instances[0]!;

    view.unmount();

    await waitFor(() => expect(source.closed).toBe(true));
  });
});
