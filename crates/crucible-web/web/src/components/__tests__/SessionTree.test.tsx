import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent, waitFor } from '@solidjs/testing-library';
import { SessionTree } from '../SessionTree';
import type { Project, Session } from '@/lib/types';

const session = (over: Partial<Session>): Session => ({
  id: 'sid',
  session_type: 'chat',
  kilns: ['/kilns/main'],
  workspace: '/kilns/main',
  state: 'active',
  title: 'a session',
  agent_model: 'm',
  agent_mode: null,
  started_at: '2026-07-22T10:00:00Z',
  last_activity: '2026-07-22T10:00:00Z',
  event_count: 0,
  archived: false,
  ...over,
});

const project = (path: string, name: string, repo?: { root: string; is_worktree: boolean }): Project => ({
  path,
  name,
  kilns: [],
  last_accessed: '',
  ...(repo
    ? {
        repository: {
          root: repo.root,
          remote_url: undefined,
          is_worktree: repo.is_worktree,
          main_repo_git_dir: repo.is_worktree ? '/repo/.git' : undefined,
        },
      }
    : {}),
});

const baseProps = {
  onSelectSession: vi.fn(),
  onSelectProject: vi.fn(),
  onNewSession: vi.fn(),
  onArchiveSession: vi.fn(),
  onDeleteSession: vi.fn(),
  branchOf: (ws: string) => (ws.includes('tree/') ? 'feat/x' : 'master'),
  kilnName: (p: string) => p.split('/').pop() ?? null,
};

beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
});

