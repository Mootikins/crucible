import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent, screen } from '@solidjs/testing-library';
import type { Project, Session } from '@/lib/types';

let sessionList: Session[] = [];
let projectList: Project[] = [];
// The pin is INDEPENDENT of the roster: a shell can have projects and no
// pinned one, which is the state the tree must not scope itself into nothing.
let pinnedProject: Project | null = null;

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  listWorkspaceTargets: async () => [],
}));

vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: () => null,
    sessions: () => sessionList,
    selectSession: async () => {},
    archiveSession: async () => {},
    deleteSession: async () => {},
    refreshSessions: async () => {},
  }),
}));

vi.mock('@/contexts/ProjectContext', () => ({
  useProjectSafe: () => ({
    projects: () => projectList,
    currentProject: () => pinnedProject,
    selectProject: async () => {},
  }),
}));

import { SessionsPanel } from '../SessionsPanel';
import { attentionActions } from '@/stores/attentionStore';
import { INBOX_SIZE } from '@/lib/session-inbox';

const project = (path: string, name: string): Project => ({
  path,
  name,
  kilns: [],
  last_accessed: '2026-01-01T00:00:00Z',
});

const minutesAgo = (m: number) => new Date(Date.now() - m * 60_000).toISOString();

const session = (id: string, title: string, workspace: string | null, ageMinutes = 5): Session => ({
  id,
  session_type: 'chat',
  kilns: [],
  workspace,
  state: 'active',
  title,
  agent_model: null,
  agent_mode: null,
  started_at: minutesAgo(ageMinutes),
  last_activity: minutesAgo(ageMinutes),
  event_count: 0,
  archived: false,
});

/**
 * Enough recent sessions to fill the Inbox, so that what comes after them
 * lands in the tree. The Inbox takes the newest `INBOX_SIZE`; these are
 * newer than anything a test adds with the default age.
 */
const inboxFillers = (workspace: string): Session[] =>
  Array.from({ length: INBOX_SIZE }, (_, i) => session(`f${i}`, `filler-${i}`, workspace, i + 1));

describe('SessionsPanel — two tiers, project over session', () => {
  beforeEach(() => {
    localStorage.clear();
    projectList = [project('/home/me/crucible', 'crucible'), project('/home/me/atlas', 'atlas')];
    pinnedProject = projectList[0];
    sessionList = [
      ...inboxFillers('/home/me/crucible'),
      session('s1', 'netcode-spike', '/home/me/crucible', 60),
      session('s2', 'atlas-migration', '/home/me/atlas', 90),
    ];
  });

  it('puts each session under its own project group', () => {
    render(() => <SessionsPanel />);
    // The panel used to render a flat recency list and leave the grouping to a
    // SessionTree nothing mounted.
    expect(screen.getByTestId('session-group-/home/me/crucible')).toBeTruthy();
    expect(screen.getByTestId('session-item-s1')).toBeTruthy();
    // atlas is not the pinned project, so it sits behind the counted fold.
    expect(screen.queryByTestId('session-group-/home/me/atlas')).toBeNull();
    fireEvent.click(screen.getByTestId('idle-projects-toggle'));
    expect(screen.getByTestId('session-group-/home/me/atlas')).toBeTruthy();
  });

  it('collapses a project without touching its neighbour', () => {
    render(() => <SessionsPanel />);
    fireEvent.click(screen.getByTestId('idle-projects-toggle'));
    fireEvent.click(screen.getByTestId('session-group-/home/me/crucible'));
    expect(screen.queryByTestId('session-item-s1')).toBeNull();
    expect(screen.queryByTestId('session-item-s2')).toBeTruthy();
  });

  it('drops the panel-wide New Session button', () => {
    render(() => <SessionsPanel />);
    // It could not name the project it meant. The ribbon keeps a
    // project-agnostic entry point for when you have none in mind.
    expect(screen.queryByTestId('new-session-button')).toBeNull();
  });

  it('starts a session in the project whose row was clicked', () => {
    const started: unknown[] = [];
    const listener = (e: Event) => started.push((e as CustomEvent).detail);
    window.addEventListener('crucible:new-session', listener);

    render(() => <SessionsPanel />);
    fireEvent.click(screen.getByTestId('idle-projects-toggle'));
    fireEvent.click(screen.getByTestId('session-group-new-/home/me/atlas'));

    window.removeEventListener('crucible:new-session', listener);
    expect(started).toEqual([{ workspace: '/home/me/atlas' }]);
  });

  it('hides a detected project until a session starts in it', () => {
    pinnedProject = null;
    projectList = [...projectList, project('/home/me/quiet', 'quiet')];
    render(() => <SessionsPanel />);
    // No row, no chevron, no fold: a directory the user never worked in is
    // not a place on the rail yet.
    expect(screen.queryByTestId('session-group-/home/me/quiet')).toBeNull();
    expect(screen.queryByTestId('idle-projects-toggle')).toBeNull();
  });
});

