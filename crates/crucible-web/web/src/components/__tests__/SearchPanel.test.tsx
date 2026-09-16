import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent, screen } from '@solidjs/testing-library';
import { SearchPanel } from '../SearchPanel';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { resetKilnsForTests } from '@/lib/query/kilns';

const selectSessionMock = vi.fn();
vi.mock('@/contexts/ProjectContext', () => ({
  useProjectSafe: () => ({ projects: () => [{ path: '/repos/app', name: 'app', kilns: [] }] }),
}));
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({ selectSession: selectSessionMock, currentSession: () => undefined }),
}));
const openFileMock = vi.fn();
vi.mock('@/lib/file-actions', () => ({ openFileInEditor: (...a: unknown[]) => openFileMock(...a) }));

/**
 * Nothing in `@/lib/api` is stubbed.
 *
 * The three searches are held in the query cache now, so a mocked module
 * counts the calls that reach it rather than the calls that reach the daemon —
 * and the debounce, which is the point of half of these cases, would not be
 * observable at all.
 */
let env: TestQueryEnv;
/** Every grep the daemon answered: its root, its query and its glob. */
let greps: { root: string; query: string; glob: string | null }[] = [];
/** Every session search the daemon answered: its query, its kilns, its limit. */
let sessionSearches: { query: string; kilns: string[]; limit: string | null }[] = [];

beforeEach(() => {
  localStorage.clear();
  resetKilnsForTests();
  greps = [];
  sessionSearches = [];
  env = createTestQueryEnv({
    'GET /api/kilns': () => ({ kilns: [{ path: '/kilns/main', name: 'main' }] }),
    'GET /api/config': () => ({ kiln_path: '/kilns/main' }),
    'POST /api/search/grep': async (request: Request) => {
      const body = (await request.json()) as { root: string; query: string; glob: string | null };
      greps.push(body);
      // The notes call carries the markdown glob; the files call does not.
      return body.glob === '*.md'
        ? {
            truncated: false,
            hits: [
              { path: `${body.root}/Trust.md`, rel_path: 'Trust.md', line: 3, text: 'derived trust is the boundary', match_start: 8, match_end: 13 },
            ],
          }
        : {
            truncated: false,
            hits: [
              { path: `${body.root}/trust.rs`, rel_path: 'src/trust.rs', line: 12, text: 'fn resolve_trust()', match_start: 11, match_end: 16 },
            ],
          };
    },
    'GET /api/sessions/search': (request: Request) => {
      const params = new URL(request.url).searchParams;
      sessionSearches.push({
        query: params.get('q') ?? '',
        kilns: params.getAll('kiln'),
        limit: params.get('limit'),
      });
      return [{ id: 's1', title: 'Trust session', started_at: '2026-07-20T00:00:00Z' }];
    },
  });
});

afterEach(() => {
  cleanup();
  env.restore();
  resetKilnsForTests();
  vi.clearAllMocks();
});

describe('SearchPanel', () => {
  it('fans a query out to notes (glob *.md), files, and sessions', async () => {
    render(() => <SearchPanel />);
    fireEvent.input(screen.getByTestId('search-input'), { target: { value: 'trust' } });

    await waitFor(() =>
      expect(greps).toContainEqual({ root: '/kilns/main', query: 'trust', glob: '*.md', limit: 60, case_insensitive: true }),
    );
    await waitFor(() =>
      expect(greps).toContainEqual({ root: '/repos/app', query: 'trust', glob: null, limit: 60, case_insensitive: true }),
    );

    // Note + file hits render; a session hit renders.
    await waitFor(() => expect(screen.getAllByTestId('search-hit').length).toBe(2));
    await waitFor(() =>
      expect(screen.getByTestId('search-session-hit').textContent).toContain('Trust session'),
    );
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
      expect(greps.map((g) => g.root)).toContain('/kilns/main'),
    );
    await waitFor(() => expect(sessionSearches).toHaveLength(1));
    expect(sessionSearches[0]).toEqual({ query: 'trust', kilns: ['main'], limit: '30' });
    // Neither call carries the other's spelling.
    expect(greps.map((g) => g.root)).not.toContain('main');
    expect(sessionSearches[0].kilns).not.toContain('/kilns/main');
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
    await waitFor(() => expect(screen.getByTestId('search-session-hit')).toBeTruthy());
  });

  /**
   * One request per settled query, not one per keystroke.
   *
   * Grep walks a tree and a semantic search embeds the text with a provider,
   * so a request per character is a walk per character. The wait used to be
   * the panel's own timer; it belongs to the reads it throttles.
   */
  it('waits out the typing instead of searching on every keystroke', async () => {
    render(() => <SearchPanel />);
    const input = screen.getByTestId('search-input');

    // Spaced the way a person types, and inside one debounce window between
    // them: a panel that asked per keystroke would run five searches here.
    for (const partial of ['t', 'tr', 'tru', 'trus', 'trust']) {
      fireEvent.input(input, { target: { value: partial } });
      await new Promise((resolve) => setTimeout(resolve, 30));
    }

    await waitFor(() => expect(screen.getAllByTestId('search-hit').length).toBe(2));
    await waitFor(() => expect(sessionSearches).toHaveLength(1));
    expect(greps.map((g) => g.query)).toEqual(['trust', 'trust']);
    expect(sessionSearches.map((search) => search.query)).toEqual(['trust']);
  });
});
