import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createMemo, createSignal } from 'solid-js';
import { render, fireEvent, screen, waitFor } from '@solidjs/testing-library';
import { RootDropdown } from '../RootDropdown';
import { buildRoster, rootKey, rosterIndex, type TreeRoot } from '@/lib/tree-root';
import type { KilnListEntry, Project } from '@/lib/types';

vi.mock('@/lib/api', () => ({
  scmBranches: vi.fn(),
  scmWorktreeAdd: vi.fn(),
  scmClone: vi.fn(),
  registerProject: vi.fn(),
  isGitRepoUrl: (s: string) => /^(https?:\/\/|git@)/.test(s) || /^[\w.-]+\/[\w.-]+$/.test(s),
  isBranchNameish: (s: string) => !!s && !/\s|\.\.|^[-/]|\\|:|@\{|\/$/.test(s),
}));

import { scmBranches, scmWorktreeAdd, registerProject } from '@/lib/api';

const project = (path: string, name: string, kilns: Project['kilns'] = []): Project => ({
  path,
  name,
  kilns,
  last_accessed: '',
});

const openPopout = (getByTestId: (id: string) => HTMLElement) => {
  fireEvent.click(getByTestId('root-dropdown'));
};

beforeEach(() => {
  vi.mocked(scmBranches).mockRejectedValue(new Error('no repo'));
  vi.mocked(scmWorktreeAdd).mockReset();
  vi.mocked(registerProject).mockReset();
});

describe('RootDropdown', () => {
  it('renders grouped section headers with the expected option counts', () => {
    const groups = buildRoster(
      [project('/p1', 'P1'), project('/p2', 'P2')],
      [{ path: '/vault', name: 'Vault' }],
    );
    const { getByTestId } = render(() => (
      <RootDropdown groups={groups} selectedKey={null} onSelect={() => {}} />
    ));
    openPopout(getByTestId);
    const popout = screen.getByTestId('root-dropdown-popout');
    const rows = popout.querySelectorAll('[role="option"]');
    expect(rows).toHaveLength(3);
    expect(popout.textContent).toContain('Projects');
    expect(popout.textContent).toContain('Kilns');
    expect(popout.textContent).not.toContain('Worktrees'); // empty group omitted
  });

  // Regression (of the original select-reset bug, now structural): the
  // trigger label must track the RESOLVED root across async roster arrival.
  it('trigger label follows the resolved root across async roster arrival', async () => {
    const [projects, setProjects] = createSignal<Project[]>([]);
    const [kilns, setKilns] = createSignal<KilnListEntry[]>([]);
    const persisted = 'kiln:/vault';
    const roster = createMemo(() => buildRoster(projects(), kilns()));
    const activeKey = createMemo(() => {
      const idx = rosterIndex(roster());
      if (idx.has(persisted)) return persisted;
      const first = roster().find((g) => g.roots.length > 0)?.roots[0];
      return first ? rootKey(first) : null;
    });
    const { getByTestId } = render(() => (
      <RootDropdown groups={roster()} selectedKey={activeKey()} onSelect={() => {}} />
    ));

    setProjects([project('/p1', 'crucible')]);
    await Promise.resolve();
    expect(getByTestId('root-dropdown').textContent).toContain('crucible');

    setKilns([{ path: '/vault', name: 'docs' }]);
    await Promise.resolve();
    expect(getByTestId('root-dropdown').textContent).toContain('docs');

    setProjects([project('/p1', 'crucible'), project('/p2', 'other')]);
    await Promise.resolve();
    expect(getByTestId('root-dropdown').textContent).toContain('docs');
  });

  it('calls onSelect with the resolved TreeRoot when an option is picked', () => {
    const groups = buildRoster([project('/p1', 'P1')], [{ path: '/vault', name: 'Vault' }]);
    const onSelect = vi.fn<(r: TreeRoot) => void>();
    const { getByTestId } = render(() => (
      <RootDropdown groups={groups} selectedKey={null} onSelect={onSelect} />
    ));
    openPopout(getByTestId);
    fireEvent.click(screen.getByText('Vault'));
    expect(onSelect).toHaveBeenCalledWith({ kind: 'kiln', path: '/vault', name: 'Vault' });
  });

  it('shows a "No roots" fallback and no trigger for an empty roster', () => {
    const groups = buildRoster([], []);
    const { container, queryByTestId } = render(() => (
      <RootDropdown groups={groups} selectedKey={null} onSelect={() => {}} />
    ));
    expect(queryByTestId('root-dropdown')).toBeNull();
    expect(container.textContent).toContain('No roots');
  });

  it('lists repo branches for an active project root and jumps to a worktree', async () => {
    vi.mocked(scmBranches).mockResolvedValue({
      repo_root: '/repo',
      current_branch: 'master',
      branches: [
        { name: 'master', worktree_path: '/repo', is_current: true, remote_only: false },
        {
          name: 'feat/x',
          worktree_path: '/repo/tree/feat/x',
          is_current: false,
          remote_only: false,
        },
      ],
    });
    vi.mocked(registerProject).mockResolvedValue(project('/repo/tree/feat/x', 'x'));
    const onSelect = vi.fn<(r: TreeRoot) => void>();
    const groups = buildRoster([project('/repo', 'repo')], []);
    const { getByTestId } = render(() => (
      <RootDropdown
        groups={groups}
        selectedKey="project:/repo"
        onSelect={onSelect}
        activeRoot={{ kind: 'project', path: '/repo', name: 'repo' }}
      />
    ));
    openPopout(getByTestId);
    await waitFor(() => expect(screen.getByTestId('root-dropdown-popout').textContent).toContain('Branches — repo'));

    fireEvent.click(screen.getByText('feat/x'));
    await waitFor(() =>
      expect(onSelect).toHaveBeenCalledWith({
        kind: 'project',
        path: '/repo/tree/feat/x',
        name: 'x',
      }),
    );
    expect(registerProject).toHaveBeenCalledWith('/repo/tree/feat/x');
    expect(scmWorktreeAdd).not.toHaveBeenCalled();
  });

  it('offers to create a worktree for a branch without one', async () => {
    vi.mocked(scmBranches).mockResolvedValue({
      repo_root: '/repo',
      current_branch: 'master',
      branches: [
        { name: 'fix/y', worktree_path: null, is_current: false, remote_only: false },
      ],
    });
    vi.mocked(scmWorktreeAdd).mockResolvedValue({
      path: '/repo/tree/fix/y',
      project: project('/repo/tree/fix/y', 'y'),
      warning: null,
    });
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    const onSelect = vi.fn<(r: TreeRoot) => void>();
    const groups = buildRoster([project('/repo', 'repo')], []);
    const { getByTestId } = render(() => (
      <RootDropdown
        groups={groups}
        selectedKey="project:/repo"
        onSelect={onSelect}
        activeRoot={{ kind: 'project', path: '/repo', name: 'repo' }}
      />
    ));
    openPopout(getByTestId);
    await waitFor(() => expect(screen.getByText('fix/y')).toBeTruthy());

    fireEvent.click(screen.getByText('fix/y'));
    await waitFor(() => expect(scmWorktreeAdd).toHaveBeenCalledWith('/repo', 'fix/y', false));
    await waitFor(() =>
      expect(onSelect).toHaveBeenCalledWith({
        kind: 'project',
        path: '/repo/tree/fix/y',
        name: 'y',
      }),
    );
    confirmSpy.mockRestore();
  });

  it('typing an unknown name offers branch-plus-worktree creation', async () => {
    vi.mocked(scmBranches).mockResolvedValue({
      repo_root: '/repo',
      current_branch: 'master',
      branches: [
        { name: 'master', worktree_path: '/repo', is_current: true, remote_only: false },
      ],
    });
    vi.mocked(scmWorktreeAdd).mockResolvedValue({
      path: '/repo/tree/feat/new-thing',
      project: project('/repo/tree/feat/new-thing', 'new-thing'),
      warning: 'tree/ is not gitignored',
    });
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    const onNotice = vi.fn();
    // Big roster so the filter input renders (searchThreshold).
    const groups = buildRoster(
      Array.from({ length: 8 }, (_, i) => project(`/p${i}`, `P${i}`)),
      [],
    );
    const { getByTestId } = render(() => (
      <RootDropdown
        groups={groups}
        selectedKey={null}
        onSelect={() => {}}
        activeRoot={{ kind: 'project', path: '/p0', name: 'P0' }}
        onNotice={onNotice}
      />
    ));
    openPopout(getByTestId);
    await waitFor(() => expect(screen.getByTestId('root-dropdown-popout').textContent).toContain('master'));

    const filter = screen.getByLabelText('Search Browse root') as HTMLInputElement;
    fireEvent.input(filter, { target: { value: 'feat/new-thing' } });
    const createRow = screen.getByTestId('root-dropdown-create');
    expect(createRow.textContent).toContain("Create branch + worktree 'feat/new-thing'");

    fireEvent.click(createRow);
    await waitFor(() => expect(scmWorktreeAdd).toHaveBeenCalledWith('/repo', 'feat/new-thing', true));
    await waitFor(() => expect(onNotice).toHaveBeenCalledWith('tree/ is not gitignored'));
    confirmSpy.mockRestore();
  });
});