describe('SessionsPanel — nothing yet', () => {
  beforeEach(() => {
    localStorage.clear();
    projectList = [];
    pinnedProject = null;
    sessionList = [];
  });

  it('offers a first session from the empty state', () => {
    const started: unknown[] = [];
    const listener = (e: Event) => started.push((e as CustomEvent).detail);
    window.addEventListener('crucible:new-session', listener);

    render(() => <SessionsPanel />);
    const empty = screen.getByTestId('sessions-empty');
    expect(empty.textContent).toContain('No sessions yet');
    expect(empty.getAttribute('data-tone')).toBe('empty');

    fireEvent.click(empty.querySelector('[data-testid="empty-state-action"]')!);
    window.removeEventListener('crucible:new-session', listener);
    // No workspace: there is no project to name, so the draft asks for one.
    expect(started).toEqual([null]);
  });

  it('shows the empty state, not an empty Inbox, when only projects exist', () => {
    projectList = [project('/home/me/crucible', 'crucible')];
    render(() => <SessionsPanel />);
    expect(screen.getByTestId('sessions-empty')).toBeTruthy();
    expect(screen.queryByTestId('inbox-section')).toBeNull();
  });
});

describe('SessionsPanel — the Inbox', () => {
  const ASK = { id: 'r1', kind: 'ask' as const, question: 'Proceed?' };

  beforeEach(() => {
    localStorage.clear();
    for (const id of ['s1', 's2', 's3']) attentionActions.clear(id);
    projectList = [project('/home/me/crucible', 'crucible'), project('/home/me/atlas', 'atlas')];
    pinnedProject = null;
    sessionList = [
      session('s1', 'netcode-spike', '/home/me/crucible', 5),
      session('s2', 'atlas-migration', '/home/me/atlas', 10),
      session('s3', 'ancient', '/home/me/crucible', 60 * 30),
    ];
  });

  it('lists the last few sessions, above the tree, whatever they are doing', () => {
    const { container } = render(() => <SessionsPanel />);

    const section = screen.getByTestId('inbox-section');
    expect(section.textContent).toContain('Inbox');
    // Nothing here is working or waiting. The Inbox is "where was I", not a
    // status filter, so an idle session still has its place.
    expect(section.textContent).toContain('3');
    // Above the project tree.
    const order = [...container.querySelectorAll('[data-testid="inbox-section"], [data-testid^="session-group-"]')];
    expect(order[0]).toBe(section);
  });

  it('draws a session once: in the Inbox, not again in the tree', () => {
    render(() => <SessionsPanel />);
    expect(screen.getAllByTestId('session-item-s1')).toHaveLength(1);
    // Its project stays, because New Session lives there — but with nothing
    // to unfold, so no chevron.
    const header = screen.getByTestId('session-group-/home/me/atlas');
    expect(header.querySelector('[data-testid="session-group-chevron"]')).toBeNull();
    expect(screen.getByTestId('session-group-new-/home/me/atlas')).toBeTruthy();
  });

  it('stops at the newest few and leaves the rest to the tree', () => {
    sessionList = [...inboxFillers('/home/me/crucible'), ...sessionList];
    render(() => <SessionsPanel />);
    expect(screen.getByTestId('inbox-section').textContent).toContain(String(INBOX_SIZE));
    // The fillers are newer than s1, so s1 and its siblings are tree rows,
    // under their project header; a filler sits above it, in the Inbox, once.
    const header = screen.getByTestId('session-group-/home/me/crucible');
    const after = (id: string) =>
      !!(header.compareDocumentPosition(screen.getByTestId(id)) & Node.DOCUMENT_POSITION_FOLLOWING);
    expect(after('session-item-s1')).toBe(true);
    expect(after('session-item-s3')).toBe(true);
    expect(after('session-item-f0')).toBe(false);
    expect(screen.getAllByTestId('session-item-f0')).toHaveLength(1);
    expect(header.textContent).toContain('2');
  });

  it('names the project on an inbox row, and does not indent it', () => {
    render(() => <SessionsPanel />);
    // An inbox row is out of its project's group. That is exactly when the
    // project must ride the row; a tree row has the header above it.
    const inboxRow = screen.getByTestId('inbox-section').parentElement!.querySelector(
      '[data-testid="session-item-s2"]',
    )!;
    expect(inboxRow.textContent).toContain('atlas');
    expect(inboxRow.className).not.toMatch(/\bpl-6\b/);
  });

  it('counts what is waiting on you in the accent', () => {
    attentionActions.report('s1', { pendingInteraction: ASK });
    render(() => <SessionsPanel />);
    const count = screen.getByTestId('inbox-section').querySelector('.tabular-nums')!;
    expect(count.className).toContain('text-attention');
  });
});

