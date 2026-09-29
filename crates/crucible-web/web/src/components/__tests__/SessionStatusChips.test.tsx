import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, screen, fireEvent } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { SessionStatusChips } from '../SessionStatusChips';
import { ChatProvider } from '@/contexts/ChatContext';
import type { Session } from '@/lib/types';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { MockFetchAnswer } from '@/test-utils/mock-fetch';
import { installFakeEventSource } from '@/test-utils/sse';
import { getBus } from '@/lib/bus';

const [currentSession, setCurrentSession] = createSignal<Session | undefined>(undefined);
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({ currentSession }),
}));

// No `vi.mock('@/lib/api')`. The chips read the status slots and the mode list
// through `lib/query/`, so each case answers ROUTES: that is what proves the
// chips ask for the session they are pointed at, and lets the last case count
// the reads of one mode list.
//
// The chips retain the session's review state, which lists the composed diff.
// That one call is stubbed below: this suite is about the chips.
const STATUS = 'GET /api/session/s1/status';
const MODES = 'GET /api/session/s1/modes';

/** One status item as the daemon sends it, with the fields a case does not name. */
function item(fields: { id: string; plugin: string; text: string } & Record<string, unknown>) {
  return { color_group: 'info', priority: 128, pinned: false, action: null, kind: 'published', progress: null, ...fields };
}

let env: TestQueryEnv;

/** Installs a fresh cache and a fetch answering what the chips ask for. */
function serve(routes: Record<string, MockFetchAnswer> = {}): TestQueryEnv {
  env = createTestQueryEnv({
    [STATUS]: () => ({ status: [] }),
    [MODES]: () => ({ current_mode_id: 'ask', modes: [] }),
    // What the chat pane touches on mount; the last case mounts one around
    // the chips to count the reads of one mode list.
    'GET /api/session/s1': () => ({
      session_id: 's1',
      type: 'chat',
      kilns: ['/kilns/main'],
      workspace: '/kilns/main',
      state: 'active',
      title: null,
      agent_model: null,
      agent: null,
      started_at: '2026-01-01T00:00:00Z',
      event_count: 0,
    }),
    'GET /api/session/s1/history': () => ({ session_id: 's1', history: [], total_events: 0 }),
    'GET /api/interactions/pending': () => ({ pending: [] }),
    ...routes,
  });
  return env;
}

const baseSession = (id = 's1'): Session => ({
  session_id: id,
  type: 'chat',
  kilns: ['/kilns/main'],
  workspace: '/kilns/main',
  state: 'active',
  title: null,
  agent_model: null,
  started_at: '2026-01-01T00:00:00Z',
  event_count: 0,
  archived: false,
});

beforeEach(() => {
  // The chat pane opens one stream. A fake source keeps the case off the
  // network and disposes with the client.
  installFakeEventSource();
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  setCurrentSession(undefined);
  // The query cache outlives one case, so a fresh client per case keeps one
  // session's answer from serving the next one.
  env?.restore();
});