describe('SessionTree', () => {
  it('groups sessions by project and folds worktree sessions into the repo group', () => {
    const projects = [
      project('/repo', 'crucible', { root: '/repo', is_worktree: false }),
      project('/repo/tree/feat/x', 'x', { root: '/repo', is_worktree: true }),
      project('/other', 'other'),
    ];
    const sessions = [
      session({ id: 's-main', workspace: '/repo' }),
      session({ id: 's-wt', workspace: '/repo/tree/feat/x' }),
      session({ id: 's-none', workspace: '/kilns/main' }), // workspace == kiln → no project
    ];
    const { getByTestId, queryByTestId } = render(() => (
      <SessionTree sessions={sessions} projects={projects} {...baseProps} />
    ));

    // ONE group for the repo (main + worktree), one session-folders bucket.
    // 'other' has no session; it is still listed, uncounted.
    const repoGroup = getByTestId('session-group-/repo');
    expect(repoGroup).toBeTruthy();
    expect(repoGroup.textContent).toContain('2');
    expect(getByTestId('session-group-/other').textContent).not.toMatch(/\d/);
    expect(getByTestId('session-group-::none')).toBeTruthy();
    expect(queryByTestId('session-group-x')).toBeNull(); // worktree never a group

    // All three sessions render as rows.
    expect(getByTestId('session-item-s-main')).toBeTruthy();
    expect(getByTestId('session-item-s-wt')).toBeTruthy();
    expect(getByTestId('session-item-s-none')).toBeTruthy();

    // The branch chip stays on the worktree session's row: inside one project
    // group it is the thing that tells two checkouts apart.
    const wtRow = getByTestId('session-item-s-wt');
    expect(wtRow.textContent).toContain('feat/x');
    // The kiln does NOT: every session here shares it, so printing it on each
    // row filled a column with a word that distinguished nothing.
    expect(wtRow.textContent).not.toContain('main');
  });

  it('shows a kiln only on the row whose kiln differs from its siblings', () => {
    const projects = [project('/repo', 'crucible', { root: '/repo', is_worktree: false })];
    const sessions = [
      session({ id: 'a', workspace: '/repo', kilns: ['/kilns/docs'] }),
      session({ id: 'b', workspace: '/repo', kilns: ['/kilns/docs'] }),
      session({ id: 'odd', workspace: '/repo', kilns: ['/kilns/scratch'] }),
    ];
    const { getByTestId } = render(() => (
      <SessionTree sessions={sessions} projects={projects} {...baseProps} />
    ));

    expect(getByTestId('session-item-a').textContent).not.toContain('docs');
    expect(getByTestId('session-item-odd').textContent).toContain('scratch');
  });

  it('collapsing a group hides its rows and persists', () => {
    const projects = [project('/repo', 'crucible', { root: '/repo', is_worktree: false })];
    const sessions = [session({ id: 's1', workspace: '/repo' })];
    const { getByTestId, queryByTestId } = render(() => (
      <SessionTree sessions={sessions} projects={projects} {...baseProps} />
    ));

    fireEvent.click(getByTestId('session-group-/repo'));
    expect(queryByTestId('session-item-s1')).toBeNull();
    expect(JSON.parse(localStorage.getItem('crucible:sessionTree.collapsed')!)).toContain('/repo');
  });

  it('orders sessions inside a group by recency, newest first', () => {
    const projects = [project('/repo', 'crucible', { root: '/repo', is_worktree: false })];
    const sessions = [
      session({ id: 'old', workspace: '/repo', last_activity: '2026-07-20T10:00:00Z' }),
      session({ id: 'new', workspace: '/repo', last_activity: '2026-07-22T10:00:00Z' }),
    ];
    const { getByTestId } = render(() => (
      <SessionTree sessions={sessions} projects={projects} {...baseProps} />
    ));
    const list = getByTestId('session-tree');
    const ids = [...list.querySelectorAll('[data-testid^="session-item-"]')].map((el) =>
      el.getAttribute('data-testid'),
    );
    expect(ids).toEqual(['session-item-new', 'session-item-old']);
  });

  it('gives the whole group row ONE action — toggle', () => {
    const projects = [project('/repo', 'crucible', { root: '/repo', is_worktree: false })];
    const sessions = [session({ id: 's1', workspace: '/repo' })];
    const { getByTestId, getByText } = render(() => (
      <SessionTree sessions={sessions} projects={projects} {...baseProps} />
    ));

    // The name used to pin the project while the rest of the same row
    // toggled — one highlighted row, two outcomes decided by x-coordinate,
    // with no visual seam and no keyboard path to the second. Pinning is a
    // context-menu action now.
    fireEvent.click(getByText('crucible'));
    expect(baseProps.onSelectProject).not.toHaveBeenCalled();
    expect(getByTestId('session-group-/repo').getAttribute('aria-expanded')).toBe('false');
  });

  // Zero kilns is a legitimate session shape, so a row must say nothing about
  // kilns rather than resolve the empty path — `kilnLabel('')` is "Home kiln",
  // which would claim an attachment the session does not have.
  it('a kiln-less session row carries no kiln name', () => {
    const projects = [project('/repo', 'crucible', { root: '/repo', is_worktree: false })];
    const sessions = [session({ id: 'tools-only', kilns: [], workspace: '/repo' })];
    const kilnName = (p: string) => (p ? p.split('/').pop()! : 'Home kiln');
    const { getByTestId } = render(() => (
      <SessionTree sessions={sessions} projects={projects} {...baseProps} kilnName={kilnName} />
    ));
    expect(getByTestId('session-item-tools-only').textContent).not.toContain('Home kiln');
  });
});

