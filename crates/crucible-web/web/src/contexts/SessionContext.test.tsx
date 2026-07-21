import { render, screen, waitFor } from '@solidjs/testing-library';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { useSessionSafe, useSession, SessionProvider } from './SessionContext';
import * as api from '@/lib/api';
import type { Session } from '@/lib/types';

vi.mock('@/lib/api', () => ({
  createSession: vi.fn(),
  listSessions: vi.fn(() => Promise.resolve([])),
  getSession: vi.fn(),
  getSessionHistory: vi.fn(),
  pauseSession: vi.fn(),
  resumeSession: vi.fn(),
  endSession: vi.fn(),
  cancelSession: vi.fn(),
  listModels: vi.fn(() => Promise.resolve([])),
  switchModel: vi.fn(),
  setSessionTitle: vi.fn(),
  listProviders: vi.fn(() => Promise.resolve([])),
}));

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
    render(() => <SafeTestConsumer />);

    expect(screen.getByTestId('session').textContent).toBe('no-session');
    expect(screen.getByTestId('sessions-count').textContent).toBe('0');
    expect(screen.getByTestId('loading').textContent).toBe('idle');
    expect(screen.getByTestId('models-count').textContent).toBe('0');
  });

  it('noop methods do not throw when called outside provider', async () => {
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
  function makeSession(state: Session['state'], id = 'test-id'): Session {
    return {
      id,
      session_type: 'chat',
      kiln: '/tmp/test-kiln',
      workspace: '/tmp/test-workspace',
      connected_kilns: [],
      state,
      title: 'Test Session',
      agent_model: 'test-model',
      agent_mode: null,
      started_at: new Date().toISOString(),
      event_count: 0,
    };
  }

  function SelectConsumer() {
    const { selectSession } = useSession();
    return (
      <button data-testid="select" onClick={() => void selectSession('test-id')}>Select</button>
    );
  }

  let dispatchSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.clearAllMocks();
    (api.listSessions as ReturnType<typeof vi.fn<any>>).mockResolvedValue([]);
    (api.getSessionHistory as ReturnType<typeof vi.fn<any>>).mockResolvedValue({
      session_id: 'test-id',
      history: [],
      total_events: 0,
    });
    (api.listModels as ReturnType<typeof vi.fn<any>>).mockResolvedValue([]);
    (api.listProviders as ReturnType<typeof vi.fn<any>>).mockResolvedValue([]);
    (api.resumeSession as ReturnType<typeof vi.fn<any>>).mockResolvedValue(undefined);
    dispatchSpy = vi.spyOn(window, 'dispatchEvent');
  });

  afterEach(() => {
    dispatchSpy.mockRestore();
  });

  it('calls resumeSession for paused sessions', async () => {
    (api.getSession as ReturnType<typeof vi.fn<any>>).mockResolvedValue(makeSession('paused'));

    render(() => (
      <SessionProvider initialKiln="/tmp/test-kiln">
        <SelectConsumer />
      </SessionProvider>
    ));

    await waitFor(() => {
      expect(api.listSessions).toHaveBeenCalled();
    });

    screen.getByTestId('select').click();

    await waitFor(() => {
      expect(api.resumeSession).toHaveBeenCalledWith('test-id');
    });

    expect(dispatchSpy).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'crucible:open-session' })
    );
  });

  it('does not call resumeSession for active sessions', async () => {
    (api.getSession as ReturnType<typeof vi.fn<any>>).mockResolvedValue(makeSession('active'));

    render(() => (
      <SessionProvider initialKiln="/tmp/test-kiln">
        <SelectConsumer />
      </SessionProvider>
    ));

    screen.getByTestId('select').click();

    await waitFor(() => {
      expect(dispatchSpy).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'crucible:open-session' })
      );
    });

    expect(api.resumeSession).not.toHaveBeenCalled();
  });

  it('does not call resumeSession for ended sessions', async () => {
    (api.getSession as ReturnType<typeof vi.fn<any>>).mockResolvedValue(makeSession('ended'));

    render(() => (
      <SessionProvider initialKiln="/tmp/test-kiln">
        <SelectConsumer />
      </SessionProvider>
    ));

    screen.getByTestId('select').click();

    await waitFor(() => {
      expect(dispatchSpy).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'crucible:open-session' })
      );
    });

    expect(api.resumeSession).not.toHaveBeenCalled();
  });

  it('loads persisted session into daemon when selecting from existing list', async () => {
    (api.listSessions as ReturnType<typeof vi.fn<any>>).mockResolvedValue([makeSession('active')]);
    (api.getSession as ReturnType<typeof vi.fn<any>>)
      .mockRejectedValueOnce(new Error('Session not found'))
      .mockResolvedValue(makeSession('active'));

    render(() => (
      <SessionProvider initialKiln="/tmp/test-kiln">
        <SelectConsumer />
      </SessionProvider>
    ));

    screen.getByTestId('select').click();

    await waitFor(() => {
      expect(api.getSessionHistory).toHaveBeenCalledWith('test-id', '/tmp/test-kiln', 1, 0);
    });

    await waitFor(() => {
      expect(dispatchSpy).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'crucible:open-session' })
      );
    });
  });
});

function makeChatSession(id = 'new-id'): Session {
  return {
    id,
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
  };
}

