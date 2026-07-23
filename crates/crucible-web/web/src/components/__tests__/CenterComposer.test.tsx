import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { CenterComposer } from '../CenterComposer';

const createSessionMock = vi.fn().mockResolvedValue({ id: 'sess-1' });
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({ createSession: createSessionMock }),
}));

const openFileInEditorMock = vi.fn();
vi.mock('@/lib/file-actions', () => ({
  openFileInEditor: (...args: unknown[]) => openFileInEditorMock(...args),
}));

vi.mock('@/lib/api', () => ({
  getConfig: vi.fn().mockResolvedValue({ kiln_path: '/home/user/kilns/helios' }),
  listAgents: vi.fn().mockResolvedValue([
    { name: 'claude', description: 'Claude Code via ACP', command: 'npx', is_builtin: true, available: true },
  ]),
  listAllModels: vi.fn().mockResolvedValue(['ollama/llama3.2', 'openai/gpt-4o']),
  listKilns: vi.fn().mockResolvedValue([{ path: '/home/user/kilns/other', name: 'other' }]),
  listProjects: vi.fn().mockResolvedValue([{ path: '/repos/crucible', name: 'crucible', kilns: [] }]),
  listProviders: vi.fn().mockResolvedValue([
    { name: 'ollama', available: true, default_model: 'llama3.2' },
  ]),
  // Server-backed recents: empty server list keeps localStorage-driven
  // fixtures in charge; record is fire-and-forget.
  fetchRecents: vi.fn().mockResolvedValue([]),
  recordRecent: vi.fn().mockResolvedValue(undefined),
  // Clone-from-popout flow.
  isGitRepoUrl: (s: string) => /^(https?:\/\/|git@)/.test(s) || /^[\w.-]+\/[\w.-]+$/.test(s),
  isBranchNameish: (s: string) => !!s && !/\s|\.\.|^[-/]|\\|:|@\{|\/$/.test(s),
  scmClone: vi.fn(),
  // Branch chip: no repo unless a test overrides.
  scmBranches: vi.fn().mockRejectedValue(new Error('no repo')),
  scmWorktreeAdd: vi.fn(),
  registerProject: vi.fn().mockResolvedValue({}),
}));

beforeEach(() => {
  localStorage.clear();
  createSessionMock.mockClear();
  openFileInEditorMock.mockClear();
});
afterEach(cleanup);

describe('CenterComposer', () => {
  it('renders the input, context chips, and quick actions', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    expect(getByTestId('composer-input')).toBeInTheDocument();
    expect(getByTestId('composer-kiln')).toBeInTheDocument();
    expect(getByTestId('composer-project')).toBeInTheDocument();
    expect(getByTestId('composer-agent')).toBeInTheDocument();
    await waitFor(() => expect(getByTestId('composer-model')).toBeInTheDocument());
  });

  it('Enter submits the first message through createSession', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    // Wait for the async defaults (config/kilns) to land before submitting —
    // 'helios' appears on the chip label once defaultKiln resolves.
    await waitFor(() =>
      expect(getByTestId('composer-kiln').textContent).toContain('helios'),
    );

    const input = getByTestId('composer-input') as HTMLTextAreaElement;
    fireEvent.input(input, { target: { value: 'hello world' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(createSessionMock).toHaveBeenCalledTimes(1));
    const [scope, opts] = createSessionMock.mock.calls[0];
    expect(scope.kiln).toBe('/home/user/kilns/helios');
    expect(opts.initialMessage).toBe('hello world');
  });

  it('a chip popout selects a value used on submit', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    await waitFor(() => expect(getByTestId('composer-project')).toBeInTheDocument());
    fireEvent.click(getByTestId('composer-project'));
    // The popout renders through a Portal into document.body — query via screen.
    await waitFor(() => expect(screen.getByTestId('composer-project-popout')).toBeInTheDocument());
    await waitFor(() => expect(screen.getByText('crucible')).toBeInTheDocument());
    fireEvent.click(screen.getByText('crucible'));

    const input = getByTestId('composer-input') as HTMLTextAreaElement;
    fireEvent.input(input, { target: { value: 'with project' } });
    fireEvent.click(getByTestId('composer-send'));
    await waitFor(() => expect(createSessionMock).toHaveBeenCalled());
    expect(createSessionMock.mock.calls[0][0].workspace).toBe('/repos/crucible');
  });

  it('project popout has a discoverable clone action row', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    await waitFor(() => expect(getByTestId('composer-project')).toBeInTheDocument());
    fireEvent.click(getByTestId('composer-project'));
    // Always-visible footer row — no filter typing required to find it.
    const row = await waitFor(() => screen.getByTestId('composer-project-action'));
    expect(row.textContent).toContain('Clone a repository…');

    fireEvent.click(row);
    const input = screen.getByTestId('composer-project-action-input') as HTMLInputElement;
    fireEvent.input(input, { target: { value: 'octocat/Spoon-Knife' } });
    const submit = screen.getByTestId('composer-project-action-submit') as HTMLButtonElement;
    expect(submit.disabled).toBe(false);

    const { scmClone } = await import('@/lib/api');
    vi.mocked(scmClone).mockResolvedValue({
      path: '/home/user/Projects/Spoon-Knife',
      project: { path: '/home/user/Projects/Spoon-Knife', name: 'Spoon-Knife', kilns: [], last_accessed: '' },
    });
    fireEvent.click(submit);
    await waitFor(() => expect(scmClone).toHaveBeenCalledWith('octocat/Spoon-Knife'));
  });

  it('selecting a repo project reveals a branch chip; picking a branch jumps to its worktree', async () => {
    const { scmBranches, listProjects } = await import('@/lib/api');
    vi.mocked(scmBranches).mockResolvedValue({
      repo_root: '/repos/crucible',
      current_branch: 'master',
      branches: [
        { name: 'master', worktree_path: '/repos/crucible', is_current: true, remote_only: false },
        {
          name: 'feat/x',
          worktree_path: '/repos/crucible/tree/feat/x',
          is_current: false,
          remote_only: false,
        },
      ],
    });

    const { getByTestId, queryByTestId } = render(() => <CenterComposer />);
    // No branch chip before a repo project is selected.
    expect(queryByTestId('composer-branch')).toBeNull();

    await waitFor(() => expect(getByTestId('composer-project')).toBeInTheDocument());
    fireEvent.click(getByTestId('composer-project'));
    await waitFor(() => expect(screen.getByText('crucible')).toBeInTheDocument());
    fireEvent.click(screen.getByText('crucible'));

    // Branch chip appears showing the current branch.
    await waitFor(() =>
      expect(getByTestId('composer-branch').textContent).toContain('master'),
    );

    // Picking the other branch switches the workspace to its worktree
    // (post-switch refresh returns the roster including the worktree).
    vi.mocked(listProjects).mockResolvedValue([
      { path: '/repos/crucible', name: 'crucible', kilns: [], last_accessed: '' },
      {
        path: '/repos/crucible/tree/feat/x',
        name: 'x',
        kilns: [],
        last_accessed: '',
        repository: {
          root: '/repos/crucible',
          is_worktree: true,
          main_repo_git_dir: '/repos/crucible/.git',
        },
      },
    ]);
    fireEvent.click(getByTestId('composer-branch'));
    await waitFor(() => expect(screen.getByText('feat/x')).toBeInTheDocument());
    fireEvent.click(screen.getByText('feat/x'));
    await waitFor(() =>
      expect(getByTestId('composer-project').textContent).toContain('feat/x'),
    );
  });

  it('run-on chip shows the local machine with disabled cloud/remote seams', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    const chip = getByTestId('composer-target');
    expect(chip.textContent).toContain('This machine');

    fireEvent.click(chip);
    const pop = await waitFor(() => screen.getByTestId('composer-target-popout'));
    expect(pop.textContent).toContain('Run on');
    const rows = [...pop.querySelectorAll('[role="option"]')] as HTMLButtonElement[];
    expect(rows.map((r) => r.disabled)).toEqual([false, true, true]);
    // Remote-control status card (state from /api/config remote_shell).
    expect(pop.textContent).toContain('Remote control');
    await waitFor(() =>
      expect(screen.getByTestId('remote-control-state').getAttribute('aria-label')).toBe(
        'Remote control off',
      ),
    );
  });


});
