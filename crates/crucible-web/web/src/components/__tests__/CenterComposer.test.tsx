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
  getConfig: vi.fn().mockResolvedValue({
    kiln_path: '/home/user/kilns/helios',
  }),
  // The two axes, as plugins published them. Not read out of plugin config:
  // the composer renders what providers declared and nothing more.
  getTargetProviders: vi.fn().mockImplementation((axis: string) =>
    Promise.resolve(
      axis === 'runtime'
        ? [{ plugin: 'oci', axis, label: 'Container', targets_command: 'oci.targets' }]
        : [
            {
              plugin: 'worktree',
              axis,
              label: 'Worktree',
              targets_command: 'worktree.targets',
              resolve_command: 'worktree.resolve',
            },
          ],
    ),
  ),
  getProviderTargets: vi.fn().mockImplementation((provider: { plugin: string }) =>
    Promise.resolve(
      provider.plugin === 'oci'
        ? [
            { value: '', label: 'Default', hint: 'alpine:latest', spec: 'oci:' },
            { value: 'throwaway', label: 'throwaway', spec: 'oci:throwaway' },
          ]
        : [
            {
              value: 'master',
              label: 'master',
              hint: 'current',
              spec: 'worktree:master',
              path: '/repos/crucible',
              current: true,
            },
            { value: 'feat/x', label: 'feat/x', hint: 'new worktree', spec: 'worktree:feat/x' },
          ],
    ),
  ),
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
  scmClone: vi.fn(),
  registerProject: vi.fn().mockResolvedValue({}),
}));

