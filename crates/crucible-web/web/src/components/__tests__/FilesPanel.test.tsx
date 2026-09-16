import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, waitFor } from '@solidjs/testing-library';

// The old test read FilesPanel.tsx as a string and asserted the SOURCE did not
// contain emoji and did contain Lucide identifiers ("FileText", "Folder", …).
// That passes even if the component never renders. Here we RENDER FilesPanel
// with a mocked project + note listing and assert the emitted DOM: file rows
// carry real <svg> icons (Lucide) and no emoji glyph reaches the screen.

const EMOJI = ['📝', '🔷', '🟨', '🦀', '📋', '⚙️', '🎨', '🌐', '🌙', '📄', '📂', '📁'];

// `@/lib/api` is NOT mocked. Every read below answers on the wire instead
// (`createTestQueryEnv`), which is what makes the deduplication assertions
// mean something: a second panel that fetched behind the cache's back would
// still be counted here, and a module mock would hide it.
const listNotesMock = vi.fn();
// The roster this run's `GET /api/kilns` answers. The panel reads it through
// `useKilns`, which runs the real `listKilns` against the mocked fetch.
let kilnRoster: unknown[] = [];
const listDirMock = vi.fn();
// Mutable so one describe can browse a PROJECT root (lazy, listDir) while the
// rest use the kiln fallback (eager, listNotes).
let projectRoots: unknown[] = [];
// `file-tree-a11y` is deliberately NOT mocked. `currentOpenFilePath` reads the
// real `windowStore`, and the reveal it used to trigger only fired once the
// tree had finished loading — so a non-reactive stub produced a test that
// passed with the bug still in place. Driving the actual store is what makes
// the assertion mean anything.

/** The daemon's side of every read the panel makes, over the mocked fetch. */
function fsRoutes() {
  return {
    'GET /api/kilns': () => ({ kilns: kilnRoster }),
    'GET /api/notes': async (request: Request) => ({
      notes: await listNotesMock(new URL(request.url).searchParams.get('kiln')),
    }),
    'GET /api/fs/list': async (request: Request) => {
      const params = new URL(request.url).searchParams;
      return listDirMock(params.get('root'), params.get('rel_path') ?? '');
    },
  };
}

// The file tree builds its roster from projects + kilns. Use a KILN-only
// roster (no projects) so the deterministic fallback selects the kiln root and
// the panel loads notes via listNotes (the migrated Notes view).
vi.mock('@/contexts/ProjectContext', () => ({
  useProjectSafe: () => ({
    currentProject: () => null,
    projects: () => projectRoots,
    refreshProjects: async () => {},
  }),
}));

// The tree follows the ACTIVE SESSION: its root is the session's workspace or
// one of its attached kilns, so these specs must say which session is open.
// Without one there is nothing to browse, which is the correct empty state
// and not a fixture worth writing.
let currentSessionValue: unknown = null;

vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: () => currentSessionValue,
    applySessionScope: () => {},
  }),
}));

import { FilesPanel } from '../FilesPanel';
import { setStore } from '@/stores/windowStore';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource, onlyEventSource } from '@/test-utils/sse';
import { resetKilnsForTests } from '@/lib/query/kilns';
import { resetSseForTests } from '@/lib/query/sse';
import { installFsEventRoute } from '@/lib/query/routes/fs';
import { treeRootActions } from '@/stores/treeRootStore';

let env: TestQueryEnv;

/** Focus a file tab, the way opening a file in the editor does. */
function focusFileTab(filePath: string) {
  setStore('tabGroups', {
    center: {
      id: 'center',
      activeTabId: 'tab-1',
      tabs: [
        {
          id: 'tab-1',
          title: filePath.split('/').pop() ?? filePath,
          contentType: 'file',
          metadata: { filePath },
        },
      ],
    },
  } as never);
}

// One note per FileIcon branch so every extension arm is exercised in the DOM.
const NOTE_NAMES = [
  'readme.md', // FileText
  'main.ts', // FileCode
  'app.jsx', // FileCode
  'lib.rs', // FileCode
  'data.json', // FileJson
  'config.toml', // Cog
  'theme.css', // Palette
  'index.html', // Globe
  'init.lua', // Moon
  'notes.unknownext', // File (fallback)
];