describe('SessionTree — New Session belongs to the project', () => {
  const projects = [project('/repo', 'crucible'), project('/other', 'other')];
  const sessions = [
    session({ id: 's-main', workspace: '/repo' }),
    session({ id: 's-other', workspace: '/other' }),
    session({ id: 's-loose', workspace: '/kilns/main' }), // no project
  ];

  it('offers New Session on each project row, aimed at that project', () => {
    const onNewSession = vi.fn();
    const { getByTestId } = render(() => (
      <SessionTree
        sessions={sessions}
        projects={projects}
        {...baseProps}
        onNewSession={onNewSession}
      />
    ));

    fireEvent.click(getByTestId('session-group-new-/other'));
    // The path, not the name: the draft opens aimed at a directory.
    expect(onNewSession).toHaveBeenCalledWith('/other');
  });

  it('keeps two projects with the same basename apart', () => {
    // Multi-root guarantees duplicate basenames, and this tree deliberately
    // names worktree groups by basename — so a testid or a key derived from the
    // display name collides on the common case, not the exotic one.
    const onNewSession = vi.fn();
    const dupes = [project('/work/api', 'api'), project('/oss/api', 'api')];
    const twice = [
      session({ id: 's-work', workspace: '/work/api' }),
      session({ id: 's-oss', workspace: '/oss/api' }),
    ];
    const { getByTestId } = render(() => (
      <SessionTree sessions={twice} projects={dupes} {...baseProps} onNewSession={onNewSession} />
    ));

    expect(getByTestId('session-group-/work/api')).toBeTruthy();
    expect(getByTestId('session-group-/oss/api')).toBeTruthy();
    fireEvent.click(getByTestId('session-group-new-/oss/api'));
    expect(onNewSession).toHaveBeenCalledWith('/oss/api');
  });

  it('offers no New Session on the project-less group', () => {
    const { queryByTestId, getByTestId } = render(() => (
      <SessionTree sessions={sessions} projects={projects} {...baseProps} />
    ));

    // The bucket for sessions with no project. There is no project there to
    // start one in, so the row must not pretend otherwise.
    expect(getByTestId('session-group-::none')).toBeTruthy();
    expect(queryByTestId('session-group-new-::none')).toBeNull();
  });

  it('keeps the project tier collapsible over its sessions', () => {
    const { getByTestId, queryByTestId } = render(() => (
      <SessionTree sessions={sessions} projects={projects} {...baseProps} />
    ));

    expect(queryByTestId('session-item-s-main')).toBeTruthy();
    fireEvent.click(getByTestId('session-group-/repo'));
    expect(queryByTestId('session-item-s-main')).toBeNull();
    fireEvent.click(getByTestId('session-group-/repo'));
    expect(queryByTestId('session-item-s-main')).toBeTruthy();
  });
});