// Re-established per test, not just cleared: a test that narrows a provider
// (no targets, a second provider) would otherwise leave its implementation
// installed for everything that follows — which is exactly how four of these
// silently started asserting against an empty menu.
beforeEach(async () => {
  localStorage.clear();
  createSessionMock.mockClear();
  openFileInEditorMock.mockClear();

  const { getTargetProviders, getProviderTargets } = await import('@/lib/api');
  vi.mocked(getTargetProviders).mockImplementation((axis) =>
    Promise.resolve(
      axis === 'runtime'
        ? [{ plugin: 'oci', axis, label: 'Container', targets_command: 'oci.targets' }]
        : [
            {
              plugin: 'worktree',
              axis,
              label: 'Worktree',
              targets_command: 'worktree.targets',
              resolve_command: 'worktree.resolve',
            },
          ],
    ),
  );
  vi.mocked(getProviderTargets).mockImplementation((provider) =>
    Promise.resolve(
      provider.plugin === 'oci'
        ? [
            { value: '', label: 'Default', hint: 'alpine:latest', spec: 'oci:' },
            { value: 'throwaway', label: 'throwaway', spec: 'oci:throwaway' },
          ]
        : [
            {
              value: 'master',
              label: 'master',
              hint: 'current',
              spec: 'worktree:master',
              path: '/repos/crucible',
              current: true,
            },
            { value: 'feat/x', label: 'feat/x', hint: 'new worktree', spec: 'worktree:feat/x' },
          ],
    ),
  );
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

  it('shows the model chip as Auto, not a "choose one" placeholder', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    const chip = await waitFor(() => getByTestId('composer-model'));
    // An unset model IS the Auto row (provider default) — the chip names it
    // rather than implying a choice is still owed.
    expect(chip.textContent).toContain('Auto');
    expect(chip.textContent).not.toContain('Select model');
  });

  it('marks each ACP agent row with its own icon', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    const chip = await waitFor(() => getByTestId('composer-agent'));
    fireEvent.click(chip);

    // Internal agent + the mocked 'claude' profile, each iconed. (The chip
    // trigger repeats the selected label, so scope to the option list.)
    const list = await screen.findByRole('listbox', { name: 'agent' });
    for (const label of ['Internal agent', 'claude']) {
      const row = [...list.querySelectorAll('button')].find((b) =>
        b.textContent?.includes(label),
      );
      expect(row, `no ${label} row`).toBeTruthy();
      expect(row!.querySelector('svg'), `${label} row has no icon`).toBeTruthy();
    }
    // The two marks are different glyphs, not the same fallback.
    const paths = [...list.querySelectorAll('button svg')].map((s) => s.innerHTML);
    expect(new Set(paths).size).toBe(paths.length);
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

  const submitWith = async (getByTestId: (id: string) => HTMLElement, text: string) => {
    const input = getByTestId('composer-input') as HTMLTextAreaElement;
    fireEvent.input(input, { target: { value: text } });
    fireEvent.click(getByTestId('composer-send'));
    await waitFor(() => expect(createSessionMock).toHaveBeenCalled());
    return createSessionMock.mock.calls[0][0];
  };

  const pickProject = async (getByTestId: (id: string) => HTMLElement) => {
    await waitFor(() => expect(getByTestId('composer-project')).toBeInTheDocument());
    fireEvent.click(getByTestId('composer-project'));
    await waitFor(() => expect(screen.getByText('crucible')).toBeInTheDocument());
    fireEvent.click(screen.getByText('crucible'));
  };

  // ---------------------------------------------------------------------------
  // The workspace axis — where the session's files live
  // ---------------------------------------------------------------------------

  it('offers the branches a workspace provider enumerated for the selected project', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    await waitFor(() => expect(getByTestId('composer-workspace-target')).toBeInTheDocument());

    fireEvent.click(getByTestId('composer-workspace-target'));
    // A single provider flattens: a submenu holding the entire menu is an
    // extra click, not a drill-down.
    await waitFor(() => expect(screen.getByText('feat/x')).toBeInTheDocument());
    expect(screen.queryByText('Worktree')).toBeNull();
  });

  it('sends the pick as a provider-addressed spec, and nothing when untouched', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    await waitFor(() => expect(getByTestId('composer-workspace-target')).toBeInTheDocument());

    expect(await submitWith(getByTestId, 'untouched')).not.toHaveProperty('workspace_target');
    createSessionMock.mockClear();
    cleanup();

    const second = render(() => <CenterComposer />);
    await waitFor(() => expect(second.getByTestId('composer-workspace-target')).toBeInTheDocument());
    fireEvent.click(second.getByTestId('composer-workspace-target'));
    await waitFor(() => expect(screen.getByText('feat/x')).toBeInTheDocument());
    fireEvent.click(screen.getByText('feat/x'));

    // The daemon splits on the first colon to find who answers. A bare branch
    // name would name no provider.
    expect(await submitWith(second.getByTestId, 'on a worktree')).toMatchObject({
      workspace_target: 'worktree:feat/x',
    });
  });

  // A branch chosen for one repo need not exist in the next. Carrying it over
  // would resolve against a repository the user never picked it in.
  it('clears the chosen workspace target when the project changes', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    await waitFor(() => expect(getByTestId('composer-workspace-target')).toBeInTheDocument());
    fireEvent.click(getByTestId('composer-workspace-target'));
    await waitFor(() => expect(screen.getByText('feat/x')).toBeInTheDocument());
    fireEvent.click(screen.getByText('feat/x'));
    await waitFor(() =>
      expect(getByTestId('composer-workspace-target').textContent).toContain('feat/x'),
    );

    await pickProject(getByTestId);
    await waitFor(() =>
      expect(getByTestId('composer-workspace-target').textContent).not.toContain('feat/x'),
    );
    expect(await submitWith(getByTestId, 'after switching')).not.toHaveProperty('workspace_target');
  });

  it('shows no workspace chip when no provider offers anything', async () => {
    const { getProviderTargets } = await import('@/lib/api');
    vi.mocked(getProviderTargets).mockResolvedValue([]);

    const { queryByTestId, getByTestId } = render(() => <CenterComposer />);
    await waitFor(() => expect(getByTestId('composer-kiln').textContent).toContain('helios'));
    // A repo-less project gets no chip rather than an empty one — a control
    // that can only fail is worse than none.
    await waitFor(() => expect(queryByTestId('composer-workspace-target')).toBeNull());
  });

  // ---------------------------------------------------------------------------
  // The runtime axis — where the process runs
  // ---------------------------------------------------------------------------

  it('always offers this machine, whatever plugins are installed', async () => {
    const { getProviderTargets } = await import('@/lib/api');
    vi.mocked(getProviderTargets).mockResolvedValue([]);

    const { getByTestId } = render(() => <CenterComposer />);
    fireEvent.click(getByTestId('composer-target'));
    // Running here is what happens when no provider is asked, so it cannot
    // depend on one being installed.
    await waitFor(() => expect(screen.getByText('This PC')).toBeInTheDocument());
  });

  it('leaves the runtime unset until the chip is touched', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    await waitFor(() => expect(getByTestId('composer-target')).toBeInTheDocument());
    // Untouched and "This PC" are different instructions: one lets the
    // project's own setting decide, the other overrides it.
    expect(await submitWith(getByTestId, 'untouched')).not.toHaveProperty('isolation');
  });

  it('sends false when this machine is chosen explicitly', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    fireEvent.click(getByTestId('composer-target'));
    await waitFor(() => expect(screen.getByText('This PC')).toBeInTheDocument());
    fireEvent.click(screen.getByText('This PC'));

    expect(await submitWith(getByTestId, 'no sandbox')).toMatchObject({ isolation: false });
  });

  // Addressed, not a bare name: more than one plugin answers on this channel
  // now, and a name meant for one used to be a hard error inside another.
  it('addresses a runtime target to the provider that offered it', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    fireEvent.click(getByTestId('composer-target'));
    await waitFor(() => expect(screen.getByText('throwaway')).toBeInTheDocument());
    fireEvent.click(screen.getByText('throwaway'));

    expect(await submitWith(getByTestId, 'sandboxed')).toMatchObject({
      isolation: { plugin: 'oci', target: 'throwaway' },
    });
  });

  it('addresses an unnamed default target to its provider too', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    fireEvent.click(getByTestId('composer-target'));
    await waitFor(() => expect(screen.getByText('Default')).toBeInTheDocument());
    fireEvent.click(screen.getByText('Default'));

    // Empty target, still addressed: the provider resolves its own default.
    expect(await submitWith(getByTestId, 'default env')).toMatchObject({
      isolation: { plugin: 'oci', target: '' },
    });
  });

  // The axes are independent, and this is the combination the oci plugin
  // already assumed worked but nothing could express.
  it('carries a worktree and a container together', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    await waitFor(() => expect(getByTestId('composer-workspace-target')).toBeInTheDocument());

    fireEvent.click(getByTestId('composer-workspace-target'));
    await waitFor(() => expect(screen.getByText('feat/x')).toBeInTheDocument());
    fireEvent.click(screen.getByText('feat/x'));

    fireEvent.click(getByTestId('composer-target'));
    await waitFor(() => expect(screen.getByText('throwaway')).toBeInTheDocument());
    fireEvent.click(screen.getByText('throwaway'));

    expect(await submitWith(getByTestId, 'both axes')).toMatchObject({
      workspace_target: 'worktree:feat/x',
      isolation: { plugin: 'oci', target: 'throwaway' },
    });
  });

  it('drills into a submenu once a second provider answers on an axis', async () => {
    const { getTargetProviders } = await import('@/lib/api');
    vi.mocked(getTargetProviders).mockImplementation((axis: string) =>
      Promise.resolve(
        axis === 'runtime'
          ? [
              { plugin: 'oci', axis, label: 'Container', targets_command: 'oci.targets' },
              { plugin: 'ssh', axis, label: 'Remote Machines', targets_command: 'ssh.targets' },
            ]
          : [],
      ),
    );

    const { getByTestId } = render(() => <CenterComposer />);
    fireEvent.click(getByTestId('composer-target'));
    // Two providers earn the drill-down; the targets stay behind it.
    await waitFor(() => expect(screen.getByText('Remote Machines')).toBeInTheDocument());
    expect(screen.getByText('Container')).toBeInTheDocument();
    expect(screen.queryByText('throwaway')).toBeNull();

    fireEvent.click(screen.getByText('Remote Machines'));
    await waitFor(() => expect(screen.getByTestId('composer-target-flyout')).toBeInTheDocument());
  });

  it('shows the remote-control status card in the run-on menu', async () => {
    const { getByTestId } = render(() => <CenterComposer />);
    fireEvent.click(getByTestId('composer-target'));
    const pop = await waitFor(() => screen.getByTestId('composer-target-popout'));
    expect(pop.textContent).toContain('Remote control');
    await waitFor(() =>
      expect(screen.getByTestId('remote-control-state').getAttribute('aria-label')).toBe(
        'Remote control off',
      ),
    );
  });

});
