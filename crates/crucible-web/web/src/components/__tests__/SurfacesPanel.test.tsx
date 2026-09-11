import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import type { Surface } from '@/lib/api';

const getSurfacesMock = vi.fn();
/** Captures the SSE callback so a test can fire a change without a server. */
let surfaceListener: (() => void) | null = null;
const unsubscribeMock = vi.fn();

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getSurfaces: (...args: unknown[]) => getSurfacesMock(...args),
  subscribeToSurfaceEvents: (cb: () => void) => {
    surfaceListener = cb;
    return unsubscribeMock;
  },
}));

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
    surfaceListener?.();

    await waitFor(() => expect(getByText('second')).toBeTruthy());
    expect(getSurfacesMock).toHaveBeenCalledTimes(2);
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