beforeEach(() => {
  vi.clearAllMocks();
  setStore('tabGroups', {});
  projectRoots = [];
  // A kiln-attached session with no workspace, so the tree roots at the kiln
  // and loads notes through listNotes (the migrated Notes view).
  currentSessionValue = { id: 's-1', kilns: ['kiln'], workspace: null };
  localStorage.clear();
  // These assertions match full filenames; keep extensions visible (the tree
  // hides `.md` by default now).
  localStorage.setItem('crucible.filetree.hideExts', 'false');
  kilnRoster = [{ path: '/project/kiln', name: 'kiln' }];
  resetKilnsForTests();
  // jsdom has no `EventSource`, and the panel opens the shared filesystem
  // stream on mount. The fake one stands in for the daemon's.
  installFakeEventSource();
  env = createTestQueryEnv(fsRoutes());
  listNotesMock.mockResolvedValue(
    NOTE_NAMES.map((name) => ({
      name,
      path: name,
      title: null,
      tags: [],
      updated_at: '',
    })),
  );
});

afterEach(() => {
  env.restore();
  resetKilnsForTests();
  // The panel and the editor share one root in `lib/query/sse.ts`, and a root
  // outlives the test that opened it. Forget them between cases.
  resetSseForTests();
});

/**
 * The panel has exactly ONE root control.
 *
 * It used to have two: a strip of tabs for the session's own roots AND a
 * dropdown for the rest. They were the same piece of state behind two
 * affordances, so a root picked in the menu had no tab to live in and the two
 * disagreed about what "selected" meant. The strip is gone; these hold it
 * gone.
 */
describe('FilesPanel — one always-visible root selector', () => {
  it('renders the dropdown and no tab strip', async () => {
    const { findByTestId, queryByTestId, container } = render(() => <FilesPanel />);
    await findByTestId('root-dropdown');
    expect(queryByTestId('root-strip')).toBeNull();
    expect(container.querySelector('[data-testid^="root-tab-"]')).toBeNull();
  });

  it('shows the dropdown even with nothing to browse', async () => {
    // No session, no projects, no kilns: the emptiest state the panel has.
    currentSessionValue = null;
    projectRoots = [];
    kilnRoster = [];
    const { findByTestId } = render(() => <FilesPanel />);
    const trigger = await findByTestId('root-dropdown');
    expect(trigger.textContent).toContain('No roots');
  });

  it('offers the root dropdown from the empty state', async () => {
    // A dead end was the bug: the panel said there was nothing to browse and
    // gave no way to pick one. The action opens the dropdown the panel
    // already owns, rather than holding a second copy of its open flag.
    currentSessionValue = null;
    projectRoots = [];
    kilnRoster = [];
    const { findByTestId } = render(() => <FilesPanel />);

    const empty = await findByTestId('files-empty');
    expect(empty.textContent).toContain('No project or kiln to browse');
    const trigger = await findByTestId('root-dropdown');
    expect(trigger.getAttribute('aria-expanded')).toBe('false');

    fireEvent.click(empty.querySelector('[data-testid="empty-state-action"]')!);
    expect(trigger.getAttribute('aria-expanded')).toBe('true');
    // The list itself renders through a Portal, outside this container.
    expect(document.querySelector('[data-testid="root-dropdown-popout"]')).not.toBeNull();
  });

  it('names the browsed root on the trigger', async () => {
    const { findByTestId } = render(() => <FilesPanel />);
    const trigger = await findByTestId('root-dropdown');
    // The roster arrives with the kiln query, one tick after the first paint.
    await waitFor(() => expect(trigger.textContent).toContain('kiln'));
  });

  // Attaching is a separate, explicit gesture from browsing: picking a root
  // must never widen what the agent can read. The affordance therefore exists
  // only while an UNATTACHED kiln is the one on screen.
  it('offers Attach only for a browsed kiln the session has not attached', async () => {
    const { findByTestId, queryByTestId } = render(() => <FilesPanel />);
    await findByTestId('root-dropdown');
    // The fixture session is attached to its kiln, so there is nothing to attach.
    expect(queryByTestId('root-attach')).toBeNull();
  });
});