describe('SessionTree — the project row context menu', () => {
  const projects = [project('/repo', 'crucible'), project('/other', 'other')];
  const sessions = [
    session({ id: 's-main', workspace: '/repo' }),
    session({ id: 's-other', workspace: '/other' }),
  ];

  // The menu renders through a Portal, so it lives on `document`, not inside
  // the render container. Scope to the OPEN content: ark keeps the closed
  // content mounted, so counting items anywhere would find these two rows even
  // with no menu on screen — an assertion that could never fail.
  const openMenu = () =>
    document.querySelector<HTMLElement>('[data-scope="menu"][data-part="content"][data-state="open"]');
  const menuItems = () => [...(openMenu()?.querySelectorAll<HTMLElement>('[data-part="item"]') ?? [])];
  const menuLabels = () => menuItems().map((i) => i.textContent?.trim());

  /** zag highlights on pointerdown and selects the HIGHLIGHTED item on click. */
  const chooseMenuItem = (label: string) => {
    const item = menuItems().find((i) => i.textContent?.trim() === label)!;
    fireEvent.pointerDown(item);
    fireEvent.click(item);
  };

  it('starts a session in the project that was right-clicked', async () => {
    const onNewSession = vi.fn();
    const { getByTestId } = render(() => (
      <SessionTree
        sessions={sessions}
        projects={projects}
        {...baseProps}
        onNewSession={onNewSession}
      />
    ));

    fireEvent.contextMenu(getByTestId('session-group-/other'));
    await waitFor(() => expect(menuLabels()).toContain('New session here'));
    chooseMenuItem('New session here');

    await waitFor(() => expect(onNewSession).toHaveBeenCalledWith('/other'));
  });

  it('retargets the one menu at whichever project was right-clicked', async () => {
    const onSelectProject = vi.fn();
    const { getByTestId } = render(() => (
      <SessionTree
        sessions={sessions}
        projects={projects}
        {...baseProps}
        onSelectProject={onSelectProject}
      />
    ));

    // One hoisted trigger serves every row, so a stale target is the defect
    // hoisting can introduce: open on one project, then on another.
    fireEvent.contextMenu(getByTestId('session-group-/other'));
    await waitFor(() => expect(menuLabels()).toContain('Pin project'));
    fireEvent.keyDown(document.body, { key: 'Escape' });

    fireEvent.contextMenu(getByTestId('session-group-/repo'));
    await waitFor(() => expect(menuLabels()).toContain('Pin project'));
    chooseMenuItem('Pin project');

    await waitFor(() => expect(onSelectProject).toHaveBeenCalledWith('/repo'));
  });

  it('keeps Delete behind the context menu, off every row', async () => {
    const onDeleteSession = vi.fn();
    const { getByTestId } = render(() => (
      <SessionTree
        sessions={sessions}
        projects={projects}
        {...baseProps}
        onDeleteSession={onDeleteSession}
      />
    ));

    // It used to be a red button 4px from Archive on all 40 rows. Every file
    // explorer keeps a destructive tree operation behind the menu.
    const row = getByTestId('session-item-s-main');
    expect(row.querySelector('[title="Delete session"]')).toBeNull();

    fireEvent.contextMenu(row);
    await waitFor(() => expect(menuLabels()).toContain('Delete'));
    chooseMenuItem('Delete');
    await waitFor(() => expect(onDeleteSession).toHaveBeenCalledWith('s-main'));
  });

  it('offers session actions on a session row, project actions on a project row', async () => {
    const { getByTestId } = render(() => (
      <SessionTree sessions={sessions} projects={projects} {...baseProps} />
    ));

    fireEvent.contextMenu(getByTestId('session-item-s-main'));
    await waitFor(() => expect(menuLabels()).toContain('Archive'));
    expect(menuLabels()).not.toContain('Pin project');
    fireEvent.keyDown(document.body, { key: 'Escape' });

    fireEvent.contextMenu(getByTestId('session-group-/repo'));
    await waitFor(() => expect(menuLabels()).toContain('Pin project'));
    expect(menuLabels()).not.toContain('Archive');
  });

  /**
   * Did the event reach ark's trigger?
   *
   * Deterministic, unlike waiting a tick and hoping: the router vetoes by
   * calling `stopPropagation` in the CAPTURE phase on a wrapper above the
   * trigger, so a vetoed event never reaches a bubble-phase listener on the
   * trigger element itself. Timing plays no part — a `setTimeout(0)` proved
   * only that one macrotask had passed, and would have gone green for the
   * wrong reason the day ark moved its open onto a frame.
   */
  const reachesTrigger = (row: Element, init?: { shiftKey: boolean }): boolean => {
    const trigger = document.querySelector('[data-part="context-trigger"]')!;
    let reached = false;
    const spy = () => {
      reached = true;
    };
    trigger.addEventListener('contextmenu', spy);
    fireEvent.contextMenu(row, init);
    trigger.removeEventListener('contextmenu', spy);
    return reached;
  };

  it('leaves the project-less group to the browser menu', async () => {
    const { getByTestId } = render(() => (
      <SessionTree
        sessions={[...sessions, session({ id: 's-loose', workspace: '/kilns/main' })]}
        projects={projects}
        {...baseProps}
      />
    ));

    // Anchored to a positive, so a veto that stopped vetoing shows up here.
    expect(reachesTrigger(getByTestId('session-group-/repo'))).toBe(true);
    // Both project items act on a project path, and that bucket has none.
    expect(reachesTrigger(getByTestId('session-group-::none'))).toBe(false);
  });

  it('leaves Shift+right-click to the browser', async () => {
    const { getByTestId } = render(() => (
      <SessionTree sessions={sessions} projects={projects} {...baseProps} />
    ));

    const row = getByTestId('session-group-/repo');
    expect(reachesTrigger(row)).toBe(true);
    expect(reachesTrigger(row, { shiftKey: true })).toBe(false);
  });
});

