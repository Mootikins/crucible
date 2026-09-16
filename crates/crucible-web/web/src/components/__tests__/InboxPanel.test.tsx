import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor } from '@solidjs/testing-library';
import InboxPanel from '../InboxPanel';
import { attentionStore, attentionActions } from '@/stores/attentionStore';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { MockFetchAnswer } from '@/test-utils/mock-fetch';
import { resetSessionsForTests } from '@/lib/query/sessions';
import { keys } from '@/lib/query/keys';
import { getBus } from '@/lib/bus';
import type { InteractionOf, Session } from '@/lib/types';

// No `vi.mock('@/lib/api')`. The panel reads the session list through
// `lib/query/sessions.ts`, so the list it draws and the list the rail draws
// are one cache entry; a mocked module would hide exactly that.

const LIST = 'GET /api/session/list';

const perm: InteractionOf<'permission'> = {
  kind: 'permission',
  id: 'req-42',
  action_type: 'bash',
  tokens: ['cargo', 'test', '--package', 'helios-core'],
  tool_name: 'Bash',
};

/** One session as the daemon sends it. */
function wire(id: string, archived: boolean): Record<string, unknown> {
  return {
    session_id: id,
    type: 'chat',
    kilns: ['main'],
    workspace: '/repos/app',
    state: 'active',
    title: `Session ${id}`,
    agent_model: null,
    agent: null,
    started_at: '2026-09-15T00:00:00Z',
    last_activity: '2026-09-15T00:00:00Z',
    event_count: 0,
    archived,
  };
}

function clearAttention() {
  for (const id of Object.keys(attentionStore.entries)) {
    attentionActions.clear(id);
  }
}

let env: TestQueryEnv;

function serve(routes: Record<string, MockFetchAnswer> = {}): TestQueryEnv {
  env = createTestQueryEnv({
    [LIST]: () => ({ sessions: [], total: 0 }),
    'POST /api/interaction/respond': () => new Response(null, { status: 204 }),
    'GET /api/interactions/pending': () => ({ pending: [] }),
    ...routes,
  });
  return env;
}

beforeEach(() => {
  clearAttention();
  resetSessionsForTests();
  localStorage.removeItem('crucible:cache:sessions');
});

afterEach(() => {
  cleanup();
  clearAttention();
  env?.restore();
  resetSessionsForTests();
  localStorage.removeItem('crucible:cache:sessions');
  vi.clearAllMocks();
});

describe('InboxPanel', () => {
  it('shows all-clear when nothing is pending', () => {
    serve();
    const { getByText } = render(() => <InboxPanel />);
    expect(getByText(/all clear/)).toBeTruthy();
    expect(getByText(/0 pending/)).toBeTruthy();
  });

  it('renders a pending permission answerable in place', () => {
    serve();
    attentionActions.report('s1', {
      pendingInteraction: perm,
      title: 'scheduler-backpressure',
    });

    const { getByText, queryByText } = render(() => <InboxPanel />);
    expect(getByText('scheduler-backpressure')).toBeTruthy();
    expect(getByText(/cargo test --package helios-core/)).toBeTruthy();
    expect(getByText(/1 pending/)).toBeTruthy();
    expect(queryByText(/all clear/)).toBeNull();
  });

  it('responds via the API and announces the resolution on Allow', async () => {
    let sent: unknown = null;
    const served = serve({
      'POST /api/interaction/respond': async (request) => {
        sent = await request.json();
        return new Response(null, { status: 204 });
      },
    });
    attentionActions.report('s1', {
      pendingInteraction: perm,
      title: 'scheduler-backpressure',
    });
    // The bus, not a window CustomEvent: the write announces the answer, and
    // the pane holding the card listens on the same bus.
    const announced: Array<{ sessionId: string; requestId: string }> = [];
    const off = getBus().on('interactionResolved', (payload) => announced.push(payload));

    const { getByText } = render(() => <InboxPanel />);
    (getByText('Allow') as HTMLElement).click();

    await waitFor(() => {
      expect(sent).toMatchObject({
        session_id: 's1',
        request_id: 'req-42',
        response: { allowed: true },
      });
    });
    expect(served.fetch.calls('POST /api/interaction/respond')).toBe(1);
    expect(announced).toEqual([{ sessionId: 's1', requestId: 'req-42' }]);
    // Entry resolved locally: badge drops, resolved note shows.
    await waitFor(() => {
      expect(attentionStore.attentionCount()).toBe(0);
      expect(getByText(/Resolved — scheduler-backpressure/)).toBeTruthy();
    });

    off();
  });

  it('takes the answered request out of the shared pending list', async () => {
    // The daemon's aggregate lags the answer by up to one poll. The list the
    // badge and every chat pane read must not wait for it.
    const served = serve({
      'GET /api/interactions/pending': () => ({
        pending: [{ session_id: 's1', request_id: 'req-42', request: perm }],
      }),
    });
    await attentionActions.refresh();
    attentionActions.report('s1', { pendingInteraction: perm, title: 'scheduler' });

    const { getByText } = render(() => <InboxPanel />);
    (getByText('Allow') as HTMLElement).click();

    await waitFor(() =>
      expect(served.client.getQueryData(keys.pendingInteractions())).toEqual([]),
    );
    // And nothing asked the daemon for the aggregate again.
    expect(served.fetch.calls('GET /api/interactions/pending')).toBe(1);
  });
});

