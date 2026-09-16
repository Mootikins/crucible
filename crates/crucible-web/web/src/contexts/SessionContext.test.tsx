import { render, screen, waitFor } from '@solidjs/testing-library';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { useSessionSafe, useSession, SessionProvider } from './SessionContext';
import { apiError, type MockFetchAnswer } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { resetSessionsForTests } from '@/lib/query/sessions';
import { statusBarActions } from '@/stores/statusBarStore';
import type { Session } from '@/lib/types';

// No `vi.mock('@/lib/api')`. The context reads the daemon through
// `lib/query/sessions.ts` now, so each case answers ROUTES: that is what
// proves the context asks for the right thing, and it is the only way to
// count the requests two readers of one list make between them.

const LIST = 'GET /api/session/list';
const PROVIDERS = 'GET /api/providers';

/** One session as the daemon sends it, which `lib/api.ts` maps. */
function wire(over: Partial<Session> & { id: string }): Record<string, unknown> {
  return {
    session_id: over.id,
    type: over.type ?? 'chat',
    kilns: over.kilns ?? ['main'],
    workspace: over.workspace ?? '/repos/app',
    state: over.state ?? 'active',
    title: over.title ?? 'Test Session',
    agent_model: over.agent_model ?? null,
    agent: over.agent ? { mode: over.agent } : null,
    started_at: over.started_at ?? '2026-09-15T00:00:00Z',
    last_activity: over.last_activity ?? null,
    event_count: over.event_count ?? 0,
    archived: over.archived ?? false,
  };
}

/** The list reply around a set of rows. */
function listOf(rows: Array<Partial<Session> & { id: string }>): Record<string, unknown> {
  return { sessions: rows.map(wire), total: rows.length };
}

let env: TestQueryEnv;

/** Installs a fresh cache and a fetch that answers only what a case names. */
function serve(routes: Record<string, MockFetchAnswer>): TestQueryEnv {
  env = createTestQueryEnv({
    [LIST]: () => listOf([]),
    [PROVIDERS]: () => ({ providers: [] }),
    ...routes,
  });
  return env;
}

beforeEach(() => {
  resetSessionsForTests();
  statusBarActions.setActiveSessionId(null);
  localStorage.removeItem('crucible:cache:sessions');
});

afterEach(() => {
  env?.restore();
  statusBarActions.setActiveSessionId(null);
  localStorage.removeItem('crucible:cache:sessions');
  resetSessionsForTests();
});

describe('useSessionSafe', () => {
  function SafeTestConsumer() {
    const { currentSession, sessions, isLoading, availableModels } = useSessionSafe();

    return (
      <div>
        <span data-testid="session">{currentSession() ? 'has-session' : 'no-session'}</span>
        <span data-testid="sessions-count">{sessions().length}</span>
        <span data-testid="loading">{isLoading() ? 'loading' : 'idle'}</span>
        <span data-testid="models-count">{availableModels().length}</span>
      </div>
    );
  }

  it('returns fallback values when used outside provider', () => {
    serve({});
    render(() => <SafeTestConsumer />);

    expect(screen.getByTestId('session').textContent).toBe('no-session');
    expect(screen.getByTestId('sessions-count').textContent).toBe('0');
    expect(screen.getByTestId('loading').textContent).toBe('idle');
    expect(screen.getByTestId('models-count').textContent).toBe('0');
  });

  it('noop methods do not throw when called outside provider', async () => {
    serve({});
    function ActionTestConsumer() {
      const { selectSession, pauseSession, resumeSession } = useSessionSafe();

      return (
        <div>
          <button onClick={() => selectSession('test-id')}>Select</button>
          <button onClick={() => pauseSession()}>Pause</button>
          <button onClick={() => resumeSession()}>Resume</button>
        </div>
      );
    }

    render(() => <ActionTestConsumer />);

    expect(() => screen.getByText('Select').click()).not.toThrow();
    expect(() => screen.getByText('Pause').click()).not.toThrow();
    expect(() => screen.getByText('Resume').click()).not.toThrow();
  });
});