describe('applySessionScope', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (api.listSessions as ReturnType<typeof vi.fn<any>>).mockResolvedValue([]);
    (api.listModels as ReturnType<typeof vi.fn<any>>).mockResolvedValue([]);
    (api.listProviders as ReturnType<typeof vi.fn<any>>).mockResolvedValue([]);
    (api.createSession as ReturnType<typeof vi.fn<any>>).mockResolvedValue(makeChatSession());
  });

  function ScopeConsumer() {
    const { createSession, applySessionScope, currentSession, sessions } = useSession();
    return (
      <div>
        <button data-testid="create" onClick={() => void createSession({ kiln: '/kilns/main' })}>
          create
        </button>
        <button
          data-testid="apply"
          onClick={() =>
            applySessionScope({
              session_id: 'new-id',
              kiln: '/kilns/main',
              workspace: '/repos/app',
              connected_kilns: ['/kilns/extra'],
            })
          }
        >
          apply
        </button>
        <span data-testid="cur-workspace">{currentSession()?.workspace ?? '-'}</span>
        <span data-testid="cur-connected">
          {(currentSession()?.connected_kilns ?? []).join(',')}
        </span>
        <span data-testid="list-workspace">{sessions()[0]?.workspace ?? '-'}</span>
        <span data-testid="list-connected">
          {(sessions()[0]?.connected_kilns ?? []).join(',')}
        </span>
      </div>
    );
  }

  it('patches both currentSession and the matching sessions list entry', async () => {
    render(() => (
      <SessionProvider initialKiln="/kilns/main">
        <ScopeConsumer />
      </SessionProvider>
    ));
    await waitFor(() => expect(api.listSessions).toHaveBeenCalled());

    screen.getByTestId('create').click();
    await waitFor(() =>
      expect(screen.getByTestId('list-workspace').textContent).toBe('/kilns/main'),
    );

    screen.getByTestId('apply').click();

    await waitFor(() =>
      expect(screen.getByTestId('cur-workspace').textContent).toBe('/repos/app'),
    );
    expect(screen.getByTestId('cur-connected').textContent).toBe('/kilns/extra');
    expect(screen.getByTestId('list-workspace').textContent).toBe('/repos/app');
    expect(screen.getByTestId('list-connected').textContent).toBe('/kilns/extra');
  });
});

describe('createSession param forwarding', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (api.listSessions as ReturnType<typeof vi.fn<any>>).mockResolvedValue([]);
    (api.listModels as ReturnType<typeof vi.fn<any>>).mockResolvedValue([]);
    (api.listProviders as ReturnType<typeof vi.fn<any>>).mockResolvedValue([]);
    (api.createSession as ReturnType<typeof vi.fn<any>>).mockResolvedValue(makeChatSession());
    (api.switchModel as ReturnType<typeof vi.fn<any>>).mockResolvedValue(undefined);
  });

  function CreateConsumer(props: { run: (ctx: ReturnType<typeof useSession>) => void }) {
    const ctx = useSession();
    return (
      <button data-testid="run" onClick={() => props.run(ctx)}>
        run
      </button>
    );
  }

  async function renderWithRun(run: (ctx: ReturnType<typeof useSession>) => void) {
    render(() => (
      <SessionProvider initialKiln="/kilns/home">
        <CreateConsumer run={run} />
      </SessionProvider>
    ));
    await waitFor(() => expect(api.listSessions).toHaveBeenCalled());
    screen.getByTestId('run').click();
  }

  it('forwards an internal create with the chosen kiln and applies the model', async () => {
    await renderWithRun((ctx) =>
      void ctx.createSession(
        { kiln: '/kilns/main' },
        { initialMessage: 'hi', model: 'openai/gpt-4o' },
      ),
    );

    await waitFor(() =>
      expect(api.createSession).toHaveBeenCalledWith({ kiln: '/kilns/main' }),
    );
    await waitFor(() =>
      expect(api.switchModel).toHaveBeenCalledWith('new-id', 'openai/gpt-4o'),
    );
  });

  it('forwards a kiln-less ACP create without a model override', async () => {
    await renderWithRun((ctx) =>
      void ctx.createSession(
        { agent_type: 'acp', agent_name: 'claude' },
        { initialMessage: 'refactor auth' },
      ),
    );

    await waitFor(() => expect(api.createSession).toHaveBeenCalledTimes(1));
    const params = (api.createSession as ReturnType<typeof vi.fn<any>>).mock.calls[0][0] as {
      agent_type?: string;
      agent_name?: string;
      kiln?: string;
    };
    expect(params.agent_type).toBe('acp');
    expect(params.agent_name).toBe('claude');
    expect(params.kiln).toBeUndefined();
    expect(api.switchModel).not.toHaveBeenCalled();
  });

  it('forwards additional kilns as connect_kilns', async () => {
    await renderWithRun((ctx) =>
      void ctx.createSession(
        { kiln: '/kilns/main', connect_kilns: ['/kilns/extra'] },
        { initialMessage: 'multi-kiln' },
      ),
    );

    await waitFor(() =>
      expect(api.createSession).toHaveBeenCalledWith({
        kiln: '/kilns/main',
        connect_kilns: ['/kilns/extra'],
      }),
    );
  });
});