describe('FilesPanel — a project root loads once', () => {
  // The kiln query answers twice by design (stored list, then fetched), so
  // the roster changes twice per mount. `activeRoot` is a memo over a rebuilt
  // roster, so it handed back a NEW TreeRoot object each time and the
  // identity-keyed loader effect re-ran — refetching the root and every
  // persisted-expanded folder. That was the duplicate /api/fs/list per expand.
  const dir = (rel: string) => ({ rel_path: rel, name: rel.split('/').pop(), is_dir: true });
  const file = (rel: string) => ({ rel_path: rel, name: rel.split('/').pop(), is_dir: false });

  beforeEach(() => {
    projectRoots = [{ path: '/proj', name: 'proj', kilns: [], repository: null }];
    // This describe browses a PROJECT root, so the session acts in it.
    currentSessionValue = { id: 's-1', kilns: [], workspace: '/proj' };
    // A cached kilns value paints synchronously AND is corrected by the
    // response — the double-apply this guards against.
    localStorage.setItem('crucible:cache:kilns', JSON.stringify([{ path: '/vault', name: 'vault' }]));
    localStorage.setItem('crucible.filetree.expanded.project:/proj', JSON.stringify(['src']));
    listDirMock.mockImplementation(async (_root: string, rel: string) => ({
      entries:
        rel === ''
          ? [dir('src'), dir('later'), file('README.md')]
          : rel === 'later'
            ? [file('later/lazy.rs')]
            : [file('src/main.rs')],
      truncated: false,
    }));
  });

  it('keeps the existing rows when a lazily loaded folder arrives', async () => {
    // The tree used to be rebuilt from scratch on every first expand: the
    // merged tree came back via `onLoadedTree` -> `setRawRoot`, the `collection`
    // memo took a new identity, and a KEYED `<Show>` tore down and recreated
    // every row. Measured at 1134 visible rows, expanding a folder with two
    // children cost 1775ms of DOM work against 4ms of network, because the cost
    // tracked total rows (~1.3ms each) rather than children added.
    //
    // Asserted on node IDENTITY, not on row counts: counts stay equal across a
    // teardown-and-recreate, which is exactly what made this invisible.
    const { findByText, container } = render(() => <FilesPanel />);
    const readmeRow = await findByText('README.md');
    await findByText('main.rs'); // initial load settled
    const treeBefore = container.querySelector('[role="tree"]');
    expect(treeBefore).not.toBeNull();

    // `later` is deliberately NOT in the persisted-expanded set, so expanding it
    // goes through `loadChildren` -> `onLoadedTree` -> `setRawRoot` AFTER mount
    // — the path that used to remount. An already-expanded folder would not
    // exercise it, which is what made an earlier version of this test vacuous.
    fireEvent.click(await findByText('later'));
    await findByText('lazy.rs');

    expect(container.querySelector('[role="tree"]')).toBe(treeBefore);
    expect(container.contains(readmeRow)).toBe(true);
  });

  // Two panels on one root is a split, or the phone shell's sheet over the
  // desktop tree. They used to be two independent loaders: the same folder was
  // listed twice, and the two could disagree about what was in it. They read
  // one cache entry per folder now.
  it('lists each folder once for two panels browsing one root', async () => {
    const { findAllByText } = render(() => (
      <>
        <FilesPanel />
        <FilesPanel />
      </>
    ));

    await waitFor(async () => expect(await findAllByText('README.md')).toHaveLength(2));
    await waitFor(async () => expect(await findAllByText('main.rs')).toHaveLength(2));
    await new Promise((resolve) => setTimeout(resolve, 150));

    // The root and the one persisted-expanded folder, once each — not once
    // per panel.
    expect(listDirMock.mock.calls.map((c) => c[1])).toEqual(['', 'src']);
    expect(env.fetch.calls('GET /api/fs/list')).toBe(2);
  });

  it('fetches each directory exactly once despite the cache-then-fetch double apply', async () => {
    const { findByText } = render(() => <FilesPanel />);
    await findByText('README.md');
    // Let any second pass land before counting.
    await new Promise((resolve) => setTimeout(resolve, 150));

    // Pre-fix this was ['', '', 'src', 'src'] — the root and every
    // persisted-expanded folder, once per pass.
    expect(listDirMock.mock.calls.map((c) => c[1])).toEqual(['', 'src']);
  });
});