describe('SessionsPanel — one section vocabulary', () => {
  it('gives Inbox, Other projects and Archived the same header shape', () => {
    localStorage.clear();
    projectList = [project('/home/me/crucible', 'crucible'), project('/home/me/atlas', 'atlas')];
    pinnedProject = projectList[0];
    sessionList = [
      session('s1', 'netcode-spike', '/home/me/crucible'),
      session('s2', 'atlas-migration', '/home/me/atlas', 30),
      { ...session('s9', 'old', '/home/me/crucible'), archived: true },
    ];
    render(() => <SessionsPanel />);

    // Three sections of one list that used to have three paddings, two count
    // placements and two chevron treatments.
    const classes = ['inbox-section', 'idle-projects-toggle', 'archived-section'].map(
      (id) => screen.getByTestId(id).className,
    );
    expect(new Set(classes).size).toBe(1);
  });
});

describe('SessionsPanel — the tree is scoped to the pinned project', () => {
  beforeEach(() => {
    localStorage.clear();
    projectList = [project('/home/me/crucible', 'crucible'), project('/home/me/atlas', 'atlas')];
    pinnedProject = projectList[0];
    sessionList = [
      ...inboxFillers('/home/me/crucible'),
      session('s1', 'netcode-spike', '/home/me/crucible', 60),
      session('s2', 'atlas-migration', '/home/me/atlas', 90),
    ];
  });

  it('states how much it is hiding, and shows it on one click', () => {
    render(() => <SessionsPanel />);
    const fold = screen.getByTestId('idle-projects-toggle');
    // A filter you cannot see is worse than the rows it saves, so the count
    // says how much is behind it.
    expect(fold.textContent).toContain('Other projects');
    expect(fold.textContent).toContain('1');

    fireEvent.click(fold);
    expect(screen.getByTestId('session-item-s2')).toBeTruthy();
  });

  it('shows everything when no project is pinned', () => {
    // Scoping to a pin that matches nothing would empty the rail — a dead end
    // on the screen a new user starts from.
    pinnedProject = null;
    render(() => <SessionsPanel />);
    expect(screen.getByTestId('session-group-/home/me/crucible')).toBeTruthy();
    expect(screen.getByTestId('session-group-/home/me/atlas')).toBeTruthy();
    expect(screen.queryByTestId('idle-projects-toggle')).toBeNull();
  });

  it('keeps a folded project fully usable', () => {
    const started: unknown[] = [];
    const listener = (e: Event) => started.push((e as CustomEvent).detail);
    window.addEventListener('crucible:new-session', listener);

    render(() => <SessionsPanel />);
    fireEvent.click(screen.getByTestId('idle-projects-toggle'));
    fireEvent.click(screen.getByTestId('session-group-new-/home/me/atlas'));

    window.removeEventListener('crucible:new-session', listener);
    // A folded project is the SAME header row as a pinned one.
    expect(started).toEqual([{ workspace: '/home/me/atlas' }]);
  });
});

describe('SessionsPanel — Reflections', () => {
  beforeEach(() => {
    localStorage.clear();
    projectList = [project('/home/me/crucible', 'crucible')];
    pinnedProject = projectList[0];
    sessionList = [
      session('s1', 'netcode-spike', '/home/me/crucible'),
      { ...session('p1', 'Reflection: yesterday', null), session_type: 'plugin' },
    ];
  });

  // A pass has no workspace, so the project tree files it under "No project"
  // beside every other workspace-less session. The daemon now holds it out of
  // the archive while its hunks are undecided, which is only useful if the
  // user can find it.
  it('lists a plugin session under its own section', () => {
    // No pinned project, so the tree below is the one that could hold a
    // workspace-less pass. Pinned to a project the tree is scoped to it and
    // the pass is absent whether or not the section claims it, which would
    // make the "listed once" assertion gate nothing.
    pinnedProject = null;
    render(() => <SessionsPanel />);
    const section = screen.getByTestId('reflections-section');
    expect(section.textContent).toContain('Reflections');
    expect(section.textContent).toContain('1');
    // Folded like Archived, and the count is what makes it discoverable.
    expect(screen.queryByTestId('session-item-p1')).toBeNull();
    fireEvent.click(section);
    expect(screen.getByTestId('session-item-p1')).toBeTruthy();
    // And the tree below leaves it to this section, so it is listed once.
    expect(screen.getAllByText('Reflection: yesterday')).toHaveLength(1);
  });

  it('offers no section when no plugin session is listed', () => {
    sessionList = [session('s1', 'netcode-spike', '/home/me/crucible')];
    render(() => <SessionsPanel />);
    // A control that does nothing must not take a row.
    expect(screen.queryByTestId('reflections-section')).toBeNull();
  });
});
