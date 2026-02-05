import { render, screen } from '@solidjs/testing-library';
import { describe, it, expect, vi } from 'vitest';
import { useSessionSafe } from './SessionContext';

vi.mock('@/lib/api', () => ({
  createSession: vi.fn(),
  listSessions: vi.fn(() => Promise.resolve([])),
  getSession: vi.fn(),
  pauseSession: vi.fn(),
  resumeSession: vi.fn(),
  endSession: vi.fn(),
  cancelSession: vi.fn(),
  listModels: vi.fn(() => Promise.resolve([])),
  switchModel: vi.fn(),
  setSessionTitle: vi.fn(),
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