describe('selectSession auto-resume', () => {
  function SelectConsumer() {
    const { selectSession } = useSession();
    return (
      <button data-testid="select" onClick={() => void selectSession('test-id')}>Select</button>
    );
  }

  let dispatchSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    dispatchSpy = vi.spyOn(window, 'dispatchEvent');
  });

  afterEach(() => {
    dispatchSpy.mockRestore();
  });

  /** Serves one session by id, its models, and its resume route. */
  function serveSession(state: Session['state'], extra: Record<string, MockFetchAnswer> = {}) {
    return serve({
      'GET /api/session/test-id': () => wire({ id: 'test-id', state }),
      'GET /api/session/test-id/models': () => ({ models: [] }),
      'POST /api/session/test-id/resume': () => new Response(null, { status: 204 }),
      ...extra,
    });
  }

  function mount() {
    render(() => (
      <SessionProvider initialKiln="/tmp/test-kiln">
        <SelectConsumer />
      </SessionProvider>
    ));
  }

  it('calls resumeSession for paused sessions', async () => {
    const served = serveSession('paused');
    mount();
    await waitFor(() => expect(served.fetch.calls(LIST)).toBe(1));

    screen.getByTestId('select').click();

    await waitFor(() =>
      expect(served.fetch.calls('POST /api/session/test-id/resume')).toBe(1),
    );
    expect(dispatchSpy).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'crucible:open-session' })
    );
  });

  it('does not call resumeSession for active sessions', async () => {
    const served = serveSession('active');
    mount();

    screen.getByTestId('select').click();

    await waitFor(() => {
      expect(dispatchSpy).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'crucible:open-session' })
      );
    });

    expect(served.fetch.calls('POST /api/session/test-id/resume')).toBe(0);
  });

  it('transparently resumes ended sessions on select', async () => {
    // Ended sessions are always resumable: selecting one revives it (the daemon
    // resume route falls back to storage) so the composer is never a dead end.
    const served = serveSession('ended');
    mount();

    screen.getByTestId('select').click();

    await waitFor(() =>
      expect(served.fetch.calls('POST /api/session/test-id/resume')).toBe(1),
    );
    expect(dispatchSpy).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'crucible:open-session' })
    );
  });

  it('opens a stored session from the list with one read, no history page first', async () => {
    // The daemon answers `session.get` for a session it holds in storage
    // only. The client used to read one history page to revive it first.
    const served = serveSession('active', { [LIST]: () => listOf([{ id: 'test-id' }]) });
    mount();
    await waitFor(() => expect(served.fetch.calls(LIST)).toBe(1));

    screen.getByTestId('select').click();

    await waitFor(() => expect(served.fetch.calls('GET /api/session/test-id')).toBe(1));
    expect(served.fetch.calls('GET /api/session/test-id/history')).toBe(0);

    await waitFor(() => {
      expect(dispatchSpy).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'crucible:open-session' })
      );
    });
  });

  it('drops a row the daemon no longer holds instead of opening it', async () => {
    // The list can come from the seeded localStorage copy, and another tab may
    // have deleted the session since. A failed read prunes the dead row.
    const served = serve({
      [LIST]: () => listOf([{ id: 'test-id' }]),
      'GET /api/session/test-id': apiError(404, 'no such session'),
    });

    let context!: ReturnType<typeof useSession>;
    function Probe() {
      context = useSession();
      return <span data-testid="count">{context.sessions().length}</span>;
    }
    render(() => (
      <SessionProvider initialKiln="/tmp/test-kiln">
        <Probe />
      </SessionProvider>
    ));
    await waitFor(() => expect(screen.getByTestId('count').textContent).toBe('1'));

    await context.selectSession('test-id');

    await waitFor(() => expect(screen.getByTestId('count').textContent).toBe('0'));
    expect(served.fetch.calls('POST /api/session/test-id/resume')).toBe(0);
  });
});

