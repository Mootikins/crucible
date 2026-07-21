import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent } from '@solidjs/testing-library';
import { SessionScopeChips } from '../SessionScopeChips';
import type { Session } from '@/lib/types';

let mockSession: Session;
const applySessionScopeMock = vi.fn();
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: () => mockSession,
    applySessionScope: applySessionScopeMock,
  }),
}));
vi.mock('@/contexts/ChatContext', () => ({
  useChatSafe: () => ({ isStreaming: () => false }),
}));

const connectMock = vi.fn().mockResolvedValue({
  session_id: 's1',
  kiln: '/kilns/main',
  workspace: '/kilns/main',
  connected_kilns: ['/kilns/extra'],
});
const disconnectMock = vi.fn().mockResolvedValue({
  session_id: 's1',
  kiln: '/kilns/main',
  workspace: '/kilns/main',
  connected_kilns: [],
});
const setWorkspaceMock = vi.fn().mockResolvedValue({
  session_id: 's1',
  kiln: '/kilns/main',
  workspace: '/kilns/main',
  connected_kilns: [],
});

vi.mock('@/lib/api', () => ({
  listKilns: vi.fn().mockResolvedValue([
    { path: '/kilns/main', name: 'main' },
    { path: '/kilns/extra', name: 'extra' },
  ]),
  listProjects: vi.fn().mockResolvedValue([{ path: '/repos/crucible', name: 'crucible', kilns: [] }]),
  connectSessionKiln: (...args: unknown[]) => connectMock(...args),
  disconnectSessionKiln: (...args: unknown[]) => disconnectMock(...args),
  setSessionWorkspace: (...args: unknown[]) => setWorkspaceMock(...args),
}));

const baseSession = (): Session => ({
  id: 's1',
  session_type: 'chat',
  kiln: '/kilns/main',
  workspace: '/kilns/main',
  connected_kilns: [],
  state: 'active',
  title: null,
  agent_model: null,
  agent_mode: null,
  started_at: '2026-01-01T00:00:00Z',
  event_count: 0,
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('SessionScopeChips', () => {
  it('floating session shows "+ project" instead of a workspace chip', () => {
    mockSession = baseSession();
    const { getByTestId, queryByTestId } = render(() => <SessionScopeChips />);
    expect(queryByTestId('workspace-chip')).toBeNull();
    expect(getByTestId('attach-project')).toBeTruthy();
  });

  it('attaching a project calls setSessionWorkspace and applies the scope', async () => {
    mockSession = baseSession();
    const { getByTestId, getByText } = render(() => <SessionScopeChips />);
    fireEvent.click(getByTestId('attach-project'));
    await waitFor(() => expect(getByText(/crucible —/)).toBeTruthy());
    fireEvent.click(getByText(/crucible —/));
    await waitFor(() => expect(setWorkspaceMock).toHaveBeenCalledWith('s1', '/repos/crucible'));
    await waitFor(() => expect(applySessionScopeMock).toHaveBeenCalled());
  });

  it('attached workspace shows a chip whose ✕ detaches (workspace: null)', async () => {
    mockSession = { ...baseSession(), workspace: '/repos/crucible' };
    const { getByTestId } = render(() => <SessionScopeChips />);
    expect(getByTestId('workspace-chip').textContent).toContain('crucible');
    fireEvent.click(getByTestId('detach-workspace'));
    await waitFor(() => expect(setWorkspaceMock).toHaveBeenCalledWith('s1', null));
  });

  it('primary kiln has no detach control; connected kiln does', async () => {
    mockSession = { ...baseSession(), connected_kilns: ['/kilns/extra'] };
    const { getByTestId } = render(() => <SessionScopeChips />);
    expect(getByTestId('primary-kiln-chip').querySelector('button')).toBeNull();
    fireEvent.click(getByTestId('detach-kiln-extra'));
    await waitFor(() => expect(disconnectMock).toHaveBeenCalledWith('s1', '/kilns/extra'));
  });

  it('kiln picker excludes the primary and already-connected kilns', async () => {
    mockSession = { ...baseSession(), connected_kilns: [] };
    const { getByTestId, getByText, queryByText } = render(() => <SessionScopeChips />);
    fireEvent.click(getByTestId('attach-kiln'));
    await waitFor(() => expect(getByText(/extra —/)).toBeTruthy());
    expect(queryByText(/main —/)).toBeNull();
    fireEvent.click(getByText(/extra —/));
    await waitFor(() => expect(connectMock).toHaveBeenCalledWith('s1', '/kilns/extra'));
  });
});
