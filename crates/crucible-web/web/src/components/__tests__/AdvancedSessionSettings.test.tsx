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
    currentSession: () => ({ id: 's1' }),
  }),
}));

import { AdvancedSessionSettingsSection } from '../settings/AdvancedSessionSettings';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { MockFetchAnswer } from '@/test-utils/mock-fetch';

const GET = 'GET /api/session/s1/config/context-strategy';
const SET = 'PUT /api/session/s1/config/context-strategy';

let env: TestQueryEnv;

/** Installs a fresh cache and a fetch answering the strategy routes. */
function serve(routes: Record<string, MockFetchAnswer> = {}): TestQueryEnv {
  env = createTestQueryEnv({
    [GET]: () => ({ context_strategy: 'recent' }),
    [SET]: () => new Response(null, { status: 204 }),
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
  it('sends the enum knob by its string spelling', async () => {
    let sent: { context_strategy: string } | null = null;
    serve({
      [SET]: async (request) => {
        sent = (await request.json()) as { context_strategy: string };
        return new Response(null, { status: 204 });
      },
    });
    renderSection();
    await waitFor(() => screen.getByTestId('context-strategy-select'));

    fireEvent.change(screen.getByTestId('context-strategy-select'), {
      target: { value: 'truncate' },
    });

    await waitFor(() => expect(sent).toEqual({ context_strategy: 'truncate' }));
  });

  it('keeps a strategy name the dropdown does not know about', async () => {
    // The daemon owns the enum; a value it accepts must not vanish from the UI
    // because this file's convenience list is out of date. Nothing here
    // validates — the daemon answers 422 for a name it rejects.
    serve({ [GET]: () => ({ context_strategy: 'some-future-strategy' }) });

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

    await waitFor(() => expect(env.fetch.calls(SET)).toBe(1));
    await waitFor(() =>
      expect((screen.getByTestId('context-strategy-select') as HTMLSelectElement).value).toBe(
        'summarize',
      ),
    );
  });
});