describe('applySessionScope', () => {
  function ScopeConsumer() {
    const { createSession, applySessionScope, currentSession, sessions } = useSession();
    return (
      <div>
        <button data-testid="create" onClick={() => void createSession({ kilns: ['/kilns/main'] })}>
          create
        </button>
        <button
          data-testid="apply"
          onClick={() =>
            applySessionScope({
              session_id: 'new-id',
              kilns: ['/kilns/main', '/kilns/extra'],
              workspace: '/repos/app',
            })
          }
        >
          apply
        </button>
        <span data-testid="cur-workspace">{currentSession()?.workspace ?? '-'}</span>
        <span data-testid="cur-connected">{(currentSession()?.kilns ?? []).join(',')}</span>
        <span data-testid="list-workspace">{sessions()[0]?.workspace ?? '-'}</span>
        <span data-testid="list-connected">{(sessions()[0]?.kilns ?? []).join(',')}</span>
      </div>
    );
  }

  it('patches both currentSession and the matching sessions list entry', async () => {
    const created = { id: 'new-id', kilns: ['/kilns/main'], workspace: '/kilns/main' };
    // The list the daemon answers after the create carries the new row, so the
    // refetch that follows the write agrees with the optimistic prepend.
    let rows: Array<Partial<Session> & { id: string }> = [];
    const served = serve({
      [LIST]: () => listOf(rows),
      'POST /api/session': () => {
        rows = [created];
        return wire(created);
      },
      'GET /api/session/new-id/models': () => ({ models: [] }),
    });

    render(() => (
      <SessionProvider initialKiln="/kilns/main">
        <ScopeConsumer />
      </SessionProvider>
    ));
    await waitFor(() => expect(served.fetch.calls(LIST)).toBe(1));

    screen.getByTestId('create').click();
    await waitFor(() =>
      expect(screen.getByTestId('list-workspace').textContent).toBe('/kilns/main'),
    );
    // The selection lands when the write settles, which is after the list the
    // invalidation asked for.
    await waitFor(() =>
      expect(screen.getByTestId('cur-workspace').textContent).toBe('/kilns/main'),
    );

    screen.getByTestId('apply').click();

    await waitFor(() =>
      expect(screen.getByTestId('cur-workspace').textContent).toBe('/repos/app'),
    );
    expect(screen.getByTestId('cur-connected').textContent).toBe('/kilns/main,/kilns/extra');
    // The cached row reaches the DOM on the cache's own notification, which
    // is a microtask later than the selection's signal.
    await waitFor(() =>
      expect(screen.getByTestId('list-workspace').textContent).toBe('/repos/app'),
    );
    expect(screen.getByTestId('list-connected').textContent).toBe('/kilns/main,/kilns/extra');
  });
});

describe('createSession param forwarding', () => {
  function CreateConsumer(props: { run: (ctx: ReturnType<typeof useSession>) => void }) {
    const ctx = useSession();
    return (
      <button data-testid="run" onClick={() => props.run(ctx)}>
        run
      </button>
    );
  }

  async function renderWithRun(
    run: (ctx: ReturnType<typeof useSession>) => void,
    routes: Record<string, MockFetchAnswer> = {},
  ): Promise<{ sent: unknown[] }> {
    const sent: unknown[] = [];
    const served = serve({
      'POST /api/session': async (request) => {
        sent.push(await request.json());
        return wire({ id: 'new-id', title: null });
      },
      'POST /api/session/new-id/model': () => new Response(null, { status: 204 }),
      'GET /api/session/new-id/models': () => ({ models: [] }),
      ...routes,
    });

    render(() => (
      <SessionProvider initialKiln="/kilns/home">
        <CreateConsumer run={run} />
      </SessionProvider>
    ));
    await waitFor(() => expect(served.fetch.calls(LIST)).toBe(1));
    screen.getByTestId('run').click();
    return { sent };
  }

  it('forwards an internal create with the chosen kiln and applies the model', async () => {
    const { sent } = await renderWithRun((ctx) =>
      void ctx.createSession(
        { kilns: ['/kilns/main'] },
        { initialMessage: 'hi', model: 'openai/gpt-4o' },
      ),
    );

    await waitFor(() => expect(sent[0]).toEqual({ kilns: ['/kilns/main'] }));
    await waitFor(() =>
      expect(env.fetch.calls('POST /api/session/new-id/model')).toBe(1),
    );
  });

  it('forwards a kiln-less ACP create without a model override', async () => {
    const { sent } = await renderWithRun((ctx) =>
      void ctx.createSession(
        { agent_type: 'acp', agent_name: 'claude' },
        { initialMessage: 'refactor auth' },
      ),
    );

    await waitFor(() => expect(sent.length).toBe(1));
    const params = sent[0] as { agent_type?: string; agent_name?: string; kilns?: string[] };
    expect(params.agent_type).toBe('acp');
    expect(params.agent_name).toBe('claude');
    expect(params.kilns).toBeUndefined();
    expect(env.fetch.calls('POST /api/session/new-id/model')).toBe(0);
  });

  it('forwards every kiln in one flat set', async () => {
    const { sent } = await renderWithRun((ctx) =>
      void ctx.createSession(
        { kilns: ['/kilns/main', '/kilns/extra'] },
        { initialMessage: 'multi-kiln' },
      ),
    );

    await waitFor(() =>
      expect(sent[0]).toEqual({ kilns: ['/kilns/main', '/kilns/extra'] }),
    );
  });
});


