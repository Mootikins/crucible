import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createMemo, createSignal } from 'solid-js';
import { render, fireEvent, screen, waitFor } from '@solidjs/testing-library';
import { RootDropdown } from '../RootDropdown';
import { buildRoster, rootKey, rosterIndex, type TreeRoot } from '@/lib/tree-root';
import type { KilnListEntry, Project } from '@/lib/types';

vi.mock('@/lib/api', () => ({
  listWorkspaceTargets: vi.fn(),
  resolveWorkspaceTarget: vi.fn(),
  scmClone: vi.fn(),
  registerProject: vi.fn(),
  isGitRepoUrl: (s: string) => /^(https?:\/\/|git@)/.test(s) || /^[\w.-]+\/[\w.-]+$/.test(s),
}));

import { listWorkspaceTargets, resolveWorkspaceTarget, registerProject } from '@/lib/api';

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
  // A repo-less root answers with no targets rather than throwing — the
  // enumerating calls swallow provider failure so one bad plugin cannot take
  // the picker down.
  vi.mocked(listWorkspaceTargets).mockResolvedValue([]);
  vi.mocked(resolveWorkspaceTarget).mockReset();
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

  it('lists workspace targets for an active project root and jumps to an existing checkout', async () => {
    vi.mocked(listWorkspaceTargets).mockResolvedValue([
      {
        value: 'master',
        label: 'master',
        hint: 'current',
        spec: 'worktree:master',
        path: '/repo',
        current: true,
      },
      {
        value: 'feat/x',
        label: 'feat/x',
        hint: 'feat-x',
        spec: 'worktree:feat/x',
        path: '/repo/tree/feat/x',
      },
    ]);
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
    await waitFor(() =>
      expect(screen.getByTestId('root-dropdown-popout').textContent).toContain('Branches — repo'),
    );

    fireEvent.click(screen.getByText('feat/x'));
    await waitFor(() =>
      expect(onSelect).toHaveBeenCalledWith({
        kind: 'project',
        path: '/repo/tree/feat/x',
        name: 'x',
      }),
    );
    expect(registerProject).toHaveBeenCalledWith('/repo/tree/feat/x');
    // A target the provider already resolved needs no round trip.
    expect(resolveWorkspaceTarget).not.toHaveBeenCalled();
  });

  // No confirmation prompt: picking a row labelled "new worktree" IS the
  // confirmation, and the provider is idempotent if it turns out to exist.
  it('asks the provider to materialise a target that has no checkout yet', async () => {
    vi.mocked(listWorkspaceTargets).mockResolvedValue([
      { value: 'fix/y', label: 'fix/y', hint: 'new worktree', spec: 'worktree:fix/y' },
    ]);
    vi.mocked(resolveWorkspaceTarget).mockResolvedValue('/repo/tree/fix/y');
    vi.mocked(registerProject).mockResolvedValue(project('/repo/tree/fix/y', 'y'));
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
    await waitFor(() =>
      expect(resolveWorkspaceTarget).toHaveBeenCalledWith('worktree:fix/y', '/repo'),
    );
    await waitFor(() =>
      expect(onSelect).toHaveBeenCalledWith({
        kind: 'project',
        path: '/repo/tree/fix/y',
        name: 'y',
      }),
    );
  });

  // A target the provider refuses (a name git rejects, a busy destination)
  // must say so — this is an explicit pick, not a background enumeration.
  it('surfaces a provider refusal instead of silently doing nothing', async () => {
    vi.mocked(listWorkspaceTargets).mockResolvedValue([
      { value: 'fix/y', label: 'fix/y', hint: 'new worktree', spec: 'worktree:fix/y' },
    ]);
    vi.mocked(resolveWorkspaceTarget).mockRejectedValue(new Error('destination busy'));
    const onNotice = vi.fn();
    const groups = buildRoster([project('/repo', 'repo')], []);
    const { getByTestId } = render(() => (
      <RootDropdown
        groups={groups}
        selectedKey="project:/repo"
        onSelect={() => {}}
        activeRoot={{ kind: 'project', path: '/repo', name: 'repo' }}
        onNotice={onNotice}
      />
    ));
    openPopout(getByTestId);
    await waitFor(() => expect(screen.getByText('fix/y')).toBeTruthy());

    fireEvent.click(screen.getByText('fix/y'));
    await waitFor(() => expect(onNotice).toHaveBeenCalledWith('destination busy'));
  });

  it('typing an unknown name offers branch-plus-worktree creation', async () => {
    vi.mocked(listWorkspaceTargets).mockResolvedValue([
      {
        value: 'master',
        label: 'master',
        hint: 'current',
        spec: 'worktree:master',
        path: '/repo',
        current: true,
      },
    ]);
    vi.mocked(resolveWorkspaceTarget).mockResolvedValue('/repo/tree/feat/new-thing');
    vi.mocked(registerProject).mockResolvedValue(
      project('/repo/tree/feat/new-thing', 'new-thing'),
    );
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
    await waitFor(() =>
      expect(screen.getByTestId('root-dropdown-popout').textContent).toContain('master'),
    );

    const filter = screen.getByLabelText('Search Browse root') as HTMLInputElement;
    fireEvent.input(filter, { target: { value: 'feat/new-thing' } });
    const createRow = screen.getByTestId('root-dropdown-create');
    expect(createRow.textContent).toContain("Create branch + worktree 'feat/new-thing'");

    fireEvent.click(createRow);
    // Addressed to the provider that offered the other rows, so a typed name
    // goes to the same place a picked one does.
    await waitFor(() =>
      expect(resolveWorkspaceTarget).toHaveBeenCalledWith('worktree:feat/new-thing', '/p0'),
    );
  });
});
