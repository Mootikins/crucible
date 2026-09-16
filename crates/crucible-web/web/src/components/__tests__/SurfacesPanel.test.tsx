import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import type { Surface, SurfaceChangedEvent } from '@/lib/api';
import { resetSseForTests } from '@/lib/query/sse';
import { FakeEventSource, installFakeEventSource } from '@/test-utils/sse';

const getSurfacesMock = vi.fn();
/** Captures the SSE callback so a test can fire a change without a server. */
let surfaceListener: ((event: SurfaceChangedEvent) => void) | null = null;
const unsubscribeMock = vi.fn();

/** The capture, which every case but the shared-stream ones runs against. */
function captureListener(cb: (event: SurfaceChangedEvent) => void): () => void {
  surfaceListener = cb;
  return unsubscribeMock;
}

const subscribeToSurfaceEventsMock = vi.fn(captureListener);

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getSurfaces: (...args: unknown[]) => getSurfacesMock(...args),
  subscribeToSurfaceEvents: (cb: (event: SurfaceChangedEvent) => void) =>
    subscribeToSurfaceEventsMock(cb),
}));

// Every panel shares one root in `lib/query/sse.ts`, and a root outlives the
// test that opened it. Forget them between cases, so a source of one test
// cannot answer the next one.
afterEach(() => {
  resetSseForTests();
});

/** One `surface_changed` frame, as the Rust route serialises it. */
function changed(over: Partial<SurfaceChangedEvent> = {}): SurfaceChangedEvent {
  return { plugin: 'p', name: 'sessions', version: 2, ...over };
}

import { SurfacesPanel } from '../SurfacesPanel';

function surface(rows: Surface['rows'], over: Partial<Surface> = {}): Surface {
  return {
    plugin: 'p',
    name: 'sessions',
    title: 'Sessions',
    shape: 'list',
    session: null,
    version: 1,
    rows,
    ...over,
  };
}

