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

const project = (path: string, name: string): Project => ({
  path,
  name,
  kilns: [],
  last_accessed: '2026-01-01T00:00:00Z',
});

const session = (id: string, title: string, workspace: string | null): Session => ({
  id,
  session_type: 'chat',
  kilns: [],
  workspace,
  state: 'active',
  title,
  agent_model: null,
  agent_mode: null,
  // RECENT by default. The Inbox drops anything untouched for a day, so a
  // fixed date in the past silently emptied it — a fixture that ages out.
  started_at: new Date(Date.now() - 5 * 60_000).toISOString(),
  last_activity: new Date(Date.now() - 5 * 60_000).toISOString(),
  event_count: 0,
  archived: false,
});

describe('SessionsPanel — two tiers, project over session', () => {
  beforeEach(() => {
    localStorage.clear();
    projectList = [project('/home/me/crucible', 'crucible'), project('/home/me/atlas', 'atlas')];
    pinnedProject = projectList[0];
    sessionList = [
      session('s1', 'netcode-spike', '/home/me/crucible'),
      session('s2', 'atlas-migration', '/home/me/atlas'),
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
});

describe('SessionsPanel — the Inbox', () => {
  const ASK = { id: 'r1', kind: 'ask' as const, question: 'Proceed?' };
  const minutesAgo = (m: number) => new Date(Date.now() - m * 60_000).toISOString();

  beforeEach(() => {
    localStorage.clear();
    for (const id of ['s1', 's2', 's3']) attentionActions.clear(id);
    projectList = [project('/home/me/crucible', 'crucible')];
    pinnedProject = projectList[0];
    sessionList = [
      session('s1', 'netcode-spike', '/home/me/crucible'),
      session('s2', 'docs-pass', '/home/me/crucible'),
      { ...session('s3', 'ancient', '/home/me/crucible'), last_activity: minutesAgo(60 * 30) },
    ];
  });

  it('lists what is doing something, above the tree', () => {
    attentionActions.report('s1', { pendingInteraction: ASK });
    const { container } = render(() => <SessionsPanel />);

    const section = screen.getByTestId('inbox-section');
    expect(section.textContent).toContain('Inbox');
    // Above the project tree — it is what you came to look at.
    const order = [...container.querySelectorAll('[data-testid="inbox-section"], [data-testid^="session-group-"]')];
    expect(order[0]).toBe(section);
  });

  it('drops a session whose last message is over a day old', () => {
    // The staleness rule is what keeps this an inbox rather than a second
    // session list: an agent blocked since last week is not news, and would
    // otherwise sit at the top of the rail forever.
    attentionActions.report('s3', { pendingInteraction: ASK });
    render(() => <SessionsPanel />);
    expect(screen.queryByTestId('inbox-section')).toBeNull();
  });

  it('offers no Inbox when nothing is doing anything', () => {
    render(() => <SessionsPanel />);
    expect(screen.queryByTestId('inbox-section')).toBeNull();
  });

  it('keeps an inbox session in the tree below as well', () => {
    attentionActions.report('s1', { pendingInteraction: ASK });
    render(() => <SessionsPanel />);
    // It leaves the Inbox when it goes stale; the tree is where it lives.
    expect(screen.getByTestId('session-group-/home/me/crucible')).toBeTruthy();
  });
});

describe('SessionsPanel — one section vocabulary', () => {
  it('gives Inbox, No sessions and Archived the same header shape', () => {
    localStorage.clear();
    attentionActions.report('s1', { pendingInteraction: { id: 'r', kind: 'ask', question: 'q' } });
    projectList = [project('/home/me/crucible', 'crucible'), project('/home/me/quiet', 'quiet')];
    pinnedProject = projectList[0];
    sessionList = [
      session('s1', 'netcode-spike', '/home/me/crucible'),
      { ...session('s9', 'old', '/home/me/crucible'), archived: true },
    ];
    render(() => <SessionsPanel />);

    // Three sections of one list that used to have three paddings, two count
    // placements and two chevron treatments.
    const classes = ['inbox-section', 'idle-projects-toggle', 'archived-section'].map(
      (id) => screen.getByTestId(id).className,
    );
    expect(new Set(classes).size).toBe(1);
    attentionActions.clear('s1');
  });
});

describe('SessionsPanel — the tree is scoped to the pinned project', () => {
  beforeEach(() => {
    localStorage.clear();
    projectList = [project('/home/me/crucible', 'crucible'), project('/home/me/atlas', 'atlas')];
    pinnedProject = projectList[0];
    sessionList = [
      session('s1', 'netcode-spike', '/home/me/crucible'),
      session('s2', 'atlas-migration', '/home/me/atlas'),
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