describe('SessionStatusChips', () => {
  // The engine's item for a plugin set to `ask`, as `session.status` sends it.
  const engineItem = {
    id: 'plugin_turns:goal', plugin: 'goal', text: 'goal · ask',
    color_group: 'warn', priority: 0, pinned: true, action: 'plugin_approval',
    progress: null, kind: 'plugin_turns',
  };
  const APPROVALS = 'GET /api/session/s1/config/plugin-approvals';

  it('opens the approval menu from the command route with the daemon list', async () => {
    setCurrentSession(baseSession());
    serve({
      [STATUS]: () => ({ status: [engineItem] }),
      [APPROVALS]: () => ({ approvals: { goal: 'ask', sync: 'inherit' } }),
    });
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(screen.getByTestId('session-status-plugin_turns:goal')).toBeInTheDocument());
    getBus().emit('openPluginApproval', {});
    const dialog = await waitFor(() => screen.getByRole('dialog', { name: 'Plugin approval' }));
    // Every plugin that the daemon lists, with its three values; the value
    // that the session holds is the checked one.
    await waitFor(() => expect(dialog.querySelectorAll('[role="radiogroup"]')).toHaveLength(2));
    const goal = screen.getByRole('radiogroup', { name: 'goal' });
    expect([...goal.querySelectorAll('[role="radio"]')].map((r) => r.textContent)).toEqual(['inherit', 'ask', 'stop']);
    expect(screen.getByRole('radio', { name: 'goal: ask' })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('radio', { name: 'sync: inherit' })).toHaveAttribute('aria-checked', 'true');
  });

  it('says that no plugin is loaded when the daemon lists none', async () => {
    setCurrentSession(baseSession());
    serve({ [STATUS]: () => ({ status: [engineItem] }), [APPROVALS]: () => ({ approvals: {} }) });
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(screen.getByTestId('session-status-plugin_turns:goal')).toBeInTheDocument());
    getBus().emit('openPluginApproval', {});
    await waitFor(() => expect(screen.getByRole('dialog', { name: 'Plugin approval' })).toHaveTextContent('No plugins loaded'));
  });

  it('sets the knob through the daemon when a value is chosen from the item', async () => {
    setCurrentSession(baseSession());
    const put = 'PUT /api/session/s1/config/plugins/goal/approval';
    const env = serve({
      [STATUS]: () => ({ status: [engineItem] }),
      [APPROVALS]: () => ({ approvals: { goal: 'ask' } }),
      [put]: () => ({ plugin: 'goal', approval: 'stop' }),
    });
    render(() => <SessionStatusChips />);
    const dot = await waitFor(() => screen.getByTestId('session-status-plugin_turns:goal'));
    dot.click();
    screen.getByRole('menuitem', { name: /goal · ask/ }).click();
    const stop = await waitFor(() => screen.getByRole('radio', { name: 'goal: stop' }));
    stop.click();
    await waitFor(() => expect(env.fetch.calls(put)).toBe(1));
    const sent = await env.fetch.sent(
      env.fetch.mock.calls.findIndex(([input]) => String(input instanceof Request ? input.url : input).includes('/plugins/goal/approval')),
    );
    expect(sent.body).toEqual({ approval: 'stop' });
  });

  it('renders a slot from a plugin it has never heard of', async () => {
    // The anti-regression test for the generic-rendering rule: nothing in the
    // frontend knows what these keys mean. If a new plugin ever needs a code
    // change here to show up, the channel stopped being generic.
    setCurrentSession(baseSession());
    serve({
      [STATUS]: () => ({
        status: [
          item({ id: 'zarquon', plugin: 'zarquon', text: 'flux capacitor charged' }),
        ],
      }),
    });

    render(() => <SessionStatusChips />);

    const chip = await waitFor(() => screen.getByTestId('session-status-zarquon'));
    expect(chip.textContent).toContain('flux capacitor charged');
    // The owning plugin is attributed, not interpreted.
    expect(chip.getAttribute('title')).toContain('zarquon');
  });

  it('shows a slot\'s progress', async () => {
    // The daemon writes `progress` as a fraction, `"indeterminate"`, or
    // `null` (a state, not stalled work). Before this, the web dropped the
    // field entirely, so a chip could never say how far along its work was.
    setCurrentSession(baseSession());
    serve({
      [STATUS]: () => ({
        status: [
          item({ id: 'pull', plugin: 'oci', text: 'pulling image', progress: 0.42 }),
          item({ id: 'oci', plugin: 'oci', text: 'sandboxed: alpine', progress: null }),
        ],
      }),
    });

    render(() => <SessionStatusChips />);

    const withProgress = await waitFor(() => screen.getByTestId('session-status-pull'));
    expect(withProgress.textContent).toContain('42%');

    const withoutProgress = screen.getByTestId('session-status-oci');
    expect(withoutProgress.textContent).not.toContain('%');
  });

  it('uses the named status color group supplied by the daemon', async () => {
    setCurrentSession(baseSession());
    serve({
      [STATUS]: () => ({
        status: [item({ id: 'colored', plugin: 'p', text: 'working', color_group: 'hue-4' })],
      }),
    });
    render(() => <SessionStatusChips />);
    const chip = await waitFor(() => screen.getByTestId('session-status-colored'));
    expect(chip).toHaveAttribute('data-status-color', 'hue-4');
  });

  it('keeps the authored priority order and marks pinned actions', async () => {
    setCurrentSession(baseSession());
    serve({
      [STATUS]: () => ({ status: [
        item({ id: 'later', plugin: 'weather', text: 'forecast', color_group: 'hue-2', priority: 80, pinned: false, action: null }),
        item({ id: 'ask', plugin: 'goal', text: 'goal · ask', color_group: 'warn', priority: 10, pinned: true, action: 'plugin_approval' }),
      ] }),
    });
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(screen.getByTestId('session-status-ask')).toBeInTheDocument());
    const pinned = screen.getByTestId('session-status-ask');
    expect(screen.getByTestId('session-status-strip')).not.toContainElement(pinned);
    expect(pinned).toHaveAttribute('data-pinned', 'true');
    expect(pinned).toHaveAttribute('data-action', 'plugin_approval');
    pinned.click();
    expect(screen.getAllByRole('menuitem').map((el) => el.textContent)).toEqual(['goal · askgoal', 'forecastweather']);
  });

  it('keeps pinned controls outside the scroll strip and offers every item in the menu', async () => {
    setCurrentSession(baseSession());
    serve({ [STATUS]: () => ({ status: [
      item({ id: 'early', plugin: 'sync', text: 'sync idle', priority: 10 }),
      item({ id: 'ask', plugin: 'goal', text: 'goal · ask', priority: 20, pinned: true, action: 'plugin_approval' }),
      item({ id: 'late', plugin: 'index', text: 'index ready', priority: 30 }),
    ] }) });
    render(() => <SessionStatusChips />);
    const strip = await waitFor(() => screen.getByTestId('session-status-strip'));
    expect(strip).toContainElement(screen.getByTestId('session-status-early'));
    expect(strip).toContainElement(screen.getByTestId('session-status-late'));
    expect(strip).not.toContainElement(screen.getByTestId('session-status-ask'));
    expect(screen.getByTestId('session-status-ask').querySelector('[data-testid="status-dot"]')).toBeInTheDocument();
    screen.getByTestId('session-status-late').click();
    expect(screen.getByRole('menu', { name: 'Session status' })).toBeInTheDocument();
    expect(screen.getAllByRole('menuitem')).toHaveLength(3);
  });

  it('uses a first touch tap for preview and a second tap for that item', async () => {
    setCurrentSession(baseSession());
    serve({ [STATUS]: () => ({ status: [item({ id: 'sync', plugin: 'sync', text: 'sync idle' })] }) });
    render(() => <SessionStatusChips />);
    const chip = await waitFor(() => screen.getByTestId('session-status-sync'));
    fireEvent.pointerDown(chip, { pointerType: 'touch' });
    fireEvent.pointerUp(chip, { pointerType: 'touch' });
    fireEvent.click(chip, { detail: 1 });
    expect(screen.getByTestId('session-status')).toHaveClass('is-preview');
    expect(screen.queryByRole('menu')).toBeNull();
    fireEvent.click(chip, { detail: 1 });
    expect(screen.getByRole('dialog', { name: 'Status detail' })).toHaveTextContent('sync idle — sync');
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('renders nothing when the session published no slots', async () => {
    setCurrentSession(baseSession());
    serve();
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(env.fetch.calls(STATUS)).toBe(1));
    expect(screen.queryByTestId('session-status')).toBeNull();
  });

  it('a failed fetch is silence, not an error banner', async () => {
    // Every daemon reconnect fails this request; a session with no chips is
    // the normal case, so a failure must look like one.
    setCurrentSession(baseSession());
    serve({ [STATUS]: { status: 502, body: { error: { code: 502, message: 'gone' } } } });
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(env.fetch.calls(STATUS)).toBe(1));
    expect(screen.queryByTestId('session-status')).toBeNull();
  });

  it('reads the one mode list the chat pane asked for', async () => {
    // The gate: the chips held a second `createResource` over `listModes` and
    // refetched the list on every mount, beside the chat pane's own read.
    setCurrentSession(baseSession());
    serve();

    render(() => (
      <ChatProvider sessionId="s1">
        <SessionStatusChips />
      </ChatProvider>
    ));

    await waitFor(() => expect(env.fetch.calls(MODES)).toBe(1));
    // Both readers have bound; a second request would have been made by now.
    for (let tick = 0; tick < 3; tick += 1) {
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
    expect(env.fetch.calls(MODES)).toBe(1);
  });

  it('drops the previous session slots BEFORE the new fetch resolves', async () => {
    // The second session's fetch is held open on purpose. Letting it resolve
    // with `[]` clears the chips on its own, so the assertion passed with the
    // clear-on-change deleted — it has to be made during the gap, which is the
    // entire window in which a stale "sandboxed" chip could sit over a session
    // that is not sandboxed.
    let releaseSecond: () => void = () => {};
    const secondAnswered = new Promise<void>((resolve) => {
      releaseSecond = resolve;
    });
    serve({
      [STATUS]: () => ({ status: [item({ id: 'a', plugin: 'p', text: 'first' })] }),
      'GET /api/session/s2/status': async () => {
        await secondAnswered;
        return { status: [item({ id: 'b', plugin: 'p', text: 'second' })] };
      },
    });

    setCurrentSession(baseSession('s1'));
    render(() => <SessionStatusChips />);
    await waitFor(() => expect(screen.getByTestId('session-status-a')).toBeInTheDocument());

    setCurrentSession(baseSession('s2'));
    await waitFor(() => expect(env.fetch.calls('GET /api/session/s2/status')).toBe(1));
    expect(screen.queryByTestId('session-status-a')).toBeNull();

    // ...and the in-flight answer still lands when it finally arrives.
    releaseSecond();
    await waitFor(() => expect(screen.getByTestId('session-status-b')).toBeInTheDocument());
  });
});