describe('the model list of the selected session', () => {
  /// A slow answer for the session the user left must not land on the session
  /// they moved to.
  ///
  /// Regression: `refreshModels` wrote one signal for every session, so a slow
  /// earlier call resolved after a fast later one and clobbered the newer
  /// list. The user saw model options disappear right after appearing. The
  /// hand-written generation counter that guarded it is gone: the list is
  /// keyed by session id, so each answer lands on the key of the session that
  /// asked for it.
  it('keeps a late answer for the previous session off the current one', async () => {
    let releaseFirst: (() => void) | undefined;
    const first = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    serve({
      'GET /api/session/s-one': () => wire({ id: 's-one' }),
      'GET /api/session/s-two': () => wire({ id: 's-two' }),
      'GET /api/session/s-one/models': async () => {
        await first;
        return { models: ['stale-only'] };
      },
      'GET /api/session/s-two/models': () => ({ models: ['llama3.2', 'mistral'] }),
    });

    function Probe() {
      const ctx = useSession();
      return <span data-testid="models">{ctx.availableModels().join(',')}</span>;
    }
    render(() => (
      <SessionProvider>
        <Probe />
      </SessionProvider>
    ));

    // The shell points at one session, then at another, while the first
    // session's models are still in flight.
    statusBarActions.setActiveSessionId('s-one');
    await waitFor(() => expect(env.fetch.calls('GET /api/session/s-one/models')).toBe(1));
    statusBarActions.setActiveSessionId('s-two');
    await waitFor(() => {
      expect(screen.getByTestId('models').textContent).toBe('llama3.2,mistral');
    });

    // The earlier call completes now, with a different answer.
    releaseFirst?.();
    await waitFor(() => expect(env.fetch.calls('GET /api/session/s-one/models')).toBe(1));
    expect(screen.getByTestId('models').textContent).toBe('llama3.2,mistral');
  });

  /// The picker is chrome. A session whose agent declares no models of its own
  /// still offers the provider's list rather than an empty menu.
  it('falls back to the provider models when the session declares none', async () => {
    serve({
      'GET /api/session/s-one': () => wire({ id: 's-one' }),
      'GET /api/session/s-one/models': () => ({ models: [] }),
      [PROVIDERS]: () => ({
        providers: [
          {
            name: 'ollama',
            provider_type: 'ollama',
            available: true,
            default_model: 'ollama/llama3.2',
            models: ['ollama/llama3.2'],
            is_local: true,
          },
        ],
      }),
    });

    function Probe() {
      const ctx = useSession();
      return <span data-testid="models">{ctx.availableModels().join(',')}</span>;
    }
    render(() => (
      <SessionProvider>
        <Probe />
      </SessionProvider>
    ));

    statusBarActions.setActiveSessionId('s-one');

    await waitFor(() => {
      expect(screen.getByTestId('models').textContent).toBe('ollama/llama3.2');
    });
  });

  /// One probe answers the context and every other reader of the key.
  it('probes the providers once', async () => {
    serve({});

    function Probe() {
      const ctx = useSession();
      return <span data-testid="loaded">{ctx.providersLoaded() ? 'yes' : 'no'}</span>;
    }
    render(() => (
      <SessionProvider>
        <Probe />
      </SessionProvider>
    ));

    await waitFor(() => expect(screen.getByTestId('loaded').textContent).toBe('yes'));
    expect(env.fetch.calls(PROVIDERS)).toBe(1);
  });
});


