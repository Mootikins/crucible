import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent, screen } from '@solidjs/testing-library';
import { SearchPanel } from '../SearchPanel';

const selectSessionMock = vi.fn();
vi.mock('@/contexts/ProjectContext', () => ({
  useProjectSafe: () => ({ projects: () => [{ path: '/repos/app', name: 'app', kilns: [] }] }),
}));
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({ selectSession: selectSessionMock }),
}));
// swrLocal just runs the fetcher and pipes it to the setter.
vi.mock('@/lib/local-cache', () => ({
  swrLocal: (_k: string, fetcher: () => Promise<unknown>, setter: (v: unknown) => void) => {
    void fetcher().then(setter);
  },
}));

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
  searchSessions: vi.fn(async () => [
    { id: 's1', title: 'Trust session', started_at: '2026-07-20T00:00:00Z' },
  ]),
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

  it('facet toggle hides a source section', async () => {
    render(() => <SearchPanel />);
    fireEvent.input(screen.getByTestId('search-input'), { target: { value: 'trust' } });
    await waitFor(() => expect(screen.getByTestId('search-session-hit')).toBeTruthy());

    fireEvent.click(screen.getByTestId('search-facet-sessions'));
    await waitFor(() => expect(screen.queryByTestId('search-session-hit')).toBeNull());
  });
});
