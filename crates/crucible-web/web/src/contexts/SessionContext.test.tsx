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
      state,
      title: 'Test Session',
      agent_model: 'test-model',
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

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let dispatchSpy: ReturnType<typeof vi.spyOn<any, any>>;

  beforeEach(() => {
    vi.clearAllMocks();
    (api.listSessions as ReturnType<typeof vi.fn>).mockResolvedValue([]);
    (api.getSessionHistory as ReturnType<typeof vi.fn>).mockResolvedValue({
      session_id: 'test-id',
      history: [],
      total_events: 0,
    });
    (api.listModels as ReturnType<typeof vi.fn>).mockResolvedValue([]);
    (api.listProviders as ReturnType<typeof vi.fn>).mockResolvedValue([]);
    (api.resumeSession as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    dispatchSpy = vi.spyOn(window, 'dispatchEvent');
  });

  afterEach(() => {
    dispatchSpy.mockRestore();
  });

  it('calls resumeSession for paused sessions', async () => {
    (api.getSession as ReturnType<typeof vi.fn>).mockResolvedValue(makeSession('paused'));

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
    (api.getSession as ReturnType<typeof vi.fn>).mockResolvedValue(makeSession('active'));

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
    (api.getSession as ReturnType<typeof vi.fn>).mockResolvedValue(makeSession('ended'));

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
    (api.listSessions as ReturnType<typeof vi.fn>).mockResolvedValue([makeSession('active')]);
    (api.getSession as ReturnType<typeof vi.fn>)
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