describe('SessionTree — every registered project is listed', () => {
  const projects = [
    project('/repo', 'crucible'),
    project('/other', 'other'),
    project('/third', 'third'),
  ];
  const sessions = [session({ id: 's-main', workspace: '/repo' })];

  it('lists a project before it has a session: no chevron, no count, New Session on the row', () => {
    const { getByTestId, queryByTestId } = render(() => (
      <SessionTree sessions={sessions} projects={projects} {...baseProps} />
    ));
    expect(getByTestId('session-group-/repo')).toBeTruthy();
    for (const key of ['/other', '/third']) {
      const header = getByTestId(`session-group-${key}`);
      expect(header.querySelector('[data-testid="session-group-chevron"]')).toBeNull();
      expect(header.getAttribute('aria-expanded')).toBeNull();
      expect(header.textContent).not.toMatch(/\d/);
      expect(getByTestId(`session-group-new-${key}`)).toBeTruthy();
    }
    expect(queryByTestId('idle-projects-toggle')).toBeNull();
  });

  it('puts the projects with sessions first, then the rest by name', () => {
    const { container } = render(() => (
      <SessionTree sessions={sessions} projects={projects} {...baseProps} />
    ));
    const keys = [...container.querySelectorAll('[data-group-key]')].map((n) => n.getAttribute('data-group-key'));
    expect(keys).toEqual(['/repo', '/other', '/third']);
  });

  it('wraps the tier in a Projects section that stays on the rail with no project at all', () => {
    const { getByTestId } = render(() => <SessionTree sessions={[]} projects={[]} {...baseProps} />);
    const section = getByTestId('projects-section');
    expect(section.textContent).toContain('Projects');
    expect(section.textContent).not.toMatch(/\d/);
  });

  it('draws every section header at the row height, with no padding of its own', () => {
    const { getByTestId } = render(() => (
      <SessionTree sessions={sessions} projects={projects} inbox={sessions} {...baseProps} />
    ));
    for (const id of ['inbox-section', 'projects-section']) {
      const header = getByTestId(id);
      expect(header.className).toContain('h-(--cru-row-sm)');
      expect(header.className).not.toMatch(/\bp[tb]-\d/);
    }
  });

  it('keeps the project-less bucket in the main list', () => {
    const { getByTestId } = render(() => (
      <SessionTree
        sessions={[...sessions, session({ id: 's-loose', workspace: '/kilns/main' })]}
        projects={projects}
        {...baseProps}
      />
    ));
    // It HAS sessions — it is empty projects that hide, not homeless ones.
    expect(getByTestId('session-group-::none')).toBeTruthy();
  });
});

describe('SessionTree — the open session is never hidden', () => {
  const projects = [project('/repo', 'crucible'), project('/other', 'other')];
  const sessions = [
    session({ id: 's-main', workspace: '/repo' }),
    session({ id: 's-other', workspace: '/other' }),
  ];

  it('opens a collapsed group that holds the current session', () => {
    localStorage.setItem('crucible:sessionTree.collapsed', JSON.stringify(['/repo']));
    const { getByTestId } = render(() => (
      <SessionTree sessions={sessions} projects={projects} currentSessionId="s-main" {...baseProps} />
    ));
    // Selected from the palette, a session in a group collapsed last week
    // was on screen nowhere. The group that holds the open session opens.
    expect(getByTestId('session-item-s-main')).toBeTruthy();
  });

  it('opens the Other projects fold that holds the current session', () => {
    const { getByTestId } = render(() => (
      <SessionTree
        sessions={sessions}
        projects={projects}
        currentProjectPath="/repo"
        currentSessionId="s-other"
        {...baseProps}
      />
    ));
    expect(getByTestId('session-item-s-other')).toBeTruthy();
  });
});

