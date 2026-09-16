import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, screen } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { SessionStatusChips } from '../SessionStatusChips';
import { ChatProvider } from '@/contexts/ChatContext';
import { __resetReviewStore } from '@/lib/review-store';
import type { Session } from '@/lib/types';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { MockFetchAnswer } from '@/test-utils/mock-fetch';
import { installFakeEventSource } from '@/test-utils/sse';

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

vi.mock('@/lib/review-api', () => ({
  listReviewHunks: vi.fn(async () => ({ session_id: 's', hunks: [], comments: [] })),
}));

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
  __resetReviewStore();
  // The query cache outlives one case, so a fresh client per case keeps one
  // session's answer from serving the next one.
  env?.restore();
});

describe('SessionStatusChips', () => {
  it('renders a slot from a plugin it has never heard of', async () => {
    // The anti-regression test for the generic-rendering rule: nothing in the
    // frontend knows what these keys mean. If a new plugin ever needs a code
    // change here to show up, the channel stopped being generic.
    setCurrentSession(baseSession());
    serve({
      [STATUS]: () => ({
        status: [
          { key: 'zarquon', plugin: 'zarquon', text: 'flux capacitor charged', level: 'info' },
        ],
      }),
    });

    render(() => <SessionStatusChips />);

    const chip = await waitFor(() => screen.getByTestId('session-status-zarquon'));
    expect(chip.textContent).toContain('flux capacitor charged');
    // The owning plugin is attributed, not interpreted.
    expect(chip.getAttribute('title')).toContain('zarquon');
  });

  it('styles by level and falls back for a level it does not enumerate', async () => {
    setCurrentSession(baseSession());
    serve({
      [STATUS]: () => ({
        status: [
          { key: 'a', plugin: 'p', text: 'fine', level: 'info' },
          { key: 'b', plugin: 'p', text: 'careful', level: 'warn' },
          { key: 'c', plugin: 'p', text: 'broken', level: 'error' },
          { key: 'd', plugin: 'p', text: 'nautical', level: 'chartreuse' },
        ],
      }),
    });

    render(() => <SessionStatusChips />);
    await waitFor(() => expect(screen.getByTestId('session-status-d')).toBeInTheDocument());

    const cls = (key: string) => screen.getByTestId(`session-status-${key}`).className;
    expect(cls('a')).not.toBe(cls('b'));
    expect(cls('b')).not.toBe(cls('c'));
    // An unknown level renders — quietly, like info — rather than vanishing.
    expect(screen.getByTestId('session-status-d').textContent).toContain('nautical');
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
      [STATUS]: () => ({ status: [{ key: 'a', plugin: 'p', text: 'first', level: 'info' }] }),
      'GET /api/session/s2/status': async () => {
        await secondAnswered;
        return { status: [{ key: 'b', plugin: 'p', text: 'second', level: 'info' }] };
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
