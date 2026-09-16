import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup, waitFor, screen } from '@solidjs/testing-library';

/**
 * The panel drew a fixed set of controls, which was wrong for every ACP
 * session. ACP has no temperature and no token cap — the protocol has no
 * field for either — so the slider and the number box changed nothing, and
 * the daemon now refuses those calls outright.
 *
 * So the panel asks the session what it has. These tests drive the two
 * answers a session can give and check what gets drawn, because that is where
 * the old bug was visible and nowhere else.
 */
// No `vi.mock('@/lib/api')`. The panel reads the daemon through
// `lib/query/session-config.ts` now, so each case answers ROUTES: that is what
// proves the panel asks for the right thing, and it is the only way to see
// that a re-read follows a write.
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: () => ({ session_id: 's1', title: 'T' }),
  }),
}));

import { ModelSettingsSection } from '../ModelSettings';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { MockFetchAnswer } from '@/test-utils/mock-fetch';

const KNOBS = 'GET /api/session/s1/knobs';
const OPTIONS = 'GET /api/session/s1/config/agent-options';
const SET_OPTION = 'POST /api/session/s1/config/agent-options';
const PRECOG = 'GET /api/session/s1/config/precognition';

let env: TestQueryEnv;

/** Installs a fresh cache and a fetch answering the panel's three reads. */
function serve(routes: Record<string, MockFetchAnswer> = {}): TestQueryEnv {
  env = createTestQueryEnv({
    [KNOBS]: () => ALL_SUPPORTED,
    [OPTIONS]: () => ({ session_id: 's1', options: [] }),
    [PRECOG]: () => ({ precognition_enabled: true }),
    [SET_OPTION]: () => new Response(null, { status: 204 }),
    ...routes,
  });
  return env;
}

/** What the daemon answers for a session that has every setting. */
const ALL_SUPPORTED = {
  knobs: [{ id: 'precognition', supported: true }],
};

/**
 * A session that refuses it. Which knobs a session refuses depends on the
 * session, so what these tests pin is that the panel obeys the answer it is
 * given rather than a set of rows compiled into the client.
 */
const ONE_UNSUPPORTED = {
  knobs: [{ id: 'precognition', supported: false }],
};

afterEach(() => {
  cleanup();
  env?.restore();
  vi.clearAllMocks();
});

describe('ModelSettingsSection', () => {
  it('draws the controls a session supports', async () => {
    serve();
    render(() => <ModelSettingsSection />);

    await waitFor(() => expect(env.fetch.calls(KNOBS)).toBe(1));
    // `waitFor`, not a bare assertion: the call landing is not the render
    // landing, and asserting between the two passes against a panel that
    // never drew anything.
    await waitFor(() => expect(screen.getByText('Precognition')).toBeTruthy());
  });

  it('draws no control for a setting the session does not have', async () => {
    serve({ [KNOBS]: () => ONE_UNSUPPORTED });
    render(() => <ModelSettingsSection />);

    await waitFor(() => expect(env.fetch.calls(KNOBS)).toBe(1));
    // The agent-options loop is never gated on the knob list, so waiting on it
    // means the absence below is a decision rather than a render that has yet
    // to happen.
    await waitFor(() => expect(env.fetch.calls(OPTIONS)).toBe(1));

    expect(screen.queryByText('Precognition')).toBeNull();
  });

  it('draws nothing rather than guessing when the answer lists nothing', async () => {
    // A daemon that answers but names no knob — an older one, or one whose
    // list grew a name this client does not know. A control drawn on a guess
    // is the bug this whole change is about, so absence hides it.
    //
    // Stated as an empty list rather than a failed call on purpose: a failed
    // call is hidden by the panel's error path, so it would pass with no
    // gating at all and prove nothing.
    serve({ [KNOBS]: () => ({ knobs: [] }) });
    render(() => <ModelSettingsSection />);

    // Wait for the agent-options loop, which is never gated on the knob list,
    // so the absence below is a decision and not a render that has yet to
    // happen. Asserting straight after the call was made passed against a
    // panel with no gating at all.
    await waitFor(() => expect(env.fetch.calls(OPTIONS)).toBe(1));

    expect(screen.queryByText('Precognition')).toBeNull();
  });
});

/**
 * The agent's own settings. Crucible has no knob for these — a different
 * agent advertises different ones — so the panel renders what it is given
 * rather than a set of named rows. That generality is the whole point: an
 * option this file has never heard of has to draw and work.
 */
describe('ModelSettingsSection agent options', () => {
  const REASONING = {
    id: 'thought_level',
    name: 'Reasoning',
    description: 'How long the agent thinks',
    category: 'thought_level',
    kind: 'select' as const,
    current: 'low',
    choices: [
      { value: 'low', name: 'Low' },
      { value: 'high', name: 'High' },
    ],
  };

  it('draws an option it has never heard of, and sends the choice back', async () => {
    let sent: { option_id: string; value: string } | null = null;
    serve({
      [OPTIONS]: () => ({ session_id: 's1', options: [REASONING] }),
      [SET_OPTION]: async (request) => {
        sent = (await request.json()) as { option_id: string; value: string };
        return new Response(null, { status: 204 });
      },
    });
    render(() => <ModelSettingsSection />);

    await waitFor(() => expect(screen.getByText('Reasoning')).toBeTruthy());
    const select = screen.getByTestId('agent-option-thought_level') as HTMLSelectElement;
    expect(select.value).toBe('low');
    expect(Array.from(select.options).map((o) => o.value)).toEqual(['low', 'high']);

    select.value = 'high';
    select.dispatchEvent(new Event('change', { bubbles: true }));

    await waitFor(() => expect(sent).toEqual({ option_id: 'thought_level', value: 'high' }));
    // The agent is the authority on what the value became, so the list is
    // re-read rather than patched.
    await waitFor(() => expect(env.fetch.calls(OPTIONS)).toBe(2));
  });

  it('draws a toggle for a boolean option', async () => {
    serve({
      [OPTIONS]: () => ({
        session_id: 's1',
        options: [
          { id: 'verbose', name: 'Verbose', description: null, category: null, kind: 'toggle', current: false },
        ],
      }),
    });
    render(() => <ModelSettingsSection />);

    await waitFor(() => expect(screen.getByTestId('agent-option-verbose')).toBeTruthy());
  });

  it('draws nothing when the agent advertised nothing', async () => {
    serve();
    render(() => <ModelSettingsSection />);

    // Anchor on an ungated row so the absence is a decision, not a pending
    // render.
    await waitFor(() => expect(screen.getByText('Precognition')).toBeTruthy());
    expect(screen.queryByText('Reasoning')).toBeNull();
  });
});