/**
 * On reload, a chat pane is restored straight from the persisted layout and
 * bootstraps itself — it announces the session through
 * `statusBarStore.activeSessionId` but nothing calls `selectSession`. Without
 * adoption, `currentSession()` stays null and the composer sits disabled on
 * "Select a session first…" over a perfectly live session.
 */
describe('adopting the focused pane’s session', () => {
  function Probe() {
    const ctx = useSession();
    return <span data-testid="current">{ctx.currentSession()?.session_id ?? 'none'}</span>;
  }

  function mount() {
    render(() => (
      <SessionProvider>
        <Probe />
      </SessionProvider>
    ));
  }

  it('adopts a session announced by a restored pane', async () => {
    serve({
      'GET /api/session/s-restored': () => wire({ id: 's-restored' }),
      'GET /api/session/s-restored/models': () => ({ models: [] }),
    });

    mount();
    expect(screen.getByTestId('current').textContent).toBe('none');

    // What ChatContext's bootstrap does on reload.
    statusBarActions.setActiveSessionId('s-restored');

    await waitFor(() => {
      expect(screen.getByTestId('current').textContent).toBe('s-restored');
    });
  });

  it('follows a switch to another pane', async () => {
    serve({
      'GET /api/session/s-one': () => wire({ id: 's-one' }),
      'GET /api/session/s-two': () => wire({ id: 's-two' }),
      'GET /api/session/s-one/models': () => ({ models: [] }),
      'GET /api/session/s-two/models': () => ({ models: [] }),
    });
    mount();

    statusBarActions.setActiveSessionId('s-one');
    await waitFor(() => expect(screen.getByTestId('current').textContent).toBe('s-one'));

    statusBarActions.setActiveSessionId('s-two');
    await waitFor(() => expect(screen.getByTestId('current').textContent).toBe('s-two'));
  });

  it('does not refetch a session that is already current', async () => {
    const served = serve({
      'GET /api/session/s-same': () => wire({ id: 's-same' }),
      'GET /api/session/s-same/models': () => ({ models: [] }),
    });
    mount();

    statusBarActions.setActiveSessionId('s-same');
    await waitFor(() => expect(screen.getByTestId('current').textContent).toBe('s-same'));
    const calls = served.fetch.calls('GET /api/session/s-same');

    // A re-announce of the same id (pane refocus) must not thrash the network.
    statusBarActions.setActiveSessionId('s-same');
    await Promise.resolve();
    expect(served.fetch.calls('GET /api/session/s-same')).toBe(calls);
  });

  it('leaves the current session alone when the fetch fails', async () => {
    serve({ 'GET /api/session/s-broken': apiError(502, 'Bad Gateway') });
    mount();

    statusBarActions.setActiveSessionId('s-broken');
    await Promise.resolve();
    await Promise.resolve();
    // No throw, no bogus session — the pane surfaces its own load error.
    expect(screen.getByTestId('current').textContent).toBe('none');
  });
});

/**
 * A session opened from the rail, or restored on reload, must end with the
 * model picker holding the list the daemon answered — the same list a session
 * just created gets.
 */