describe('FilesPanel — the navigator does not follow the focused tab', () => {
  // Opening a file used to expand the tree to it and scroll, on every tab
  // switch, moving the navigator out from under the pointer while browsing.
  const NESTED = [
    { name: 'top.md', path: 'top.md' },
    { name: 'buried.md', path: 'folder/buried.md' },
  ];

  beforeEach(() => {
    listNotesMock.mockResolvedValue(
      NESTED.map(({ name, path }) => ({ name, path, title: null, tags: [], updated_at: '' })),
    );
  });

  /** The folder row's expansion state, which is what auto-reveal changed.
   *  Asserted via `aria-expanded` rather than the absence of `buried.md`,
   *  which would pass for the wrong reason the moment a descendant is filtered
   *  out or lazily unloaded — `aria-expanded` names the state under test. */
  const folderExpanded = (container: HTMLElement) =>
    Array.from(container.querySelectorAll('[role="treeitem"]'))
      .find((el) => el.textContent?.trim().startsWith('folder'))
      ?.getAttribute('aria-expanded');

  it('does not expand to the focused tab when a file is opened after the tree loads', async () => {
    const { container, findByText } = render(() => <FilesPanel />);
    // Wait for the tree to finish loading FIRST: the reveal this guards against
    // needed a live tree api and a built collection, so asserting before the
    // load settles proves nothing.
    await findByText('folder');
    expect(folderExpanded(container)).toBe('false');

    focusFileTab('/project/kiln/folder/buried.md');
    await new Promise((resolve) => setTimeout(resolve, 120));

    expect(folderExpanded(container)).toBe('false');
  });

  // A "file already open at mount" case is deliberately absent: the old
  // auto-reveal fired before the tree had loaded and no-opped, so such a test
  // passed with the bug in place. Verified by running this file against the
  // pre-fix component — only the case above discriminates (it failed with
  // `expected 'true' to be 'false'`).
});

/**
 * The tree keeps a root that was picked before any session existed.
 *
 * This is the ordering `c4-entities.live.spec.ts` meets against the daemon:
 * the shell loads with no session selected, the user picks a kiln, and the
 * session list then arrives carrying one live session. The pin of that pick is
 * held under `NO_SESSION_PIN_KEY`, which is deliberately not a session id — so
 * the prune that forgets dead sessions' pins forgot this one too, the tree
 * dropped to "No project or kiln to browse", and the note index lost its only
 * reader. The daemon's file event then had nothing to refresh.
 */
describe('FilesPanel — a root picked before any session', () => {
  beforeEach(() => {
    currentSessionValue = null;
    kilnRoster = [{ path: '/project/kiln', name: 'kiln' }];
    // The app installs the stream's routes at start (`src/index.tsx`), and
    // `createTestQueryEnv` forgets them again between cases. Without this the
    // event below reaches the panel and no cache at all.
    installFsEventRoute();
  });

  /** Picks `name` from the root dropdown, the way a user does. */
  async function pickRoot(trigger: HTMLElement, name: string) {
    fireEvent.click(trigger);
    const row = await waitFor(() => {
      const found = document
        .querySelector('[data-testid="root-dropdown-popout"]')
        ?.querySelector('[role="option"]');
      expect(found?.textContent).toContain(name);
      return found as HTMLElement;
    });
    fireEvent.click(row);
  }

  it('keeps the root when the session list arrives, and still refreshes on a file event', async () => {
    const { findByTestId, findByText } = render(() => <FilesPanel />);
    const trigger = await findByTestId('root-dropdown');
    await waitFor(() => expect(kilnRoster.length).toBe(1));
    await pickRoot(trigger, 'kiln');
    await findByText('readme.md');
    expect(listNotesMock).toHaveBeenCalledTimes(1);

    // The session list lands. `SessionContext` prunes the pins of sessions
    // that are gone; this browse belongs to none of them.
    treeRootActions.prune(['s-1']);

    // The daemon says a note was written into the browsed kiln.
    onlyEventSource().emit('fs_changed', {
      type: 'changed',
      path: '/project/kiln/fresh.md',
      kind: 'created',
    });

    // One refetch, into the entry the tree is reading.
    await waitFor(() => expect(listNotesMock).toHaveBeenCalledTimes(2));
    expect(listNotesMock).toHaveBeenLastCalledWith('/project/kiln');
  });
});

describe('FilesPanel — rendered rows', () => {
  it('renders a colored filetype <svg> for every file row (never emoji text)', async () => {
    const { container, findByText } = render(() => <FilesPanel />);
    await findByText('readme.md');
    for (const name of NOTE_NAMES) {
      await findByText(name);
    }
    // Each file row carries a FileIcon <svg>; at least one per note.
    const svgs = container.querySelectorAll('svg');
    expect(svgs.length).toBeGreaterThanOrEqual(NOTE_NAMES.length);
  });

  it('emits no emoji glyphs anywhere in the rendered panel', async () => {
    const { container, findByText } = render(() => <FilesPanel />);
    await findByText('readme.md');

    const text = container.textContent ?? '';
    for (const glyph of EMOJI) {
      expect(text).not.toContain(glyph);
    }
  });
});
