import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import type { SurfaceChangedEvent } from '@/lib/api';
import type { Surface, SurfaceRow } from '@/lib/types';
import { installSurfaceEventRoute } from '@/lib/query/routes/surfaces';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { FakeEventSource, installFakeEventSource } from '@/test-utils/sse';
import { SurfacesPanel } from '../SurfacesPanel';

/**
 * Nothing in `@/lib/api` is stubbed here. The panel reads the roster through
 * `useSurfaces` and the daemon's frames arrive on a fake `EventSource`, so
 * every case runs the real `getSurfaces` and the real stream parser against a
 * mocked `fetch` and a hand-driven source — which is where the two used to
 * disagree.
 */

/** What `GET /api/surfaces` answers next. Re-read on every fetch. */
let served: Surface[] = [];
let env: TestQueryEnv;

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

/** One `surface_changed` frame, as the Rust route serialises it. */
function changed(over: Partial<SurfaceChangedEvent> = {}): SurfaceChangedEvent {
  return { plugin: 'p', name: 'sessions', version: 2, ...over };
}

/** The one source the panels share, once the first of them has subscribed. */
async function stream(): Promise<FakeEventSource> {
  await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));
  return FakeEventSource.instances[0]!;
}

/** How many times the panel asked the daemon for the roster. */
function fetches(): number {
  return env.fetch.calls('GET /api/surfaces');
}

beforeEach(() => {
  served = [];
  installFakeEventSource();
  env = createTestQueryEnv({ 'GET /api/surfaces': () => ({ surfaces: served }) });
  // The app installs the route at start (`src/index.tsx`); a test installs it
  // after `createTestQueryEnv` has forgotten the previous one.
  installSurfaceEventRoute();
});

afterEach(() => {
  env.restore();
});

