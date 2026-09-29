import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';

// No `vi.mock('@/lib/api')`. The section reads and writes the strategy through
// `lib/query/session-config.ts`, so each case answers the ROUTE: that is what
// proves the value reaches the daemon under the name the dropdown shows.
//
// The `timeout_secs`-vs-`execution_timeout` wire asymmetry is asserted on the
// Rust side (`routes/session_config/tests.rs`); here the value only has to
// reach the right control and the right route.
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: () => ({ session_id: 's1' }),
  }),
}));

import { AdvancedSessionSettingsSection } from '../settings/AdvancedSessionSettings';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { MockFetchAnswer } from '@/test-utils/mock-fetch';

const GET = 'GET /api/session/s1/knob/context_strategy';
const SET_KNOB = 'PUT /api/session/s1/knob';
const APPROVALS = 'GET /api/session/s1/config/plugin-approvals';
const SET_APPROVAL = 'PUT /api/session/s1/config/plugins/alpha/approval';
const GET_LIMIT = 'GET /api/session/s1/knob/plugin_turn_limit';

let env: TestQueryEnv;

/** Installs a fresh cache and a fetch answering the strategy routes. */
function serve(routes: Record<string, MockFetchAnswer> = {}): TestQueryEnv {
  env = createTestQueryEnv({
    [GET]: () => ({ knob: 'context_strategy', value: 'recent' }),
    [SET_KNOB]: () => new Response(null, { status: 204 }),
    [APPROVALS]: () => ({ approvals: { alpha: 'inherit' } }),
    [SET_APPROVAL]: () => ({ success: true }),
    [GET_LIMIT]: () => ({ knob: 'plugin_turn_limit', value: 5 }),
    ...routes,
  });
  return env;
}

/** The section renders `<tr>`s, so it needs a table ancestor to mount into. */
function renderSection() {
  return render(() => (
    <table>
      <tbody>
        <AdvancedSessionSettingsSection />
      </tbody>
    </table>
  ));
}

afterEach(() => {
  env?.restore();
  vi.clearAllMocks();
});

describe('AdvancedSessionSettings', () => {
  it('shows and updates the session plugin turn limit', async () => {
    let sent: unknown;
    serve({ [SET_KNOB]: async (request) => {
      sent = await request.json();
      return { success: true };
    } });
    renderSection();
    const input = await screen.findByTestId('plugin-turn-limit');
    await waitFor(() => expect((input as HTMLInputElement).value).toBe('5'));
    fireEvent.change(input, { target: { value: '7' } });
    await waitFor(() => expect(sent).toEqual({ knob: 'plugin_turn_limit', value: 7 }));
  });
  it('shows loaded plugins and persists a stricter approval', async () => {
    let sent: unknown;
    serve({
      [SET_APPROVAL]: async (request) => {
        sent = await request.json();
        return { success: true };
      },
    });
    renderSection();
    const select = await screen.findByTestId('plugin-approval-alpha');
    expect((select as HTMLSelectElement).value).toBe('inherit');
    fireEvent.change(select, { target: { value: 'ask' } });
    await waitFor(() => expect(sent).toEqual({ approval: 'ask' }));
  });
  it('sends the enum knob by its string spelling', async () => {
    let sent: { knob: string; value: string } | null = null;
    serve({
      [SET_KNOB]: async (request) => {
        sent = (await request.json()) as { knob: string; value: string };
        return new Response(null, { status: 204 });
      },
    });
    renderSection();
    await waitFor(() => screen.getByTestId('context-strategy-select'));

    fireEvent.change(screen.getByTestId('context-strategy-select'), {
      target: { value: 'truncate' },
    });

    await waitFor(() => expect(sent).toEqual({ knob: 'context_strategy', value: 'truncate' }));
  });

  it('keeps a strategy name the dropdown does not know about', async () => {
    // The daemon owns the enum; a value it accepts must not vanish from the UI
    // because this file's convenience list is out of date. Nothing here
    // validates — the daemon answers 422 for a name it rejects.
    serve({ [GET]: () => ({ knob: 'context_strategy', value: 'some-future-strategy' }) });

    renderSection();

    await waitFor(() =>
      expect((screen.getByTestId('context-strategy-select') as HTMLSelectElement).value).toBe(
        'some-future-strategy',
      ),
    );
  });

  it('holds the strategy the daemon accepted, without a second read per panel', async () => {
    serve();
    renderSection();
    await waitFor(() =>
      expect((screen.getByTestId('context-strategy-select') as HTMLSelectElement).value).toBe(
        'recent',
      ),
    );

    fireEvent.change(screen.getByTestId('context-strategy-select'), {
      target: { value: 'summarize' },
    });

    await waitFor(() => expect(env.fetch.calls(SET_KNOB)).toBe(1));
    await waitFor(() =>
      expect((screen.getByTestId('context-strategy-select') as HTMLSelectElement).value).toBe(
        'summarize',
      ),
    );
  });
});