describe('SessionTree — the Inbox above the project tier', () => {
  const projects = [project('/repo', 'crucible'), project('/other', 'other')];
  const inboxed = session({ id: 's-inbox', workspace: '/repo' });
  const onlyInbox = session({ id: 's-only-inbox', workspace: '/other' });
  const sessions = [
    inboxed,
    session({ id: 's-tree', workspace: '/repo', last_activity: '2026-07-21T10:00:00Z' }),
    onlyInbox,
  ];
  const draw = (over: Record<string, unknown> = {}) =>
    render(() => (
      <SessionTree
        sessions={sessions}
        projects={projects}
        inbox={[inboxed, onlyInbox]}
        {...baseProps}
        {...over}
      />
    ));

  it('draws an inbox row once, in the Inbox, with its project named', () => {
    const { getAllByTestId, getByTestId } = draw();
    expect(getAllByTestId('session-item-s-inbox')).toHaveLength(1);
    const section = getByTestId('inbox-section');
    // The row follows the section header; the tree's rows come after.
    expect(section.compareDocumentPosition(getByTestId('session-item-s-inbox')) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // Out of its group, so the project rides the row.
    expect(getByTestId('session-item-s-only-inbox').textContent).toContain('other');
    expect(getByTestId('session-item-s-tree')).toBeTruthy();
  });

  it('keeps a project whose every session is in the Inbox, without a chevron', () => {
    const { getByTestId, queryByTestId } = draw();
    // The project is real and New Session must stay reachable on its row,
    // but there is nothing under it to unfold, so no ">" says there is.
    const header = getByTestId('session-group-/other');
    expect(header.querySelector('[data-testid="session-group-chevron"]')).toBeNull();
    expect(header.getAttribute('aria-expanded')).toBeNull();
    expect(getByTestId('session-group-new-/other')).toBeTruthy();
    expect(queryByTestId('session-item-s-only-inbox')).toBeTruthy();
    // The busy project keeps its chevron: it has a row to fold.
    expect(getByTestId('session-group-/repo').querySelector('[data-testid="session-group-chevron"]')).toBeTruthy();
  });

  it('hides the project-less group once every session in it is in the Inbox', () => {
    const loose = session({ id: 's-loose', workspace: '/scratch/chat-1' });
    const { queryByText } = render(() => (
      <SessionTree sessions={[loose]} projects={projects} inbox={[loose]} {...baseProps} />
    ));
    expect(queryByText('Session folders')).toBeNull();
  });

  it('keeps the project-less group while a session is left under it to unfold', () => {
    const loose = session({ id: 's-loose', workspace: '/scratch/chat-1' });
    const { getByText } = render(() => (
      <SessionTree sessions={[loose]} projects={projects} inbox={[]} {...baseProps} />
    ));
    expect(getByText('Session folders')).toBeTruthy();
  });

  it('draws a project row as one leading slot and a name, with no folder icon', () => {
    const { getByTestId } = draw();
    expect(getByTestId('session-group-/repo').querySelectorAll('svg')).toHaveLength(1);
    expect(getByTestId('session-group-/other').querySelectorAll('svg')).toHaveLength(0);
  });

  it('centres the status dot in the same leading slot the chevron uses', () => {
    const { getByTestId } = draw();
    const slot = getByTestId('session-item-s-inbox').firstElementChild as HTMLElement;
    expect(slot.className).toContain('w-3.5');
    expect(slot.className).toContain('justify-center');
    expect(slot.querySelector('svg, span')).toBeTruthy();
  });

  it('counts only the rows the tier draws', () => {
    const { getByTestId } = draw();
    expect(getByTestId('session-group-/repo').textContent).toContain('1');
    expect(getByTestId('session-group-/other').textContent).not.toMatch(/\d/);
  });

  // The Inbox used to be drawn by the panel, outside the tree, so its rows
  // had no context menu — and with the newest sessions living only there,
  // Delete was unreachable for exactly the sessions in use.
  it('gives an inbox row the session context menu', async () => {
    const onDeleteSession = vi.fn();
    const { getByTestId } = draw({ onDeleteSession });
    fireEvent.contextMenu(getByTestId('session-item-s-inbox'));
    const item = await waitFor(() => {
      const el = document.querySelector<HTMLElement>(
        '[data-scope="menu"][data-part="content"][data-state="open"] [data-testid="session-group-menu-delete-session"]',
      );
      expect(el).toBeTruthy();
      return el!;
    });
    fireEvent.pointerDown(item);
    fireEvent.click(item);
    await waitFor(() => expect(onDeleteSession).toHaveBeenCalledWith('s-inbox'));
  });
});
