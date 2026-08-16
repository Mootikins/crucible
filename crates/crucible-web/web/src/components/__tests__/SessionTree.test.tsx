import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent } from '@solidjs/testing-library';
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

    // ONE group for the repo (main + worktree), one empty 'other', one session-folders bucket.
    const repoGroup = getByTestId('session-group-crucible');
    expect(repoGroup).toBeTruthy();
    expect(repoGroup.textContent).toContain('2');
    expect(getByTestId('session-group-other')).toBeTruthy();
    expect(getByTestId('session-group-Session folders')).toBeTruthy();
    expect(queryByTestId('session-group-x')).toBeNull(); // worktree never a group

    // All three sessions render as rows.
    expect(getByTestId('session-item-s-main')).toBeTruthy();
    expect(getByTestId('session-item-s-wt')).toBeTruthy();
    expect(getByTestId('session-item-s-none')).toBeTruthy();

    // Branch + kiln chips on the worktree session's row.
    const wtRow = getByTestId('session-item-s-wt');
    expect(wtRow.textContent).toContain('feat/x');
    expect(wtRow.textContent).toContain('main');
  });

  it('collapsing a group hides its rows and persists', () => {
    const projects = [project('/repo', 'crucible', { root: '/repo', is_worktree: false })];
    const sessions = [session({ id: 's1', workspace: '/repo' })];
    const { getByTestId, queryByTestId } = render(() => (
      <SessionTree sessions={sessions} projects={projects} {...baseProps} />
    ));

    fireEvent.click(getByTestId('session-group-crucible'));
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

  it('clicking a group name selects the project; the row toggles collapse', () => {
    const projects = [project('/repo', 'crucible', { root: '/repo', is_worktree: false })];
    const { getByTestId, getByText } = render(() => (
      <SessionTree sessions={[]} projects={projects} {...baseProps} />
    ));
    fireEvent.click(getByText('crucible'));
    expect(baseProps.onSelectProject).toHaveBeenCalledWith('/repo');
    // Group is still expanded (name click doesn't toggle).
    expect(getByTestId('session-group-crucible').getAttribute('aria-expanded')).toBe('true');
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
