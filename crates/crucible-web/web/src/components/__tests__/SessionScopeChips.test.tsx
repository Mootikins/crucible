import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent, screen, within } from '@solidjs/testing-library';
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
  kilns: ['main', 'extra'],
  workspace: null,
});
const disconnectMock = vi.fn().mockResolvedValue({
  session_id: 's1',
  kilns: ['main'],
  workspace: null,
});
const setWorkspaceMock = vi.fn().mockResolvedValue({
  session_id: 's1',
  kilns: ['main'],
  workspace: null,
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
  // Registry NAMES, which is what a session's kiln set is on the wire.
  kilns: ['main'],
  // Floating: the daemon says outright that this session has no workspace.
  workspace: null,
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

  it('every attached kiln toggles off, including the first', async () => {
    mockSession = { ...baseSession(), kilns: ['main', 'extra'] };
    render(() => <SessionScopeChips />);
    expect(screen.getByTestId('scope-kiln').textContent).toContain('main +1');
    fireEvent.click(screen.getByTestId('scope-kiln'));
    await waitFor(() => expect(screen.getByText('extra')).toBeTruthy());
    // Flattening removed the locked primary row: no member is privileged.
    const mainOption = screen.getByText('main').closest('button') as HTMLButtonElement;
    expect(mainOption.disabled).toBe(false);
    fireEvent.click(screen.getByText('extra'));
    await waitFor(() => expect(disconnectMock).toHaveBeenCalledWith('s1', 'extra'));
  });

  it('detaching the only attached kiln is offered like any other detach', async () => {
    mockSession = baseSession();
    render(() => <SessionScopeChips />);
    fireEvent.click(screen.getByTestId('scope-kiln'));
    const popout = await screen.findByTestId('scope-kiln-popout');
    const only = within(popout).getByText('main').closest('button') as HTMLButtonElement;
    expect(only.disabled).toBe(false);
    fireEvent.click(only);
    await waitFor(() => expect(disconnectMock).toHaveBeenCalledWith('s1', 'main'));
  });

  it('a kiln-less session reads as tools-only, and says the note tools are gone', async () => {
    mockSession = { ...baseSession(), kilns: [], workspace: null };
    render(() => <SessionScopeChips />);
    expect(screen.getByTestId('scope-kiln').textContent).toContain('No kiln');
    fireEvent.click(screen.getByTestId('scope-kiln'));
    const note = await screen.findByTestId('scope-kiln-empty');
    // A legitimate state, described — not an error, and not silent about the
    // capabilities it costs.
    expect(note.textContent).toMatch(/tools-only/i);
    expect(note.textContent).toMatch(/note/i);
  });

  it('a kiln-less session never borrows the home kiln as its label', () => {
    mockSession = { ...baseSession(), kilns: [], workspace: null };
    render(() => <SessionScopeChips />);
    expect(screen.getByTestId('scope-kiln').textContent).not.toContain('Home kiln');
  });

  it('toggling an unconnected kiln attaches it', async () => {
    mockSession = baseSession();
    render(() => <SessionScopeChips />);
    fireEvent.click(screen.getByTestId('scope-kiln'));
    await waitFor(() => expect(screen.getByText('extra')).toBeTruthy());
    fireEvent.click(screen.getByText('extra'));
    await waitFor(() => expect(connectMock).toHaveBeenCalledWith('s1', 'extra'));
  });

  // The join is on the registry NAME. It used to be on `path`, which meant a
  // session's kiln set and the kiln list agreed only as long as both spelled a
  // kiln the same way — and the route now answers 422 to a path, so the chip
  // would have posted one and shown the failure as a toast.
  it('attaches by name, and the daemon is told the name', async () => {
    mockSession = baseSession();
    render(() => <SessionScopeChips />);
    fireEvent.click(screen.getByTestId('scope-kiln'));
    const popout = await screen.findByTestId('scope-kiln-popout');
    fireEvent.click(within(popout).getByText('extra'));
    await waitFor(() => expect(connectMock).toHaveBeenCalledWith('s1', 'extra'));
    const [, sent] = connectMock.mock.calls[0] as [string, string];
    expect(sent).not.toContain('/');
  });

  // A path in `kilns` is what a session file written before names carries. It
  // is not the entry whose directory it happens to equal: crediting it would
  // mark `main` attached, and clicking `main` would then DETACH a kiln the
  // session never had while leaving the real entry in place.
  it('a path in the kiln set is not credited to the entry it points at', async () => {
    mockSession = { ...baseSession(), kilns: ['/kilns/main'] };
    render(() => <SessionScopeChips />);
    fireEvent.click(screen.getByTestId('scope-kiln'));
    const popout = await screen.findByTestId('scope-kiln-popout');
    const registered = within(popout).getByText('main').closest('button') as HTMLButtonElement;
    expect(registered.getAttribute('aria-selected')).not.toBe('true');
    // ...and it still has a row of its own, or it could never be detached.
    // (`/kilns/main` also appears as the unattached `main` row's directory
    // hint, so this looks for the one that is a row, not the one that is text.)
    const orphan = within(popout)
      .getAllByText('/kilns/main')
      .map((el) => el.closest('button'))
      .find((b) => b?.getAttribute('aria-selected') === 'true');
    expect(orphan).toBeTruthy();
  });
});
