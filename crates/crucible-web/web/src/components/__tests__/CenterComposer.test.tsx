import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { CenterComposer } from '../CenterComposer';
import { recordRecentFile } from '@/lib/recent-files';

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
  scmClone: vi.fn(),
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
    expect(getByTestId('cta-open-file')).toBeInTheDocument();
    expect(getByTestId('cta-commands')).toBeInTheDocument();
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

  it('lists recent files and opens one on click', async () => {
    recordRecentFile('/kiln/notes/a.md', 'a.md');
    recordRecentFile('/kiln/notes/b.md', 'b.md');
    const { getByTestId, getByText } = render(() => <CenterComposer />);
    expect(getByTestId('composer-recents')).toBeInTheDocument();
    // Most recent first.
    fireEvent.click(getByText('b.md'));
    expect(openFileInEditorMock).toHaveBeenCalledWith('/kiln/notes/b.md', 'b.md');
  });

  it('quick actions dispatch palette open events with the right mode', () => {
    const events: Array<string | undefined> = [];
    const listener = (e: Event) => events.push((e as CustomEvent).detail?.mode);
    window.addEventListener('crucible:open-command-palette', listener);
    const { getByTestId } = render(() => <CenterComposer />);
    fireEvent.click(getByTestId('cta-open-file'));
    fireEvent.click(getByTestId('cta-commands'));
    window.removeEventListener('crucible:open-command-palette', listener);
    expect(events).toEqual(['notes', undefined]);
  });
});