describe('the model list of a session that was not just created', () => {
  function Probe(props: { onCtx: (c: ReturnType<typeof useSession>) => void }) {
    const ctx = useSession();
    props.onCtx(ctx);
    return (
      <>
        <span data-testid="current">{ctx.currentSession()?.session_id ?? 'none'}</span>
        <span data-testid="models">{ctx.availableModels().join(',')}</span>
      </>
    );
  }

  it('lists the daemon’s models after a rail click on an existing session', async () => {
    const served = serve({
      [LIST]: () => listOf([{ id: 's-old' }]),
      'GET /api/session/s-old': () => wire({ id: 's-old' }),
      'GET /api/session/s-old/models': () => ({ models: ['llama3.2', 'mistral'] }),
    });

    let ctx: ReturnType<typeof useSession> | undefined;
    render(() => (
      <SessionProvider initialKiln="/kilns/main">
        <Probe onCtx={(c) => (ctx = c)} />
      </SessionProvider>
    ));
    await waitFor(() => expect(ctx!.sessions().length).toBe(1));

    await ctx!.selectSession('s-old');

    expect(screen.getByTestId('current').textContent).toBe('s-old');
    expect(served.fetch.calls('GET /api/session/s-old/models')).toBeGreaterThan(0);
    expect(screen.getByTestId('models').textContent).toBe('llama3.2,mistral');
  });

  /**
   * Regression. On reload the restored pane announces its session while the
   * daemon holds it in storage only. `GET /api/session/{id}` used to answer
   * 422 for such a session until a history read revived it, so adoption gave
   * up, `currentSession()` stayed null, the composer sat disabled and the
   * model picker had nothing — for exactly the sessions a user comes back
   * to. The daemon now reads a stored session, and the client reads once.
   */
  it('adopts a stored session with one read, then lists its models', async () => {
    const served = serve({
      'GET /api/session/s-old': () => wire({ id: 's-old' }),
      'GET /api/session/s-old/models': () => ({ models: ['llama3.2', 'mistral'] }),
    });

    render(() => (
      <SessionProvider>
        <Probe onCtx={() => {}} />
      </SessionProvider>
    ));

    statusBarActions.setActiveSessionId('s-old');

    await waitFor(() => {
      expect(screen.getByTestId('current').textContent).toBe('s-old');
    });
    // One read of the record; no history page to bring it back first.
    expect(served.fetch.calls('GET /api/session/s-old')).toBe(1);
    expect(served.fetch.calls('GET /api/session/s-old/history')).toBe(0);
    await waitFor(() => {
      expect(screen.getByTestId('models').textContent).toBe('llama3.2,mistral');
    });
  });

  it('still leaves the current session alone when the session is really gone', async () => {
    serve({ 'GET /api/session/s-gone': apiError(404, 'Failed to load session') });

    render(() => (
      <SessionProvider>
        <Probe onCtx={() => {}} />
      </SessionProvider>
    ));

    statusBarActions.setActiveSessionId('s-gone');
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.getByTestId('current').textContent).toBe('none');
  });
});

/**
 * Two readers of the session list, one request.
 *
 * The rail's context and the inbox each used to fetch their own copy. They now
 * read one key, so mounting a second reader inside the provider asks the
 * daemon nothing.
 */
describe('the list every reader shares', () => {
  it('answers a second reader from the cache', async () => {
    const served = serve({ [LIST]: () => listOf([{ id: 's-1' }]) });

    function Reader(props: { id: string }) {
      const ctx = useSession();
      return <span data-testid={props.id}>{ctx.sessions().length}</span>;
    }
    render(() => (
      <SessionProvider initialKiln="/kilns/main">
        <Reader id="first" />
        <Reader id="second" />
      </SessionProvider>
    ));

    await waitFor(() => expect(screen.getByTestId('first').textContent).toBe('1'));
    expect(screen.getByTestId('second').textContent).toBe('1');
    expect(served.fetch.calls(LIST)).toBe(1);
  });

  it('paints the list this browser stored before the daemon answers', async () => {
    localStorage.setItem(
      'crucible:cache:sessions',
      JSON.stringify([
        { session_id: 's-stored', type: 'chat', kilns: [], workspace: null, state: 'active' },
      ]),
    );
    let release: (() => void) | undefined;
    const answered = new Promise<void>((resolve) => {
      release = resolve;
    });
    serve({
      [LIST]: async () => {
        await answered;
        return listOf([{ id: 's-live' }]);
      },
    });

    let ctx!: ReturnType<typeof useSession>;
    function Probe() {
      ctx = useSession();
      return <span data-testid="ids">{ctx.sessions().map((s) => s.session_id).join(',')}</span>;
    }
    render(() => (
      <SessionProvider initialKiln="/kilns/main">
        <Probe />
      </SessionProvider>
    ));

    expect(screen.getByTestId('ids').textContent).toBe('s-stored');

    release?.();
    await waitFor(() => expect(screen.getByTestId('ids').textContent).toBe('s-live'));
  });
});