/**
 * The panel used to fetch its own copy of the list and refetch only that copy.
 * It now reads the one key the rail reads, and the disclosure below chooses
 * which variant of it.
 */
describe('the list the inbox shares with the rail', () => {
  it('draws the recent rows the rail already fetched, and asks for nothing', async () => {
    const served = serve({ [LIST]: () => ({ sessions: [wire('s-1', false)], total: 1 }) });
    // What the rail's own reader put there.
    served.client.setQueryData(keys.sessions(false), [
      {
        session_id: 's-1',
        type: 'chat',
        kilns: ['main'],
        workspace: '/repos/app',
        state: 'active',
        title: 'Session s-1',
        agent_model: null,
        started_at: '2026-09-15T00:00:00Z',
        last_activity: '2026-09-15T00:00:00Z',
        event_count: 0,
        archived: false,
      } satisfies Session,
    ]);

    const { getByText } = render(() => <InboxPanel />);

    expect(getByText('Session s-1')).toBeTruthy();
    expect(served.fetch.calls(LIST)).toBe(0);
  });

  it('asks the daemon the wider question when the archived section opens', async () => {
    const served = serve({
      [LIST]: (request) =>
        new URL(request.url).searchParams.get('include_archived') === 'true'
          ? { sessions: [wire('s-1', false), wire('s-old', true)], total: 2 }
          : { sessions: [wire('s-1', false)], total: 1 },
    });

    const { getByTestId, getByText } = render(() => <InboxPanel />);
    await waitFor(() => expect(served.fetch.calls(LIST)).toBe(1));

    getByTestId('archived-toggle').click();

    await waitFor(() => expect(getByText('Session s-old')).toBeTruthy());
    expect(served.fetch.calls(LIST)).toBe(2);
  });

  it('deletes an archived session out of the shared list, with no refetch of its own', async () => {
    let rows = [wire('s-1', false), wire('s-old', true)];
    const served = serve({
      [LIST]: () => ({ sessions: rows, total: rows.length }),
      'DELETE /api/session/s-old': () => {
        rows = [wire('s-1', false)];
        return new Response(null, { status: 204 });
      },
    });

    const { getByTestId, getByText, queryByText } = render(() => <InboxPanel />);
    getByTestId('archived-toggle').click();
    await waitFor(() => expect(getByText('Session s-old')).toBeTruthy());

    // The row arms on the first click and deletes on the second.
    (getByText('DELETE') as HTMLElement).click();
    (getByText('SURE?') as HTMLElement).click();

    await waitFor(() => expect(queryByText('Session s-old')).toBeNull());
    expect(served.fetch.calls('DELETE /api/session/s-old')).toBe(1);
  });

  it('restores an archived session through the shared mutation', async () => {
    let rows = [wire('s-old', true)];
    const served = serve({
      [LIST]: () => ({ sessions: rows, total: rows.length }),
      'POST /api/session/s-old/unarchive': () => {
        rows = [wire('s-old', false)];
        return new Response(null, { status: 204 });
      },
    });

    const { getByTestId, getByText } = render(() => <InboxPanel />);
    getByTestId('archived-toggle').click();
    await waitFor(() => expect(getByText('RESTORE')).toBeTruthy());

    (getByText('RESTORE') as HTMLElement).click();

    await waitFor(() =>
      expect(served.fetch.calls('POST /api/session/s-old/unarchive')).toBe(1),
    );
    // The row is back in the recent list, which is the same list.
    await waitFor(() => expect(getByText(/1 recent sessions/)).toBeTruthy());
  });
});