describe('SurfacesPanel', () => {
  it('draws the declared rows and their detail', async () => {
    served = [
      surface([
        { id: 's1', text: 'crucible', mark: 'busy', detail: null },
        { id: 's2', text: 'web-fix', detail: 'waiting', mark: 'blocked' },
      ]),
    ];

    const { getByText } = render(() => <SurfacesPanel />);

    await waitFor(() => expect(getByText('crucible')).toBeTruthy());
    expect(getByText('web-fix')).toBeTruthy();
    expect(getByText('waiting')).toBeTruthy();
    expect(getByText('Sessions')).toBeTruthy();
  });

  // The plugin states a status; this layer picks how it looks. The TUI picks
  // differently, and that is the point of a stated vocabulary.
  it('colours a row from its declared mark', async () => {
    served = [
      surface([
        { id: 'a', text: 'busy row', mark: 'busy', detail: null },
        { id: 'b', text: 'failed row', mark: 'failed', detail: null },
      ]),
    ];

    const { container } = render(() => <SurfacesPanel />);

    await waitFor(() => expect(container.querySelector('.bg-primary')).toBeTruthy());
    expect(container.querySelector('.bg-error')).toBeTruthy();
  });

  // A mark this build has no colour for must say nothing, rather than assert
  // that something is wrong.
  it('renders an unknown mark as blank rather than a placeholder', async () => {
    // `SurfaceMarkRow` is a closed enum in the document, so this value can
    // only reach a browser from a daemon this build does not know. The cast
    // says that out loud; the assertion is that the panel degrades.
    served = [
      surface([
        { id: 'a', text: 'odd row', detail: null, mark: 'sideways' as SurfaceRow['mark'] },
      ]),
    ];

    const { container, getByText } = render(() => <SurfacesPanel />);

    await waitFor(() => expect(getByText('odd row')).toBeTruthy());
    expect(container.querySelector('.bg-transparent')).toBeTruthy();
    expect(container.textContent).not.toContain('sideways');
  });

  // The event carries a version and no rows, so the only correct response is to
  // ask again. If this regressed, a browser panel would show stale rows forever.
  it('refetches when a surface changes', async () => {
    served = [surface([{ id: 'a', text: 'first', detail: null, mark: null }])];
    const { getByText } = render(() => <SurfacesPanel />);
    await waitFor(() => expect(getByText('first')).toBeTruthy());
    expect(fetches()).toBe(1);

    served = [surface([{ id: 'b', text: 'second', detail: null, mark: null }], { version: 2 })];
    (await stream()).emit('surface_changed', changed());

    await waitFor(() => expect(getByText('second')).toBeTruthy());
    expect(fetches()).toBe(2);
  });

  // A plugin uninstall withdraws the surface. The daemon marks it on the event,
  // so the panel goes without asking: there is no content left to fetch, and a
  // refetch would spend a round trip to be told what the event already said.
  it('drops a withdrawn surface without refetching', async () => {
    served = [surface([{ id: 'a', text: 'crucible', detail: null, mark: null }])];
    const { getByText, queryByText } = render(() => <SurfacesPanel />);
    await waitFor(() => expect(getByText('crucible')).toBeTruthy());
    expect(fetches()).toBe(1);

    (await stream()).emit('surface_changed', changed({ withdrawn: true }));

    await waitFor(() => expect(getByText('No plugin surfaces')).toBeTruthy());
    expect(queryByText('crucible')).toBeNull();
    expect(fetches()).toBe(1);
  });

  // **The negative.** One plugin goes away while the browser draws another
  // plugin's panel. That panel must keep its rows.
  it('leaves another plugin panel drawn when one is withdrawn', async () => {
    served = [
      surface([{ id: 'a', text: 'crucible', detail: null, mark: null }]),
      surface([{ id: 'b', text: 'review queue', detail: null, mark: null }], { name: 'reviews', title: 'Reviews' }),
    ];
    const { getByText, queryByText } = render(() => <SurfacesPanel />);
    await waitFor(() => expect(getByText('crucible')).toBeTruthy());

    (await stream()).emit('surface_changed', changed({ name: 'reviews', withdrawn: true }));

    await waitFor(() => expect(queryByText('Reviews')).toBeNull());
    expect(getByText('crucible')).toBeTruthy();
    expect(fetches()).toBe(1);
  });

  // **The other negative.** Two plugins may declare one name. A withdrawal
  // names one of them, and the other one's panel must survive it.
  it('keeps a second plugin surface of the same name', async () => {
    served = [
      surface([{ id: 'a', text: 'crucible', detail: null, mark: null }]),
      surface([{ id: 'b', text: 'other plugin', detail: null, mark: null }], { plugin: 'q', title: 'Sessions (q)' }),
    ];
    const { getByText, queryByText } = render(() => <SurfacesPanel />);
    await waitFor(() => expect(getByText('crucible')).toBeTruthy());

    (await stream()).emit('surface_changed', changed({ withdrawn: true }));

    await waitFor(() => expect(queryByText('crucible')).toBeNull());
    expect(getByText('other plugin')).toBeTruthy();
    expect(fetches()).toBe(1);
  });

  // A withdrawal must not leave the chooser pointing at a surface that is gone.
  // If it did, a plugin that later re-declares the same name would silently
  // steal the panel back from whatever the user had selected.
  it('forgets a selection that named the withdrawn surface', async () => {
    served = [
      surface([{ id: 'a', text: 'crucible', detail: null, mark: null }]),
      surface([{ id: 'b', text: 'review queue', detail: null, mark: null }], { name: 'reviews', title: 'Reviews' }),
    ];
    const { getByText, queryByText } = render(() => <SurfacesPanel />);
    await waitFor(() => expect(getByText('Reviews')).toBeTruthy());

    getByText('Reviews').click();
    await waitFor(() => expect(getByText('review queue')).toBeTruthy());

    const source = await stream();
    source.emit('surface_changed', changed({ name: 'reviews', withdrawn: true }));
    await waitFor(() => expect(queryByText('review queue')).toBeNull());

    // The plugin comes back with the same name. The panel must stay where the
    // browser put it, not jump to a stale selection.
    served = [
      surface([{ id: 'a', text: 'crucible', detail: null, mark: null }]),
      surface([{ id: 'c', text: 'new review', detail: null, mark: null }], { name: 'reviews', title: 'Reviews' }),
    ];
    source.emit('surface_changed', changed({ name: 'reviews' }));

    await waitFor(() => expect(fetches()).toBe(2));
    await waitFor(() =>
      expect(getByText('crucible'), 'the panel stayed where the browser put it').toBeTruthy(),
    );
    expect(queryByText('new review'), 'a stale selection did not steal the panel').toBeNull();
  });

  it('explains itself when no plugin declares a surface', async () => {
    served = [];
    const { getByText } = render(() => <SurfacesPanel />);
    await waitFor(() => expect(getByText('No plugin surfaces')).toBeTruthy());
  });

  it('says a declared surface is empty rather than drawing nothing', async () => {
    served = [surface([])];
    const { getByText } = render(() => <SurfacesPanel />);
    await waitFor(() => expect(getByText('Nothing here yet')).toBeTruthy());
  });

  // Two plugins can each declare a panel, so the chooser has to exist — and must
  // not appear when there is only one, where the header already names it.
  //
  // Two cases, not one render after another: the roster is a shared cache entry
  // now, so a second panel in the same test would read the first one's answer
  // rather than a new roster. That sharing is the point of the migration, and
  // it is what the previous single case quietly relied on not happening.
  it('offers no chooser for a single surface', async () => {
    served = [surface([{ id: 'a', text: 'one', detail: null, mark: null }])];
    const { container, getByText } = render(() => <SurfacesPanel />);
    await waitFor(() => expect(getByText('one')).toBeTruthy());
    expect(container.querySelectorAll('button').length).toBe(0);
  });

  it('offers one chooser button per surface when there is more than one', async () => {
    served = [
      surface([{ id: 'a', text: 'one', detail: null, mark: null }]),
      surface([{ id: 'b', text: 'two', detail: null, mark: null }], { name: 'reviews', title: 'Reviews' }),
    ];
    const { container, getByText } = render(() => <SurfacesPanel />);
    await waitFor(() => expect(getByText('Reviews')).toBeTruthy());
    expect(container.querySelectorAll('button').length).toBe(2);
  });
});

// The stream itself. `lib/query/sse.ts` owns one source for the surface stream
// and every panel subscribes to it, so the count of EventSources is one
// whatever the count of panels on screen.
describe('the shared surface stream', () => {
  beforeEach(() => {
    served = [surface([{ id: 'a', text: 'crucible', detail: null, mark: null }])];
  });

  it('opens one EventSource for two panels', async () => {
    render(() => (
      <>
        <SurfacesPanel />
        <SurfacesPanel />
      </>
    ));

    const source = await stream();
    expect(source.url).toBe('/api/surfaces/events');
  });

  // The two panels share the cache entry as well as the source, so one write
  // reaches both without a fetch for either.
  it('gives both panels the same withdrawal off that one source', async () => {
    const { getAllByText, queryByText } = render(() => (
      <>
        <SurfacesPanel />
        <SurfacesPanel />
      </>
    ));
    await waitFor(() => expect(getAllByText('crucible')).toHaveLength(2));
    expect(fetches()).toBe(1);

    (await stream()).emit('surface_changed', changed({ withdrawn: true }));

    await waitFor(() => expect(queryByText('crucible')).toBeNull());
    expect(fetches()).toBe(1);
  });
});
