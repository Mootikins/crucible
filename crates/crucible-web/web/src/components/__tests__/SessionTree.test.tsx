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

/** Unfold the "No sessions" section — projects with nothing running. */
const openIdleProjects = (get: (id: string) => HTMLElement) =>
  fireEvent.click(get('idle-projects-toggle'));

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

    // ONE group for the repo (main + worktree), one empty 'other', one session-folders bucket.
    const repoGroup = getByTestId('session-group-/repo');
    expect(repoGroup).toBeTruthy();
    expect(repoGroup.textContent).toContain('2');
    openIdleProjects(getByTestId);
    expect(getByTestId('session-group-/other')).toBeTruthy();
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
    const list = getByTestId('session-list');
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

    openIdleProjects(getByTestId);
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
    const { getByTestId } = render(() => (
      <SessionTree sessions={[]} projects={dupes} {...baseProps} onNewSession={onNewSession} />
    ));

    openIdleProjects(getByTestId);
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
  const sessions = [session({ id: 's-main', workspace: '/repo' })];

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

    openIdleProjects(getByTestId);
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
    openIdleProjects(getByTestId);
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

describe('SessionTree — quiet projects fold away', () => {
  const projects = [
    project('/repo', 'crucible'),
    project('/other', 'other'),
    project('/third', 'third'),
  ];
  const sessions = [session({ id: 's-main', workspace: '/repo' })];

  it('hides projects with nothing running, and counts them', () => {
    const { getByTestId, queryByTestId } = render(() => (
      <SessionTree sessions={sessions} projects={projects} {...baseProps} />
    ));

    // A registry of twenty projects is mostly ones you are not working in
    // today, and each cost a row that pushed the busy ones off the screen.
    expect(getByTestId('session-group-/repo')).toBeTruthy();
    expect(queryByTestId('session-group-/other')).toBeNull();
    expect(getByTestId('idle-projects-toggle').textContent).toContain('2');
  });

  it('opens them on demand, New Session and all', () => {
    const onNewSession = vi.fn();
    const { getByTestId } = render(() => (
      <SessionTree
        sessions={sessions}
        projects={projects}
        {...baseProps}
        onNewSession={onNewSession}
      />
    ));

    openIdleProjects(getByTestId);
    // Starting work in a quiet project is exactly what the fold must not
    // block, so a folded project is the SAME row as a busy one.
    fireEvent.click(getByTestId('session-group-new-/third'));
    expect(onNewSession).toHaveBeenCalledWith('/third');
  });

  it('offers no fold when every project is busy', () => {
    const { queryByTestId } = render(() => (
      <SessionTree
        sessions={[session({ id: 's-main', workspace: '/repo' })]}
        projects={[project('/repo', 'crucible')]}
        {...baseProps}
      />
    ));
    // A control that does nothing must not take a row.
    expect(queryByTestId('idle-projects-toggle')).toBeNull();
  });

  it('keeps the project-less bucket in the main list, not the fold', () => {
    const { getByTestId } = render(() => (
      <SessionTree
        sessions={[...sessions, session({ id: 's-loose', workspace: '/kilns/main' })]}
        projects={projects}
        {...baseProps}
      />
    ));
    // It HAS sessions — it is quiet projects that fold, not homeless ones.
    expect(getByTestId('session-group-::none')).toBeTruthy();
  });
});
