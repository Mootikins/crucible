import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import type { ConnectionStatus } from '@/lib/types';

/**
 * The composer used to show a red `Reconnecting…` strip with no way to act on
 * it, while the terminal — same class of fault — carried a working retry. It
 * now carries the same banner, and the transport fault is held apart from a
 * daemon-side error so the retry is only offered where it can do something.
 */

const [connectionStatus, setConnectionStatus] = createSignal<ConnectionStatus>('connected');
const [error, setError] = createSignal<string | null>(null);
const mockRetryConnection = vi.fn();

vi.mock('@/contexts/ChatContext', () => ({
  useChatSafe: () => ({
    sendMessage: vi.fn(),
    isLoading: () => false,
    isStreaming: () => false,
    cancelStream: vi.fn(),
    error,
    connectionStatus,
    retryConnection: mockRetryConnection,
    chatMode: () => 'ask',
    setChatMode: vi.fn(),
    switchMode: vi.fn(),
    sessionId: () => 'test-session',
    addSystemMessage: vi.fn(),
    clearMessages: vi.fn(),
    subagentEvents: () => [],
    pendingInteraction: () => null,
    respondToInteraction: vi.fn(),
  }),
}));

vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({
    currentSession: () => ({
      id: 'test-session',
      state: 'active',
      kilns: ['/tmp/test-kiln'],
      workspace: null,
      agent_model: 'test-model',
    }),
    cancelCurrentOperation: vi.fn(),
    availableModels: () => ['model-1'],
    switchModel: vi.fn(),
    refreshModels: vi.fn(),
    selectedProvider: () => ({ provider_type: 'ollama' }),
    applySessionScope: vi.fn(),
  }),
}));

vi.mock('@/hooks/useMediaRecorder', () => ({
  useMediaRecorder: () => ({
    isRecording: () => false,
    audioLevel: () => 0,
    startRecording: vi.fn(),
    stopRecording: vi.fn(),
  }),
}));

vi.mock('@/hooks/useAutocomplete', () => ({
  useAutocomplete: () => ({
    isOpen: () => false,
    items: () => [],
    selectedIndex: () => -1,
    onInput: vi.fn(),
    onKeyDown: vi.fn(),
    complete: vi.fn(),
  }),
}));

vi.mock('../MicButton', () => ({ MicButton: () => <div /> }));
vi.mock('../ChatModeControl', () => ({
  ChatModeControl: () => <div />,
  nextChatMode: (m: string) => m,
}));
vi.mock('../AutocompletePopup', () => ({ AutocompletePopup: () => <div /> }));

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  listProjects: vi.fn(async () => []),
  connectSessionKiln: vi.fn(),
  disconnectSessionKiln: vi.fn(),
  setSessionWorkspace: vi.fn(),
  getSessionStatus: vi.fn(async () => []),
  listModes: vi.fn(async () => ({ current_mode_id: 'ask', modes: [] })),
  subscribeToEvents: vi.fn(() => () => {}),
}));

vi.mock('@/lib/review-api', () => ({
  listReviewHunks: vi.fn(async () => ({ session_id: 'test-session', hunks: [], comments: [] })),
}));

const { ChatInput } = await import('../ChatInput');
const { createTestQueryEnv } = await import('@/test-utils/query');
const { resetKilnsForTests } = await import('@/lib/query/kilns');

// The roster the scope chips read through the shared kiln query.
let kilnEnv: ReturnType<typeof createTestQueryEnv>;

beforeEach(() => {
  localStorage.clear();
  resetKilnsForTests();
  kilnEnv = createTestQueryEnv({ 'GET /api/kilns': () => ({ kilns: [] }) });
});

afterEach(() => {
  cleanup();
  kilnEnv.restore();
  resetKilnsForTests();
  setConnectionStatus('connected');
  setError(null);
  mockRetryConnection.mockClear();
});

describe('ChatInput — the dropped stream is recoverable from here', () => {
  it('shows no banner while the stream is healthy', () => {
    render(() => <ChatInput />);
    expect(screen.queryByTestId('chat-connection-banner')).toBeNull();
  });

  it('a dropped stream gets the banner AND a retry that re-opens it', () => {
    setConnectionStatus('reconnecting');
    setError('Reconnecting…');
    render(() => <ChatInput />);

    expect(screen.getByTestId('chat-connection-banner')).toHaveTextContent('Reconnecting…');
    fireEvent.click(screen.getByTestId('chat-connection-retry'));
    expect(mockRetryConnection).toHaveBeenCalledTimes(1);
  });

  it('a daemon-side error gets NO retry — nothing here can re-issue it', () => {
    // "Failed to send: unknown model" is not cured by re-opening a socket, and
    // a retry control on it would be a cure for the wrong illness.
    setError('Failed to send: unknown model');
    render(() => <ChatInput />);

    expect(screen.queryByTestId('chat-connection-banner')).toBeNull();
    expect(screen.queryByTestId('chat-connection-retry')).toBeNull();
    expect(screen.getByTestId('chat-input-form')).toHaveTextContent('Failed to send: unknown model');
  });

  it('never stacks the two — one fault, one line', () => {
    setConnectionStatus('reconnecting');
    setError('Reconnecting…');
    render(() => <ChatInput />);
    const form = screen.getByTestId('chat-input-form');
    expect(form.textContent?.match(/Reconnecting…/g)).toHaveLength(1);
  });
});
