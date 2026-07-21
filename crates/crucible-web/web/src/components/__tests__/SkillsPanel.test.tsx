import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';
import type { Session } from '@/lib/types';

vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    // Typed against the full Session contract (types.ts) so the mock can't
    // silently drift from the daemon payload — the return annotation forces
    // every required field (agent_mode) plus the optional last_activity /
    // archived to be present.
    currentSession: (): Session => ({
      id: 's1',
      session_type: 'chat',
      kiln: '/tmp/k',
      workspace: '/tmp/k',
      connected_kilns: [],
      state: 'active',
      title: null,
      agent_model: null,
      agent_mode: null,
      started_at: '',
      last_activity: null,
      event_count: 0,
      archived: false,
    }),
  }),
}));

const listSkillsMock = vi.fn();
const searchSkillsMock = vi.fn();
const getSkillMock = vi.fn();
const getConfigMock = vi.fn().mockResolvedValue({ kiln_path: '/tmp/k' });

vi.mock('@/lib/api', () => ({
  listSkills: (...args: unknown[]) => listSkillsMock(...args),
  searchSkills: (...args: unknown[]) => searchSkillsMock(...args),
  getSkill: (...args: unknown[]) => getSkillMock(...args),
  getConfig: () => getConfigMock(),
}));

const addNotificationMock = vi.fn();
vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: (...args: unknown[]) => addNotificationMock(...args) },
}));

// Import after mocks.
import { SkillsPanel } from '../SkillsPanel';

describe('SkillsPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers({ shouldAdvanceTime: true });
    listSkillsMock.mockResolvedValue([
      { name: 'alpha', scope: 'user', description: 'first', shadowed_count: 0 },
      { name: 'beta', scope: 'user', description: 'second', shadowed_count: 1 },
      { name: 'gamma', scope: 'kiln', description: 'kiln-local', shadowed_count: 0 },
    ]);
    searchSkillsMock.mockResolvedValue([
      { name: 'beta', scope: 'user', description: 'second', shadowed_count: 0 },
    ]);
    getSkillMock.mockResolvedValue({
      name: 'alpha',
      scope: 'user',
      description: 'first',
      source_path: '/tmp/alpha.md',
      agent: null,
      license: null,
      body: '# Alpha\n\nThe body.',
    });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('groups skills by scope and renders rows', async () => {
    render(() => <SkillsPanel />);
    await waitFor(() => expect(listSkillsMock).toHaveBeenCalledWith('/tmp/k'));

    await waitFor(() => {
      expect(screen.getByTestId('skill-row-alpha')).toBeInTheDocument();
      expect(screen.getByTestId('skill-row-beta')).toBeInTheDocument();
      expect(screen.getByTestId('skill-row-gamma')).toBeInTheDocument();
    });

    // Scope headers visible (grouping).
    expect(screen.getByText('user')).toBeInTheDocument();
    expect(screen.getByText('kiln')).toBeInTheDocument();
  });

  it('shows the shadow badge when shadowed_count > 0', async () => {
    render(() => <SkillsPanel />);
    await waitFor(() => expect(screen.getByText('+1')).toBeInTheDocument());
  });

  it('debounces typed query and switches to search endpoint', async () => {
    render(() => <SkillsPanel />);
    await waitFor(() => expect(listSkillsMock).toHaveBeenCalled());

    const input = screen.getByTestId('skills-search-input') as HTMLInputElement;
    fireEvent.input(input, { target: { value: 'be' } });

    // Before debounce fires, search shouldn't be called.
    expect(searchSkillsMock).not.toHaveBeenCalled();

    // Advance past the 200ms debounce.
    vi.advanceTimersByTime(250);
    await waitFor(() => expect(searchSkillsMock).toHaveBeenCalledWith('be', '/tmp/k'));
  });

  it('opens the drawer and loads detail on row click', async () => {
    render(() => <SkillsPanel />);
    await waitFor(() => expect(screen.getByTestId('skill-row-alpha')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('skill-row-alpha'));
    await waitFor(() => expect(screen.getByTestId('skills-drawer')).toBeInTheDocument());
    await waitFor(() => expect(getSkillMock).toHaveBeenCalledWith('alpha', '/tmp/k'));
    await waitFor(() => {
      const drawer = screen.getByTestId('skills-drawer');
      const pre = drawer.querySelector('pre');
      expect(pre?.textContent).toContain('# Alpha');
      expect(pre?.textContent).toContain('The body');
    });
  });

  it('copy-invocation writes /<name> to clipboard', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    render(() => <SkillsPanel />);
    await waitFor(() => expect(screen.getByTestId('skill-row-alpha')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('skill-row-alpha'));
    await waitFor(() => expect(screen.getByTestId('skills-copy-invocation')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('skills-copy-invocation'));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('/alpha'));
    expect(addNotificationMock).toHaveBeenCalledWith('success', expect.stringContaining('/alpha'));
  });

  it('drawer close returns to list', async () => {
    render(() => <SkillsPanel />);
    await waitFor(() => expect(screen.getByTestId('skill-row-alpha')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('skill-row-alpha'));
    await waitFor(() => expect(screen.getByTestId('skills-drawer')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('skills-drawer-close'));
    await waitFor(() => expect(screen.queryByTestId('skills-drawer')).not.toBeInTheDocument());
  });

  it('renders error notification when listSkills fails', async () => {
    listSkillsMock.mockRejectedValueOnce(new Error('boom'));
    render(() => <SkillsPanel />);
    await waitFor(() =>
      expect(addNotificationMock).toHaveBeenCalledWith('error', expect.stringContaining('boom')),
    );
  });
});