describe('SurfacesPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    surfaceListener = null;
  });

  it('draws the declared rows and their detail', async () => {
    getSurfacesMock.mockResolvedValue([
      surface([
        { id: 's1', text: 'crucible', mark: 'busy' },
        { id: 's2', text: 'web-fix', detail: 'waiting', mark: 'blocked' },
      ]),
    ]);

    const { getByText } = render(() => <SurfacesPanel />);

    await waitFor(() => expect(getByText('crucible')).toBeTruthy());
    expect(getByText('web-fix')).toBeTruthy();
    expect(getByText('waiting')).toBeTruthy();
    expect(getByText('Sessions')).toBeTruthy();
  });

  // The plugin states a status; this layer picks how it looks. The TUI picks
  // differently, and that is the point of a stated vocabulary.
  it('colours a row from its declared mark', async () => {
    getSurfacesMock.mockResolvedValue([
      surface([
        { id: 'a', text: 'busy row', mark: 'busy' },
        { id: 'b', text: 'failed row', mark: 'failed' },
      ]),
    ]);

    const { container } = render(() => <SurfacesPanel />);

    await waitFor(() => expect(container.querySelector('.bg-primary')).toBeTruthy());
    expect(container.querySelector('.bg-error')).toBeTruthy();
  });

  // A mark this build has no colour for must say nothing, rather than assert
  // that something is wrong.
  it('renders an unknown mark as blank rather than a placeholder', async () => {
    getSurfacesMock.mockResolvedValue([
      surface([{ id: 'a', text: 'odd row', mark: 'sideways' }]),
    ]);

    const { container, getByText } = render(() => <SurfacesPanel />);

    await waitFor(() => expect(getByText('odd row')).toBeTruthy());
    expect(container.querySelector('.bg-transparent')).toBeTruthy();
    expect(container.textContent).not.toContain('sideways');
  });

  // The event carries a version and no rows, so the only correct response is to
  // refetch. If this regressed, a browser panel would show stale rows forever.
  it('refetches when a surface changes', async () => {
    getSurfacesMock.mockResolvedValue([surface([{ id: 'a', text: 'first' }])]);
    const { getByText } = render(() => <SurfacesPanel />);
    await waitFor(() => expect(getByText('first')).toBeTruthy());
    expect(getSurfacesMock).toHaveBeenCalledTimes(1);

    getSurfacesMock.mockResolvedValue([surface([{ id: 'b', text: 'second' }], { version: 2 })]);
    surfaceListener?.(changed());

    await waitFor(() => expect(getByText('second')).toBeTruthy());
    expect(getSurfacesMock).toHaveBeenCalledTimes(2);
  });

  // A plugin uninstall withdraws the surface. The daemon marks it on the event,
  // so the panel goes without asking: there is no content left to fetch, and a
  // refetch would spend a round trip to be told what the event already said.
  it('drops a withdrawn surface without refetching', async () => {
    getSurfacesMock.mockResolvedValue([surface([{ id: 'a', text: 'crucible' }])]);
    const { getByText, queryByText } = render(() => <SurfacesPanel />);
    await waitFor(() => expect(getByText('crucible')).toBeTruthy());
    expect(getSurfacesMock).toHaveBeenCalledTimes(1);

    surfaceListener?.(changed({ withdrawn: true }));

    await waitFor(() => expect(getByText('No plugin surfaces')).toBeTruthy());
    expect(queryByText('crucible')).toBeNull();
    expect(getSurfacesMock).toHaveBeenCalledTimes(1);
  });

  // **The negative.** One plugin goes away while the browser draws another
  // plugin's panel. That panel must keep its rows.
  it('leaves another plugin panel drawn when one is withdrawn', async () => {
    getSurfacesMock.mockResolvedValue([
      surface([{ id: 'a', text: 'crucible' }]),
      surface([{ id: 'b', text: 'review queue' }], { name: 'reviews', title: 'Reviews' }),
    ]);
    const { getByText, queryByText } = render(() => <SurfacesPanel />);
    await waitFor(() => expect(getByText('crucible')).toBeTruthy());

    surfaceListener?.(changed({ name: 'reviews', withdrawn: true }));

    await waitFor(() => expect(queryByText('Reviews')).toBeNull());
    expect(getByText('crucible')).toBeTruthy();
    expect(getSurfacesMock).toHaveBeenCalledTimes(1);
  });

  // A withdrawal must not leave the chooser pointing at a surface that is gone.
  // If it did, a plugin that later re-declares the same name would silently
  // steal the panel back from whatever the user had selected.
  it('forgets a selection that named the withdrawn surface', async () => {
    getSurfacesMock.mockResolvedValue([
      surface([{ id: 'a', text: 'crucible' }]),
      surface([{ id: 'b', text: 'review queue' }], { name: 'reviews', title: 'Reviews' }),
    ]);
    const { getByText, queryByText } = render(() => <SurfacesPanel />);
    await waitFor(() => expect(getByText('Reviews')).toBeTruthy());

    getByText('Reviews').click();
    await waitFor(() => expect(getByText('review queue')).toBeTruthy());

    surfaceListener?.(changed({ name: 'reviews', withdrawn: true }));
    await waitFor(() => expect(queryByText('review queue')).toBeNull());

    // The plugin comes back with the same name. The panel must stay where the
    // browser put it, not jump to a stale selection.
    getSurfacesMock.mockResolvedValue([
      surface([{ id: 'a', text: 'crucible' }]),
      surface([{ id: 'c', text: 'new review' }], { name: 'reviews', title: 'Reviews' }),
    ]);
    surfaceListener?.(changed({ name: 'reviews' }));

    await waitFor(() => expect(getSurfacesMock).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(getByText('crucible'), 'the panel stayed where the browser put it').toBeTruthy(),
    );
    expect(queryByText('new review'), 'a stale selection did not steal the panel').toBeNull();
  });

  it('explains itself when no plugin declares a surface', async () => {
    getSurfacesMock.mockResolvedValue([]);
    const { getByText } = render(() => <SurfacesPanel />);
    await waitFor(() => expect(getByText('No plugin surfaces')).toBeTruthy());
  });

  it('says a declared surface is empty rather than drawing nothing', async () => {
    getSurfacesMock.mockResolvedValue([surface([])]);
    const { getByText } = render(() => <SurfacesPanel />);
    await waitFor(() => expect(getByText('Nothing here yet')).toBeTruthy());
  });

  // Two plugins can each declare a panel, so the chooser has to exist — and must
  // not appear when there is only one, where the header already names it.
  it('offers a chooser only when more than one surface exists', async () => {
    getSurfacesMock.mockResolvedValue([surface([{ id: 'a', text: 'one' }])]);
    const single = render(() => <SurfacesPanel />);
    await waitFor(() => expect(single.getByText('one')).toBeTruthy());
    expect(single.container.querySelectorAll('button').length).toBe(0);
    single.unmount();

    getSurfacesMock.mockResolvedValue([
      surface([{ id: 'a', text: 'one' }]),
      surface([{ id: 'b', text: 'two' }], { name: 'reviews', title: 'Reviews' }),
    ]);
    const many = render(() => <SurfacesPanel />);
    await waitFor(() => expect(many.getByText('Reviews')).toBeTruthy());
    expect(many.container.querySelectorAll('button').length).toBe(2);
  });
});

// The stream itself, not the mock of it. `lib/query/sse.ts` owns one source for
// the surface stream and every panel subscribes to it, so the count of
// EventSources is one whatever the count of panels on screen.
describe('the shared surface stream', () => {
  beforeEach(async () => {
    const actual = await vi.importActual<typeof import('@/lib/api')>('@/lib/api');
    subscribeToSurfaceEventsMock.mockImplementation(actual.subscribeToSurfaceEvents);
    installFakeEventSource();
    getSurfacesMock.mockResolvedValue([surface([{ id: 'a', text: 'crucible' }])]);
  });

  afterEach(() => {
    subscribeToSurfaceEventsMock.mockImplementation(captureListener);
  });

  it('opens one EventSource for two panels', async () => {
    render(() => (
      <>
        <SurfacesPanel />
        <SurfacesPanel />
      </>
    ));

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
    expect(FakeEventSource.instances[0]!.url).toBe('/api/surfaces/events');
  });

  it('gives both panels the same withdrawal off that one source', async () => {
    const { getAllByText, queryByText } = render(() => (
      <>
        <SurfacesPanel />
        <SurfacesPanel />
      </>
    ));
    await waitFor(() => expect(getAllByText('crucible')).toHaveLength(2));
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));

    FakeEventSource.instances[0]!.emit('surface_changed', {
      plugin: 'p',
      name: 'sessions',
      version: 2,
      withdrawn: true,
    });

    await waitFor(() => expect(queryByText('crucible')).toBeNull());
  });
});
