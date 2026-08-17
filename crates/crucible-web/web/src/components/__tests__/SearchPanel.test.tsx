import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent, screen } from '@solidjs/testing-library';
import { SearchPanel } from '../SearchPanel';

const selectSessionMock = vi.fn();
vi.mock('@/contexts/ProjectContext', () => ({
  useProjectSafe: () => ({ projects: () => [{ path: '/repos/app', name: 'app', kilns: [] }] }),
}));
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({ selectSession: selectSessionMock, currentSession: () => undefined }),
}));
// swrLocal just runs the fetcher and pipes it to the setter.
vi.mock('@/lib/local-cache', () => ({
  swrLocal: (_k: string, fetcher: () => Promise<unknown>, setter: (v: unknown) => void) => {
    void fetcher().then(setter);
  },
}));

const searchSessionsMock = vi.fn(
  async (_q: string, _kiln?: string | string[], _limit?: number) => [
    { id: 's1', title: 'Trust session', started_at: '2026-07-20T00:00:00Z' },
  ],
);

const openFileMock = vi.fn();
vi.mock('@/lib/file-actions', () => ({ openFileInEditor: (...a: unknown[]) => openFileMock(...a) }));

const grepMock = vi.fn(async (root: string, _q: string, opts?: { glob?: string }) => {
  // Notes call carries glob '*.md'; the files call does not.
  if (opts?.glob === '*.md') {
    return {
      truncated: false,
      hits: [
        { path: `${root}/Trust.md`, relPath: 'Trust.md', line: 3, text: 'derived trust is the boundary', matchStart: 8, matchEnd: 13 },
      ],
    };
  }
  return {
    truncated: false,
    hits: [
      { path: `${root}/trust.rs`, relPath: 'src/trust.rs', line: 12, text: 'fn resolve_trust()', matchStart: 11, matchEnd: 16 },
    ],
  };
});

vi.mock('@/lib/api', () => ({
  getConfig: vi.fn(async () => ({ kiln_path: '/kilns/main' })),
  listKilns: vi.fn(async () => [{ path: '/kilns/main', name: 'main' }]),
  grepSearch: (...a: [string, string, { glob?: string }?]) => grepMock(...a),
  searchSessions: (...a: Parameters<typeof searchSessionsMock>) => searchSessionsMock(...a),
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('SearchPanel', () => {
  it('fans a query out to notes (glob *.md), files, and sessions', async () => {
    render(() => <SearchPanel />);
    fireEvent.input(screen.getByTestId('search-input'), { target: { value: 'trust' } });

    await waitFor(() =>
      expect(grepMock).toHaveBeenCalledWith('/kilns/main', 'trust', expect.objectContaining({ glob: '*.md' })),
    );
    expect(grepMock).toHaveBeenCalledWith('/repos/app', 'trust', expect.not.objectContaining({ glob: '*.md' }));

    // Note + file hits render; a session hit renders.
    await waitFor(() => expect(screen.getAllByTestId('search-hit').length).toBe(2));
    expect(screen.getByTestId('search-session-hit').textContent).toContain('Trust session');
  });

  it('highlights the matched span and opens a hit', async () => {
    render(() => <SearchPanel />);
    fireEvent.input(screen.getByTestId('search-input'), { target: { value: 'trust' } });
    await waitFor(() => expect(screen.getAllByTestId('search-hit').length).toBe(2));

    // The <mark> carries the matched substring.
    const mark = document.querySelector('mark');
    expect(mark?.textContent).toBe('trust');

    fireEvent.click(screen.getAllByTestId('search-hit')[0]);
    expect(openFileMock).toHaveBeenCalled();
  });

  // Same constraint as the Navigator's scope swapper: SearchPanel renders
  // inside the left EdgePanel, whose slide frame is `overflow-hidden` and whose
  // inner wrapper always carries a `translate` (a stacking context AND a
  // containing block). An in-flow `absolute` menu is clipped at the panel edge
  // and painted under the center pane; only a portal escapes.
  it('renders the scope menu outside the panel subtree, so no ancestor overflow clips it', async () => {
    const { container } = render(() => <SearchPanel />);
    fireEvent.click(screen.getByTestId('search-scope'));

    const menu = await screen.findByTestId('search-scope-menu');
    expect(container.contains(menu)).toBe(false);
    expect(document.body.contains(menu)).toBe(true);
    expect(menu.style.position).toBe('fixed');
  });

  // The end-to-end round trip a kiln scope has to survive: the picker stores a
  // kiln's NAME and its DIRECTORY separately, and each of the two consumers
  // gets the one it actually takes. They used to share one `path` field, so
  // whichever consumer was wrong searched nothing — the note grep ran against a
  // bare name, or the session search sent a path the route drops.
  it('scoping to a kiln greps its directory and searches sessions by its name', async () => {
    render(() => <SearchPanel />);
    fireEvent.click(screen.getByTestId('search-scope'));
    await waitFor(() => expect(screen.getByTestId('search-scope-kiln-main')).toBeTruthy());
    fireEvent.click(screen.getByTestId('search-scope-kiln-main'));

    fireEvent.input(screen.getByTestId('search-input'), { target: { value: 'trust' } });

    await waitFor(() =>
      expect(grepMock).toHaveBeenCalledWith(
        '/kilns/main',
        'trust',
        expect.objectContaining({ glob: '*.md' }),
      ),
    );
    await waitFor(() =>
      expect(searchSessionsMock).toHaveBeenCalledWith('trust', 'main', 30),
    );
    // Neither call carries the other's spelling.
    expect(grepMock).not.toHaveBeenCalledWith('main', expect.anything(), expect.anything());
    expect(searchSessionsMock).not.toHaveBeenCalledWith('trust', '/kilns/main', 30);
  });

  it('scoping to Sessions drops the notes/files sections', async () => {
    render(() => <SearchPanel />);
    fireEvent.input(screen.getByTestId('search-input'), { target: { value: 'trust' } });
    // Default (no current session) = Everywhere → notes + files hits present.
    await waitFor(() => expect(screen.getAllByTestId('search-hit').length).toBe(2));

    fireEvent.click(screen.getByTestId('search-scope'));
    await waitFor(() => expect(screen.getByTestId('search-scope-sessions')).toBeTruthy());
    fireEvent.click(screen.getByTestId('search-scope-sessions'));

    await waitFor(() => expect(screen.queryAllByTestId('search-hit').length).toBe(0));
    expect(screen.getByTestId('search-session-hit')).toBeTruthy();
  });
});
