import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';

/**
 * The Navigator's scope swapper (the file-tree root/directory dropdown).
 *
 * Two things are pinned here:
 *
 *  1. The menu must live OUTSIDE the panel's own subtree, positioned against
 *     the viewport. The Navigator renders inside `EdgePanel`, whose slide frame
 *     is `overflow-hidden` and whose inner wrapper always carries a `translate`
 *     — that is a stacking context AND a containing block, so an in-flow
 *     `absolute` menu is clipped at the panel's right edge and painted under
 *     the center pane no matter what z-index it asks for. This is the same
 *     constraint `ChipSelect` documents and solves with a Portal.
 *
 *  2. The roster is a faithful projection of the registered projects/kilns —
 *     there are no synthetic entries. A root shows up exactly while something
 *     is registered at it, and unregistering one must not disturb its
 *     neighbours.
 */

const listKilnsMock = vi.fn();
const listNotesMock = vi.fn();
const listDirMock = vi.fn();

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  listKilns: (...args: unknown[]) => listKilnsMock(...args),
  listNotes: (...args: unknown[]) => listNotesMock(...args),
  listDir: (...args: unknown[]) => listDirMock(...args),
  // No EventSource in jsdom: hand back a no-op unsubscribe.
  subscribeToFsEvents: () => () => {},
}));

const projectsMock = vi.fn();

vi.mock('@/contexts/ProjectContext', () => ({
  useProjectSafe: () => ({
    currentProject: () => null,
    projects: () => projectsMock(),
    refreshProjects: async () => {},
  }),
}));

import { NavigatorPanel } from '../NavigatorPanel';

/** The developer registry that produced the reported dropdown, verbatim. */
const SCRATCHPAD_KILN = '/tmp/scratch/f1-live/kiln';
const REGISTERED_PROJECTS = [
  {
    path: '/home/dev/crucible',
    name: 'crucible',
    kilns: [{ path: '/home/dev/crucible/docs', name: 'crucible-docs' }],
  },
  { path: '/home/dev/models', name: 'models', kilns: [] },
  { path: SCRATCHPAD_KILN, name: 'kiln', kilns: [{ path: SCRATCHPAD_KILN, name: null }] },
];

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  projectsMock.mockReturnValue(REGISTERED_PROJECTS);
  listKilnsMock.mockResolvedValue([{ path: '/home/dev/crucible/docs', name: 'crucible-docs' }]);
  listNotesMock.mockResolvedValue([]);
  listDirMock.mockResolvedValue([]);
});

/** Open the swapper and hand back the menu element. */
async function openScopeMenu(): Promise<HTMLElement> {
  fireEvent.click(await screen.findByTestId('navigator-swapper'));
  return screen.findByTestId('navigator-scope-menu');
}

describe('NavigatorPanel — scope menu stacking', () => {
  it('renders scope rows outside the panel subtree', async () => {
    // Deliberately identified by a row testid that predates the fix, so this
    // reads as a behaviour change rather than "a new testid appeared".
    const { container } = render(() => <NavigatorPanel />);
    fireEvent.click(await screen.findByTestId('navigator-swapper'));
    const row = await screen.findByTestId('navigator-scope-sessions');

    expect(container.contains(row)).toBe(false);
  });

  it('renders the scope menu outside the panel subtree, so no ancestor overflow clips it', async () => {
    const { container } = render(() => <NavigatorPanel />);
    const menu = await openScopeMenu();

    expect(container.contains(menu)).toBe(false);
    expect(document.body.contains(menu)).toBe(true);
  });

  it('positions the scope menu against the viewport, not against a transformed ancestor', async () => {
    const { unmount } = render(() => <NavigatorPanel />);
    const menu = await openScopeMenu();

    expect(menu.style.position).toBe('fixed');
    expect(menu.style.left).toMatch(/^-?\d+(\.\d+)?px$/);
    expect(menu.style.top || menu.style.bottom).toMatch(/^-?\d+(\.\d+)?px$/);
    unmount();
  });

  it('closes when a click lands outside both the trigger and the portaled menu', async () => {
    render(() => <NavigatorPanel />);
    await openScopeMenu();

    fireEvent.click(document.body);
    expect(screen.queryByTestId('navigator-scope-menu')).toBeNull();
  });

  it('keeps the menu open for a click on one of its own rows until that row acts', async () => {
    render(() => <NavigatorPanel />);
    const menu = await openScopeMenu();

    // A row click must reach its handler; the outside-click closer must not
    // treat the portaled menu as "outside" and swallow it first.
    fireEvent.click(screen.getByTestId('navigator-scope-sessions'));
    expect(document.body.contains(menu)).toBe(false);
    expect(await screen.findByTestId('navigator-swapper')).toHaveTextContent('Sessions');
  });
});

describe('NavigatorPanel — scope menu roster', () => {
  it('lists a root exactly while something is registered at it', async () => {
    const { unmount } = render(() => <NavigatorPanel />);
    const menu = await openScopeMenu();

    // Every registered project, including the scratchpad one named "kiln"
    // whose entry the owner reported as errant. It is not synthetic — it is
    // what the registry says.
    expect(menu.textContent).toContain('crucible');
    expect(menu.textContent).toContain('models');
    expect(menu.textContent).toContain('kiln');
    unmount();

    // Unregister only the scratchpad project: its entry goes, its neighbours
    // stay. Nothing in the component filters by name.
    projectsMock.mockReturnValue(REGISTERED_PROJECTS.filter((p) => p.path !== SCRATCHPAD_KILN));
    render(() => <NavigatorPanel />);
    const after = await openScopeMenu();
    expect(after.textContent).toContain('crucible');
    expect(after.textContent).toContain('models');
    expect(after.textContent).not.toContain('kiln');
  });
});
