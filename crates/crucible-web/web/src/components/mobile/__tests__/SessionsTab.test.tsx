import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import type { Project, Session } from '@/lib/types';

const state = vi.hoisted(() => ({
  sessions: [] as Session[],
  projects: [] as Project[],
  currentProject: null as Project | null,
  selected: [] as string[],
  waiting: [] as string[],
}));

vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    sessions: () => state.sessions,
    currentSession: () => null,
    selectSession: (id: string) => state.selected.push(id),
    archiveSession: vi.fn(),
    deleteSession: vi.fn(),
    refreshSessions: vi.fn(),
  }),
}));
vi.mock('@/contexts/ProjectContext', () => ({
  useProjectSafe: () => ({
    projects: () => state.projects,
    currentProject: () => state.currentProject,
    selectProject: (path: string) => {
      state.currentProject = state.projects.find((p) => p.path === path) ?? null;
    },
  }),
}));
vi.mock('@/lib/session-status', () => ({
  sessionStatus: (s: Session) => (state.waiting.includes(s.id) ? 'waiting' : 'idle'),
  STATUS_RANK: { waiting: 0, working: 1, idle: 2 },
}));

import { SessionsTab } from '@/components/mobile/SessionsTab';
import { INBOX_SIZE } from '@/lib/session-inbox';

const minutesAgo = (m: number) => new Date(Date.now() - m * 60_000).toISOString();

const session = (id: string, workspace: string, title: string, ageMinutes = 60): Session =>
  ({
    id,
    title,
    started_at: minutesAgo(ageMinutes),
    last_activity: minutesAgo(ageMinutes),
    archived: false,
    kilns: [],
    metadata: { workspace },
    workspace,
  }) as unknown as Session;

/**
 * Enough recent sessions to fill the Inbox, so that the sessions a test names
 * land in the project list below it. Titles no assertion looks for.
 */
const inboxFillers = (workspace: string): Session[] =>
  Array.from({ length: INBOX_SIZE }, (_, i) => session(`f${i}`, workspace, `filler ${i}`, i + 1));

beforeEach(() => {
  state.projects = [
    { path: '/work/alpha', name: 'alpha', kilns: [] } as unknown as Project,
    { path: '/work/beta', name: 'beta', kilns: [] } as unknown as Project,
  ];
  state.currentProject = state.projects[0];
  state.sessions = [
    ...inboxFillers('/work/alpha'),
    session('a1', '/work/alpha', 'Alpha one'),
    session('b1', '/work/beta', 'Beta one'),
  ];
  state.selected = [];
  state.waiting = [];
});

describe('SessionsTab', () => {
  it('lists only the chosen project’s sessions', () => {
    render(() => <SessionsTab />);
    expect(screen.queryByText('Alpha one')).toBeTruthy();
    expect(screen.queryByText('Beta one')).toBeNull();
  });

  // The whole point of the switcher: a recency list cannot reach the rest.
  it('switches projects, and shows that project’s sessions', () => {
    render(() => <SessionsTab />);
    fireEvent.click(screen.getByRole('button', { name: /Project: alpha/ }));
    fireEvent.click(screen.getByRole('button', { name: 'beta' }));
    expect(screen.queryByText('Beta one')).toBeTruthy();
    expect(screen.queryByText('Alpha one')).toBeNull();
  });

  it('shows every project at once when asked', () => {
    render(() => <SessionsTab />);
    fireEvent.click(screen.getByRole('button', { name: /Project: alpha/ }));
    fireEvent.click(screen.getByRole('button', { name: 'All projects' }));
    expect(screen.queryByText('Alpha one')).toBeTruthy();
    expect(screen.queryByText('Beta one')).toBeTruthy();
  });

  // The session the user touched last matters whatever project is on screen.
  it('keeps the Inbox across projects', () => {
    state.sessions = [...state.sessions, session('b2', '/work/beta', 'Beta two', 0)];
    render(() => <SessionsTab />);
    const inbox = screen.getByTestId('compact-inbox');
    expect(inbox.textContent).toContain('Beta two');
  });

  it('draws a session once: in the Inbox, not again in the project list', () => {
    render(() => <SessionsTab />);
    expect(screen.getAllByText('filler 0')).toHaveLength(1);
    expect(screen.getByTestId('compact-inbox').textContent).toContain('filler 0');
    // alpha is not empty — its newest rows are above — so no empty text.
    expect(screen.queryByText('No sessions here yet.')).toBeNull();
  });

  /**
   * The phone list and the phone file tree take their metrics from the same
   * attribute. A row must not carry a `text-*` class of its own: such a class
   * pins the size and the attribute stops reaching the row.
   */
  it('asks for touch metrics and lets the rows inherit them', () => {
    const { container } = render(() => <SessionsTab />);
    const wrapper = container.querySelector('[data-density="touch"]');
    expect(wrapper).not.toBeNull();
    const row = container.querySelector('[data-session-id="a1"]')!;
    expect(row.classList.contains('tree-row')).toBe(true);
    expect(wrapper!.contains(row)).toBe(true);
    expect(row.querySelector('.text-reading')).toBeNull();
  });

  it('opens a session when a row is tapped', () => {
    render(() => <SessionsTab />);
    fireEvent.click(screen.getByText('Alpha one'));
    expect(state.selected).toEqual(['a1']);
  });

  it('starts a new session aimed at the chosen project', () => {
    const events: unknown[] = [];
    window.addEventListener('crucible:new-session', (e) => events.push((e as CustomEvent).detail));
    render(() => <SessionsTab />);
    fireEvent.click(screen.getByRole('button', { name: 'New session in alpha' }));
    expect(events).toEqual([{ workspace: '/work/alpha' }]);
  });

  // A pass a plugin ran has no workspace, so the project switcher can never
  // reach it: it is not in any project's list, and under "All projects" it
  // sits among every ordinary session. The daemon holds it out of the archive
  // while its review queue is undecided, and this is where a phone finds it.
  it('lists a plugin session under Reflections, whatever project is chosen', () => {
    // "All projects": the one choice under which the list below could hold a
    // workspace-less pass at all. Scoped to a project it is absent whether or
    // not the section filters it out, and the de-dup assertion would gate
    // nothing.
    state.currentProject = null;
    state.sessions = [
      ...state.sessions,
      { ...session('p1', '', 'Reflection: yesterday'), session_type: 'plugin', workspace: null },
    ];
    render(() => <SessionsTab />);
    const section = screen.getByTestId('compact-reflections');
    expect(section.textContent).toContain('Reflection: yesterday');
    // Not repeated in the project list below.
    expect(screen.getAllByText('Reflection: yesterday')).toHaveLength(1);
  });

  it('offers no Reflections section when no plugin session is listed', () => {
    render(() => <SessionsTab />);
    expect(screen.queryByTestId('compact-reflections')).toBeNull();
  });
});
