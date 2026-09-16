import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent, screen, within } from '@solidjs/testing-library';
import { useSessionScopeChips } from '../SessionScopeChips';
import { ChipRow, type ComposerChip } from '@/components/composer/ChipRow';
import type { Session } from '@/lib/types';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { resetKilnsForTests } from '@/lib/query/kilns';

/** The chips as the live composer draws them: on the shared row. */
const SessionScopeChips = () => {
  const chips = useSessionScopeChips();
  return <ChipRow chips={chips()} />;
};

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
// No `setSessionWorkspace` here: the chip must have no path to it.
vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  connectSessionKiln: (...args: unknown[]) => connectMock(...args),
  disconnectSessionKiln: (...args: unknown[]) => disconnectMock(...args),
}));

// The roster the chips read through `useKilns`, which runs the real
// `listKilns` against the mocked fetch.
const KILNS = [
  { path: '/kilns/main', name: 'main', registered: true },
  { path: '/kilns/extra', name: 'extra', registered: true },
  // An open directory the registration floor refuses. It is LABELLED here on
  // purpose: the daemon sends an empty name with `registered: false` today,
  // and a fixture that copied that would pass against a picker which only
  // checks the label. The flag is the authority.
  { path: '/home/u/.crucible/sessions', name: 'sessions', registered: false },
];

let env: TestQueryEnv;

beforeEach(() => {
  localStorage.clear();
  resetKilnsForTests();
  // The last-known roster, so the chips paint their rows on the first render
  // the way a reload does. The fetch below still runs and still corrects it —
  // the first spec here waits for exactly that.
  localStorage.setItem('crucible:cache:kilns', JSON.stringify(KILNS));
  env = createTestQueryEnv({ 'GET /api/kilns': () => ({ kilns: KILNS }) });
});

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
  env.restore();
  resetKilnsForTests();
  vi.clearAllMocks();
});

describe('SessionScopeChips', () => {
  it('floating session reads "Session folder" and the primary kiln name', () => {
    mockSession = baseSession();
    render(() => <SessionScopeChips />);
    expect(screen.getByTestId('scope-project').textContent).toContain('Session folder');
    expect(screen.getByTestId('scope-kiln').textContent).toContain('main');
  });

  // The chips carry the value only; the icon and the tooltip name the axis.
  // A label on every chip cost the row its width.
  it('carries the value without an axis label', () => {
    mockSession = baseSession();
    render(() => <SessionScopeChips />);
    expect(screen.getByTestId('scope-project').textContent).not.toContain('Project ·');
    expect(screen.getByTestId('scope-kiln').textContent).not.toContain('Kiln ·');
  });

  // An ephemeral session runs in `<session_scratch_dir>/<id>`, so the
  // basename IS the session id — 36 characters of nothing a reader can use,
  // and the widest chip on the row. It reads as what it is instead, and the
  // id stays in the title for whoever needs the actual directory.
  it("an ephemeral session's own folder reads as one, with the path in the title", () => {
    const id = 'chat-8f2c1a04-77bd-4c19-9a13-2b6e5d0f41aa';
    mockSession = { ...baseSession(), id, workspace: `/data/workspaces/${id}` };
    render(() => <SessionScopeChips />);
    const chip = screen.getByTestId('scope-project');
    expect(chip.textContent).toContain('Session folder');
    expect(chip.textContent).not.toContain(id);
    expect(chip.getAttribute('title')).toBe(`/data/workspaces/${id}`);
  });

  // Priority, not list order, decides which chips survive a narrowing pane.
  // The row folds from the right, so the scope chips have to say where they
  // sit among the model and mode chips the live composer adds around them.
  it('states where each chip sits on the row', () => {
    mockSession = baseSession();
    let captured: ComposerChip[] = [];
    const Probe = () => {
      const chips = useSessionScopeChips();
      captured = chips();
      return null;
    };
    render(() => <Probe />);
    expect(captured.map((c) => [c.key, c.priority])).toEqual([
      ['project', 30],
      ['kiln', 40],
    ]);
  });

  // A session's project is fixed at creation (the daemon refuses
  // `session.set_workspace`), so the chip says where the session acts and
  // opens nothing. It used to be a picker that moved the workspace.
  it('the project chip names the workspace and is not a control', async () => {
    mockSession = { ...baseSession(), workspace: '/repos/crucible' };
    render(() => <SessionScopeChips />);
    const chip = screen.getByTestId('scope-project');
    expect(chip.textContent).toContain('crucible');
    expect(chip.getAttribute('title')).toBe('/repos/crucible');
    expect(chip.tagName).not.toBe('BUTTON');
    expect(chip.querySelector('button')).toBeNull();
    fireEvent.click(chip);
    // Nothing opens: no popout, no "Session folder" alternative on offer.
    await Promise.resolve();
    expect(screen.queryByTestId('scope-project-popout')).toBeNull();
    expect(screen.queryByText('Session folder')).toBeNull();
    expect(applySessionScopeMock).not.toHaveBeenCalled();
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

  // `kiln.list` reports every OPEN directory, and the registration floor
  // refuses some of them — the session store, the daemon data root. The daemon
  // marks those `registered: false` and publishes no name, because a name it
  // publishes must be one `connect_kiln` resolves. Offering the row anyway
  // posts an empty name and shows the 422 as a toast.
  it('does not offer a kiln the daemon reports as unregistered', async () => {
    mockSession = baseSession();
    render(() => <SessionScopeChips />);
    fireEvent.click(screen.getByTestId('scope-kiln'));
    const popout = await screen.findByTestId('scope-kiln-popout');

    expect(within(popout).queryByText('sessions')).toBeNull();
    expect(within(popout).getByText('main')).toBeTruthy();
    expect(within(popout).getByText('extra')).toBeTruthy();
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
