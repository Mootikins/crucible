import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent, screen } from '@solidjs/testing-library';
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
  it('floating session reads "Session folder" and the primary kiln name', () => {
    mockSession = baseSession();
    render(() => <SessionScopeChips />);
    expect(screen.getByTestId('scope-project').textContent).toContain('Session folder');
    expect(screen.getByTestId('scope-kiln').textContent).toContain('main');
  });

  it('picking a project calls setSessionWorkspace and applies the scope', async () => {
    mockSession = baseSession();
    render(() => <SessionScopeChips />);
    fireEvent.click(screen.getByTestId('scope-project'));
    await waitFor(() => expect(screen.getByText('crucible')).toBeTruthy());
    fireEvent.click(screen.getByText('crucible'));
    await waitFor(() => expect(setWorkspaceMock).toHaveBeenCalledWith('s1', '/repos/crucible'));
    await waitFor(() => expect(applySessionScopeMock).toHaveBeenCalled());
  });

  it('attached workspace shows its basename; "Session folder" detaches (workspace: null)', async () => {
    mockSession = { ...baseSession(), workspace: '/repos/crucible' };
    render(() => <SessionScopeChips />);
    expect(screen.getByTestId('scope-project').textContent).toContain('crucible');
    fireEvent.click(screen.getByTestId('scope-project'));
    await waitFor(() => expect(screen.getByText('Session folder')).toBeTruthy());
    fireEvent.click(screen.getByText('Session folder'));
    await waitFor(() => expect(setWorkspaceMock).toHaveBeenCalledWith('s1', null));
  });

  it('primary kiln is locked (disabled); toggling a connected kiln detaches it', async () => {
    mockSession = { ...baseSession(), connected_kilns: ['/kilns/extra'] };
    render(() => <SessionScopeChips />);
    expect(screen.getByTestId('scope-kiln').textContent).toContain('main +1');
    fireEvent.click(screen.getByTestId('scope-kiln'));
    await waitFor(() => expect(screen.getByText('extra')).toBeTruthy());
    const mainOption = screen.getByText('main').closest('button') as HTMLButtonElement;
    expect(mainOption.disabled).toBe(true);
    fireEvent.click(screen.getByText('extra'));
    await waitFor(() => expect(disconnectMock).toHaveBeenCalledWith('s1', '/kilns/extra'));
  });

  it('toggling an unconnected kiln attaches it', async () => {
    mockSession = baseSession();
    render(() => <SessionScopeChips />);
    fireEvent.click(screen.getByTestId('scope-kiln'));
    await waitFor(() => expect(screen.getByText('extra')).toBeTruthy());
    fireEvent.click(screen.getByText('extra'));
    await waitFor(() => expect(connectMock).toHaveBeenCalledWith('s1', '/kilns/extra'));
  });
});