describe('the mutations every reader shares', () => {
  function Probe(props: { onCtx: (c: ReturnType<typeof useSession>) => void }) {
    const ctx = useSession();
    props.onCtx(ctx);
    return <span data-testid="ids">{ctx.sessions().map((s) => s.session_id).join(',')}</span>;
  }

  async function mountWith(routes: Record<string, MockFetchAnswer>) {
    const served = serve({ [LIST]: () => listOf([{ id: 's-1' }, { id: 's-2' }]), ...routes });
    let ctx!: ReturnType<typeof useSession>;
    render(() => (
      <SessionProvider initialKiln="/kilns/main">
        <Probe onCtx={(c) => (ctx = c)} />
      </SessionProvider>
    ));
    await waitFor(() => expect(screen.getByTestId('ids').textContent).toBe('s-1,s-2'));
    return { served, ctx };
  }

  it('takes a deleted session out of the list without waiting for a refetch', async () => {
    let rows = [{ id: 's-1' }, { id: 's-2' }];
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    const served = serve({
      [LIST]: () => listOf(rows),
      'DELETE /api/session/s-2': () => {
        rows = [{ id: 's-1' }];
        return new Response(null, { status: 204 });
      },
    });
    let ctx!: ReturnType<typeof useSession>;
    render(() => (
      <SessionProvider initialKiln="/kilns/main">
        <Probe onCtx={(c) => (ctx = c)} />
      </SessionProvider>
    ));
    await waitFor(() => expect(screen.getByTestId('ids').textContent).toBe('s-1,s-2'));

    await ctx.deleteSession('s-2');

    expect(screen.getByTestId('ids').textContent).toBe('s-1');
    expect(served.fetch.calls('DELETE /api/session/s-2')).toBe(1);
    confirmSpy.mockRestore();
  });

  it('archives a session out of the default list', async () => {
    let rows = [{ id: 's-1' }, { id: 's-2' }];
    const { ctx } = await mountWith({
      [LIST]: () => listOf(rows),
      'POST /api/session/s-2/archive': () => {
        rows = [{ id: 's-1' }];
        return new Response(null, { status: 204 });
      },
    });

    await ctx.archiveSession('s-2');

    expect(screen.getByTestId('ids').textContent).toBe('s-1');
  });

  it('re-reads the list after an unarchive', async () => {
    let rows = [{ id: 's-1' }];
    const served = serve({
      [LIST]: () => listOf(rows),
      'POST /api/session/s-2/unarchive': () => {
        rows = [{ id: 's-1' }, { id: 's-2' }];
        return new Response(null, { status: 204 });
      },
    });
    let ctx!: ReturnType<typeof useSession>;
    render(() => (
      <SessionProvider initialKiln="/kilns/main">
        <Probe onCtx={(c) => (ctx = c)} />
      </SessionProvider>
    ));
    await waitFor(() => expect(screen.getByTestId('ids').textContent).toBe('s-1'));
    expect(served.fetch.calls(LIST)).toBe(1);

    await ctx.unarchiveSession('s-2');

    await waitFor(() => expect(screen.getByTestId('ids').textContent).toBe('s-1,s-2'));
    expect(served.fetch.calls(LIST)).toBe(2);
  });
});
